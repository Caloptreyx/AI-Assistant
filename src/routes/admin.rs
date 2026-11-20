//! The admin chat and the extension settings, mounted at
//! `/api/admin/extensions/dev.caloptreyx.aiassistant`. The chat needs the `ai-assistant.chat`
//! admin permission; the settings need `extensions.read` / `extensions.manage`.

use super::{Caller, CreatePayload, Decision, RejectPayload, Room, SendPayload};
use crate::{
    chat::entry::Entry,
    settings::{self, Settings, Update},
};
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde_json::json;
use shared::{
    ApiError, GetState, Payload, State,
    models::admin_activity::GetAdminActivityLogger,
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

fn room(caller: &Caller) -> Result<Room, ApiResponse> {
    caller.permissions.has_admin_permission("ai-assistant.chat")?;
    Ok(Room::Admin)
}

async fn info(state: GetState, caller: Caller) -> ApiResponseResult {
    super::info(&state, &caller, &room(&caller)?).await
}

async fn list(state: GetState, caller: Caller) -> ApiResponseResult {
    super::list(&state, &caller, &room(&caller)?).await
}

async fn create(
    state: GetState,
    caller: Caller,
    Payload(data): Payload<CreatePayload>,
) -> ApiResponseResult {
    super::create(&state, &caller, &room(&caller)?, data).await
}

async fn show(state: GetState, caller: Caller, Path(conversation): Path<Uuid>) -> ApiResponseResult {
    super::get(&state, &caller, &room(&caller)?, conversation).await
}

async fn delete(
    state: GetState,
    caller: Caller,
    Path(conversation): Path<Uuid>,
) -> ApiResponseResult {
    super::delete(&state, &caller, &room(&caller)?, conversation).await
}

async fn send(
    state: GetState,
    caller: Caller,
    Path(conversation): Path<Uuid>,
    Payload(data): Payload<SendPayload>,
) -> ApiResponseResult {
    super::send(&state, &caller, &room(&caller)?, conversation, data).await
}

async fn resume(
    state: GetState,
    caller: Caller,
    Path(conversation): Path<Uuid>,
) -> ApiResponseResult {
    super::resume(&state, &caller, &room(&caller)?, conversation).await
}

async fn confirm(
    state: GetState,
    caller: Caller,
    Path((conversation, action)): Path<(Uuid, Uuid)>,
) -> ApiResponseResult {
    let room = room(&caller)?;
    super::decide(&state, &caller, &room, conversation, action, Decision::Confirm).await
}

async fn reject(
    state: GetState,
    caller: Caller,
    Path((conversation, action)): Path<(Uuid, Uuid)>,
    Payload(data): Payload<RejectPayload>,
) -> ApiResponseResult {
    let room = room(&caller)?;
    super::decide(
        &state,
        &caller,
        &room,
        conversation,
        action,
        Decision::Reject(data),
    )
    .await
}

async fn get_settings(state: GetState, caller: Caller) -> ApiResponseResult {
    caller.permissions.has_admin_permission("extensions.read")?;
    let settings = settings::load(&state).await?;
    ApiResponse::new_serialized(json!({ "settings": settings.admin_view() })).ok()
}

async fn put_settings(
    state: GetState,
    caller: Caller,
    activity_logger: GetAdminActivityLogger,
    Payload(data): Payload<Update>,
) -> ApiResponseResult {
    caller.permissions.has_admin_permission("extensions.manage")?;
    if let Err(errors) = data.validate() {
        return ApiResponse::new_serialized(ApiError::new_strings_value(errors))
            .with_status(StatusCode::BAD_REQUEST)
            .ok();
    }
    let keys_changed = json!({
        "openai_api_key": data.openai_api_key.is_some(),
        "anthropic_api_key": data.anthropic_api_key.is_some(),
    });

    let view = {
        let mut panel_settings = state.settings.get_mut().await?;
        let settings: &mut Settings = panel_settings.get_mut_extension_settings(crate::PACKAGE)?;
        data.apply(settings);
        let view = settings.admin_view();
        panel_settings.save().await?;
        view
    };

    let model = match view.provider {
        settings::ProviderKind::Openai => view.openai_model.clone(),
        settings::ProviderKind::Anthropic => view.anthropic_model.clone(),
    };
    activity_logger
        .log(
            "ai-assistant:settings.update",
            json!({
                "provider": view.provider,
                "openai_base_url": view.openai_base_url,
                "model": model,
                "keys_changed": keys_changed,
            }),
        )
        .await;

    ApiResponse::new_serialized(json!({ "settings": view })).ok()
}

/// Sends a tiny request with the saved settings, so the admin sees right away whether the key,
/// URL and model work.
async fn test_settings(state: GetState, caller: Caller) -> ApiResponseResult {
    caller.permissions.has_admin_permission("extensions.manage")?;
    let settings = settings::load(&state).await?;
    let provider = match settings.provider() {
        Ok(provider) => provider,
        Err(problem) => {
            return ApiResponse::new_serialized(json!({ "ok": false, "error": problem })).ok();
        }
    };
    let history = [Entry::User {
        text: "Reply with the single word OK.".into(),
    }];
    let body = provider.body("You are a connection test.", &history, &[]);
    match provider.complete(&body, |_| {}).await {
        Ok(completion) => ApiResponse::new_serialized(json!({
            "ok": true,
            "reply": completion.text.chars().take(200).collect::<String>(),
            "model": provider.model,
            "usage": { "input": completion.usage.input, "output": completion.usage.output },
        }))
        .ok(),
        Err(err) => ApiResponse::new_serialized(json!({ "ok": false, "error": err })).ok(),
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", get(info))
        .route("/conversations", get(list).post(create))
        .route("/conversations/{conversation}", get(show).delete(delete))
        .route("/conversations/{conversation}/messages", post(send))
        .route("/conversations/{conversation}/continue", post(resume))
        .route(
            "/conversations/{conversation}/actions/{action}/confirm",
            post(confirm),
        )
        .route(
            "/conversations/{conversation}/actions/{action}/reject",
            post(reject),
        )
        .route("/settings", get(get_settings).put(put_settings))
        .route("/settings/test", post(test_settings))
        .with_state(state.clone())
}
