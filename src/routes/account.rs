//! A user's own provider key ("bring your own key"), at
//! `/api/client/extensions/dev.caloptreyx.aiassistant/key`. Needs the `ai-assistant.key` user
//! permission, which sessions always have and API keys only when scoped to it. The key is stored
//! encrypted and only ever leaves the backend masked.

use super::{Caller, decrypt_user_key, error, probe};
use crate::{
    db,
    keys::{self, CUSTOM, PRESETS, UserKeyMode},
    settings::{self, SecretView, Settings},
    ssrf,
};
use axum::{
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use shared::{
    GetState, Payload, State,
    models::user_activity::GetUserActivityLogger,
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

const PERMISSION: &str = "ai-assistant.key";

async fn enabled(state: &State, caller: &Caller) -> Result<Settings, ApiResponse> {
    caller.permissions.has_user_permission(PERMISSION)?;
    let settings = settings::load(state).await?;
    if settings.user_keys == UserKeyMode::Off {
        return Err(error(
            StatusCode::FORBIDDEN,
            "The administrator does not allow own API keys.",
        ));
    }
    Ok(settings)
}

async fn view(state: &State, caller: &Caller, settings: &Settings) -> Result<Value, ApiResponse> {
    let key = match db::user_key(&state.database, caller.user.uuid).await? {
        Some(stored) => {
            let api_key = state
                .database
                .decrypt_base64(&stored.api_key_encrypted)
                .await
                .map(|key| key.to_string())
                .unwrap_or_default();
            Some(json!({
                "preset": stored.preset,
                "base_url": stored.base_url,
                "model": stored.model,
                "api_key": SecretView::of(&api_key),
                "updated": stored.updated,
            }))
        }
        None => None,
    };
    Ok(json!({
        "mode": settings.user_keys,
        "allow_custom_base_urls": settings.allow_custom_base_urls,
        "presets": PRESETS,
        "key": key,
    }))
}

async fn show(state: GetState, caller: Caller) -> ApiResponseResult {
    caller.permissions.has_user_permission(PERMISSION)?;
    let settings = settings::load(&state).await?;
    ApiResponse::new_serialized(view(&state, &caller, &settings).await?).ok()
}

#[derive(Deserialize)]
pub struct SavePayload {
    preset: String,
    #[serde(default)]
    base_url: Option<String>,
    model: String,
    /// Left out (or empty) keeps the stored key.
    #[serde(default)]
    api_key: Option<String>,
}

async fn save(
    state: GetState,
    caller: Caller,
    activity_logger: GetUserActivityLogger,
    Payload(data): Payload<SavePayload>,
) -> ApiResponseResult {
    let settings = enabled(&state, &caller).await?;
    caller.permissions.deny_while_impersonating()?;
    let bad = |message: String| error(StatusCode::BAD_REQUEST, message);

    let model = data.model.trim();
    if model.is_empty() || model.chars().count() > 255 || model.chars().any(char::is_control) {
        return Err(bad("The model must be 1 to 255 characters long.".into()));
    }
    let base_url = if data.preset == CUSTOM {
        if !settings.allow_custom_base_urls {
            return Err(bad(
                "The administrator does not allow custom provider URLs; pick one of the listed providers."
                    .into(),
            ));
        }
        let url = data
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .ok_or_else(|| bad("Enter the base URL of your provider.".into()))?;
        ssrf::vet(url, &ssrf::Dns).await.map_err(bad)?;
        Some(url.trim_end_matches('/').to_string())
    } else if keys::preset(&data.preset).is_some() {
        None
    } else {
        return Err(bad(format!("Unknown provider \"{}\".", data.preset)));
    };

    let api_key = match data.api_key.as_deref().map(str::trim).filter(|key| !key.is_empty()) {
        Some(key) if keys::valid_api_key(key) => {
            Some(state.database.encrypt_base64(key.to_string()).await?.to_string())
        }
        Some(_) => {
            return Err(bad(
                "The API key must be at most 1024 characters without spaces or line breaks."
                    .into(),
            ));
        }
        None => None,
    };
    if api_key.is_none() && db::user_key(&state.database, caller.user.uuid).await?.is_none() {
        return Err(bad("Enter your API key.".into()));
    }

    db::save_user_key(
        &state.database,
        caller.user.uuid,
        &data.preset,
        base_url.as_deref(),
        model,
        api_key.as_deref(),
    )
    .await?;
    activity_logger
        .log(
            "account:ai-assistant.key.update",
            json!({ "preset": data.preset, "model": model, "key_changed": api_key.is_some() }),
        )
        .await;

    ApiResponse::new_serialized(view(&state, &caller, &settings).await?).ok()
}

async fn remove(
    state: GetState,
    caller: Caller,
    activity_logger: GetUserActivityLogger,
) -> ApiResponseResult {
    caller.permissions.has_user_permission(PERMISSION)?;
    caller.permissions.deny_while_impersonating()?;
    db::delete_user_key(&state.database, caller.user.uuid).await?;
    activity_logger
        .log("account:ai-assistant.key.delete", json!({}))
        .await;
    let settings = settings::load(&state).await?;
    ApiResponse::new_serialized(view(&state, &caller, &settings).await?).ok()
}

/// Sends a one-word request with the stored key.
async fn test(state: GetState, caller: Caller) -> ApiResponseResult {
    let settings = enabled(&state, &caller).await?;
    state
        .cache
        .ratelimit(
            "dev.caloptreyx.aiassistant::key-test",
            10,
            3600,
            caller.user.uuid.to_string(),
        )
        .await?;
    let stored = db::user_key(&state.database, caller.user.uuid)
        .await?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Save a key first."))?;
    let key = decrypt_user_key(&state, stored).await?;
    let outcome = match key.provider(&settings, &ssrf::Dns).await {
        Ok(provider) => probe(&provider).await,
        Err(problem) => json!({ "ok": false, "error": problem }),
    };
    ApiResponse::new_serialized(outcome).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/key", get(show).put(save).delete(remove))
        .route("/key/test", post(test))
        .with_state(state.clone())
}
