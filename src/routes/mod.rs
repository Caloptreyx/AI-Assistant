//! The chat API, shared by the server chat (`/api/client/servers/{server}/ai-assistant`) and the
//! admin chat (`/api/admin/extensions/dev.caloptreyx.aiassistant`). The route modules only check
//! who may use which chat and hand over a [`Room`].

pub mod admin;
pub mod server;

use crate::{
    chat::{
        self, Busy, Event, Turn,
        entry::{Entry, ToolCall},
        prompt::{self, Place},
    },
    db,
    settings::{self, Settings},
    tools::{self, Effect, Scope, output::clip},
    upstream::{LiveUpstream, Upstream},
};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderName, HeaderValue, StatusCode, header, request::Parts},
    response::sse::{KeepAlive, Sse},
};
use serde::Deserialize;
use serde_json::json;
use shared::{
    State,
    models::{
        server::Server,
        user::{AuthMethod, PermissionManager, User, UserImpersonator},
    },
    response::{ApiResponse, ApiResponseResult},
};
use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use uuid::Uuid;

const MAX_MESSAGE: usize = 8000;

const FORWARDED_HEADERS: [HeaderName; 3] = [
    HeaderName::from_static("x-forwarded-for"),
    HeaderName::from_static("x-real-ip"),
    header::USER_AGENT,
];

fn error(status: StatusCode, message: impl AsRef<str>) -> ApiResponse {
    ApiResponse::error(message).with_status(status)
}

/// The signed-in user behind a chat request, with what is needed to send panel requests as them.
pub struct Caller {
    pub user: User,
    /// The administrator who is impersonating `user`, if any.
    pub impersonator: Option<User>,
    pub auth: AuthMethod,
    pub permissions: PermissionManager,
    /// Set on server routes.
    pub server: Option<Server>,
    addr: SocketAddr,
    forward: Vec<(HeaderName, HeaderValue)>,
}

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = ApiResponse;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let missing = || error(StatusCode::UNAUTHORIZED, "not signed in");
        let user = parts.extensions.get::<User>().cloned().ok_or_else(missing)?;
        let auth = parts
            .extensions
            .get::<Arc<AuthMethod>>()
            .map(|auth| auth.as_ref().clone())
            .ok_or_else(missing)?;
        let permissions = parts
            .extensions
            .get::<PermissionManager>()
            .cloned()
            .ok_or_else(missing)?;
        let impersonator = parts
            .extensions
            .get::<Option<UserImpersonator>>()
            .cloned()
            .flatten()
            .map(|impersonator| impersonator.0);
        let addr = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0)
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));
        let forward = FORWARDED_HEADERS
            .iter()
            .filter_map(|name| {
                parts
                    .headers
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
            })
            .collect();

        Ok(Self {
            user,
            impersonator,
            auth,
            permissions,
            server: parts.extensions.get::<Server>().cloned(),
            addr,
            forward,
        })
    }
}

impl Caller {
    /// Sends panel requests through the panel's router as this user. While an administrator
    /// impersonates someone, requests go out as the administrator with the impersonation header,
    /// so the panel applies its impersonation rules exactly as for the browser.
    fn upstream(&self, state: &State) -> Arc<dyn Upstream> {
        let mut forward = self.forward.clone();
        let user = match &self.impersonator {
            Some(admin) => {
                if let Ok(value) = HeaderValue::from_str(&self.user.uuid.to_string()) {
                    forward.push((HeaderName::from_static("calagopus-user"), value));
                }
                admin.clone()
            }
            None => self.user.clone(),
        };
        Arc::new(LiveUpstream {
            state: state.clone(),
            user,
            auth: self.auth.clone(),
            addr: self.addr,
            forward,
        })
    }
}

/// Which chat a request is about.
pub enum Room {
    Server {
        uuid: Uuid,
        name: String,
        egg: String,
    },
    Admin,
}

impl Room {
    fn scope(&self) -> Scope {
        match self {
            Self::Server { uuid, .. } => Scope::Server(uuid.to_string()),
            Self::Admin => Scope::Admin,
        }
    }

    /// The `server_uuid` of this room's conversations.
    fn server(&self) -> Option<Uuid> {
        match self {
            Self::Server { uuid, .. } => Some(*uuid),
            Self::Admin => None,
        }
    }

    fn enabled(&self, settings: &Settings) -> bool {
        match self {
            Self::Server { .. } => settings.server_chat_enabled,
            Self::Admin => settings.admin_chat_enabled,
        }
    }
}

async fn enabled_settings(state: &State, room: &Room) -> Result<Settings, ApiResponse> {
    let settings = settings::load(state).await?;
    if !room.enabled(&settings) {
        return Err(error(
            StatusCode::FORBIDDEN,
            "The administrator turned the AI assistant off here.",
        ));
    }
    Ok(settings)
}

async fn find_conversation(
    state: &State,
    caller: &Caller,
    room: &Room,
    uuid: Uuid,
) -> Result<db::Conversation, ApiResponse> {
    db::conversation(&state.database, caller.user.uuid, room.server(), uuid)
        .await?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "conversation not found"))
}

fn busy(conversation: Uuid) -> Result<Busy, ApiResponse> {
    Busy::acquire(conversation).ok_or_else(|| {
        error(
            StatusCode::CONFLICT,
            "The assistant is still working on this conversation.",
        )
    })
}

/// The per-user message rate and daily token limits.
async fn check_limits(state: &State, caller: &Caller, settings: &Settings) -> Result<(), ApiResponse> {
    if settings.requests_per_hour > 0 {
        state
            .cache
            .ratelimit(
                "dev.caloptreyx.aiassistant::messages",
                settings.requests_per_hour as u64,
                3600,
                caller.user.uuid.to_string(),
            )
            .await?;
    }
    if settings.daily_token_limit > 0
        && db::tokens_today(&state.database, caller.user.uuid).await? >= settings.daily_token_limit
    {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "You used up today's AI assistant token allowance. It resets at midnight (database time).",
        ));
    }
    Ok(())
}

pub async fn info(state: &State, caller: &Caller, room: &Room) -> ApiResponseResult {
    let settings = settings::load(state).await?;
    let problem = settings.provider().err();
    let scope = room.scope();
    let tools: Vec<_> = tools::available(&scope, &caller.permissions)
        .into_iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "confirm": tool.effect == Effect::Change,
            })
        })
        .collect();

    let tokens_today = db::tokens_today(&state.database, caller.user.uuid).await?;

    ApiResponse::new_serialized(json!({
        "enabled": room.enabled(&settings),
        "configured": problem.is_none(),
        "problem": problem,
        "provider": settings.provider,
        "model": settings.model(),
        "tools": tools,
        "usage": {
            "tokens_today": tokens_today,
            "daily_token_limit": settings.daily_token_limit,
            "requests_per_hour": settings.requests_per_hour,
        },
    }))
    .ok()
}

pub async fn list(state: &State, caller: &Caller, room: &Room) -> ApiResponseResult {
    let conversations = db::conversations(&state.database, caller.user.uuid, room.server()).await?;
    ApiResponse::new_serialized(json!({ "conversations": conversations })).ok()
}

#[derive(Deserialize)]
pub struct CreatePayload {
    #[serde(default)]
    title: Option<String>,
}

pub async fn create(
    state: &State,
    caller: &Caller,
    room: &Room,
    data: CreatePayload,
) -> ApiResponseResult {
    enabled_settings(state, room).await?;
    let title = data
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| title.chars().take(255).collect::<String>())
        .unwrap_or_else(|| "New conversation".into());
    let conversation =
        db::create_conversation(&state.database, caller.user.uuid, room.server(), &title).await?;
    ApiResponse::new_serialized(json!({ "conversation": conversation })).ok()
}

pub async fn get(state: &State, caller: &Caller, room: &Room, uuid: Uuid) -> ApiResponseResult {
    let conversation = find_conversation(state, caller, room, uuid).await?;
    db::fail_interrupted_actions(&state.database, uuid).await?;
    ApiResponse::new_serialized(json!({
        "conversation": conversation,
        "messages": db::messages(&state.database, uuid).await?,
        "actions": db::actions(&state.database, uuid).await?,
    }))
    .ok()
}

pub async fn delete(state: &State, caller: &Caller, room: &Room, uuid: Uuid) -> ApiResponseResult {
    find_conversation(state, caller, room, uuid).await?;
    let _busy = busy(uuid)?;
    db::delete_conversation(&state.database, uuid).await?;
    ApiResponse::new_serialized(json!({})).ok()
}

/// Gathers everything for a turn and streams it as server-sent events. `prelude` is sent first.
async fn start_turn(
    state: &State,
    caller: &Caller,
    room: &Room,
    settings: Settings,
    conversation: Uuid,
    guard: Busy,
    prelude: Vec<Event>,
) -> ApiResponseResult {
    let provider = settings
        .provider()
        .map_err(|problem| error(StatusCode::CONFLICT, problem))?;
    let panel = {
        let panel_settings = state.settings.get().await?;
        panel_settings.app.name.to_string()
    };
    let scope = room.scope();
    let server_uuid = room.server().map(|uuid| uuid.to_string()).unwrap_or_default();
    let system = {
        let tools = tools::available(&scope, &caller.permissions);
        let place = match room {
            Room::Server { name, egg, .. } => Place::Server {
                name,
                uuid: &server_uuid,
                egg,
            },
            Room::Admin => Place::Admin,
        };
        prompt::system(
            &panel,
            &caller.user.username,
            &place,
            &tools,
            &settings.instructions,
        )
    };

    let turn = Turn {
        state: state.clone(),
        provider,
        conversation,
        user: caller.user.uuid,
        scope,
        permissions: caller.permissions.clone(),
        upstream: caller.upstream(state),
        system,
        settings,
    };

    let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
    for event in prelude {
        let _ = events.send(event);
    }
    tokio::spawn(async move {
        let _guard = guard;
        chat::run(turn, events).await;
    });

    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver
            .recv()
            .await
            .map(|event| (Ok::<_, Infallible>(event.into_sse()), receiver))
    });
    Ok(
        ApiResponse::new_response(Sse::new(stream).keep_alive(KeepAlive::default()))
            .with_header("Cache-Control", "no-cache")
            .with_header("X-Accel-Buffering", "no"),
    )
}

/// Stores the result of a decided action as the tool result the model will see.
async fn store_result(
    state: &State,
    conversation: Uuid,
    action: &db::Action,
    outcome: Result<String, String>,
) -> Result<db::Message, ApiResponse> {
    let call = ToolCall {
        id: action.call_id.clone(),
        name: action.tool.clone(),
        arguments: action.arguments.clone(),
    };
    Ok(db::append(&state.database, conversation, Entry::tool_result(&call, outcome)).await?)
}

/// Rejects every pending action of the conversation, for example because the user wrote a new
/// message instead of deciding.
async fn reject_pending(
    state: &State,
    conversation: Uuid,
    reason: &str,
) -> Result<Vec<Event>, ApiResponse> {
    let mut events = Vec::new();
    for action in db::actions(&state.database, conversation).await? {
        if action.status != "pending" {
            continue;
        }
        let Some(action) =
            db::decide_action(&state.database, conversation, action.uuid, "rejected").await?
        else {
            continue;
        };
        let message = store_result(state, conversation, &action, Err(reason.to_string())).await?;
        if let Some(action) =
            db::finish_action(&state.database, action.uuid, "rejected", reason).await?
        {
            events.push(Event::Action(action));
        }
        events.push(Event::Message(message));
    }
    Ok(events)
}

#[derive(Deserialize)]
pub struct SendPayload {
    text: String,
}

pub async fn send(
    state: &State,
    caller: &Caller,
    room: &Room,
    uuid: Uuid,
    data: SendPayload,
) -> ApiResponseResult {
    let text = data.text.trim();
    if text.is_empty() || text.chars().count() > MAX_MESSAGE {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("A message must be 1 to {MAX_MESSAGE} characters long."),
        ));
    }
    let settings = enabled_settings(state, room).await?;
    if let Err(problem) = settings.provider() {
        return Err(error(StatusCode::CONFLICT, problem));
    }
    find_conversation(state, caller, room, uuid).await?;
    let guard = busy(uuid)?;
    check_limits(state, caller, &settings).await?;

    let mut prelude = reject_pending(
        state,
        uuid,
        "The user did not confirm this action and sent a new message instead.",
    )
    .await?;
    let first = db::messages(&state.database, uuid).await?.is_empty();
    let message = db::append(
        &state.database,
        uuid,
        Entry::User {
            text: text.to_string(),
        },
    )
    .await?;
    prelude.push(Event::Message(message));
    if first {
        let title: String = text.chars().take(80).collect();
        db::touch_conversation(&state.database, uuid, Some(&title)).await?;
    }

    start_turn(state, caller, room, settings, uuid, guard, prelude).await
}

/// Lets the model go on after the user decided every proposed action (or after an error).
pub async fn resume(state: &State, caller: &Caller, room: &Room, uuid: Uuid) -> ApiResponseResult {
    let settings = enabled_settings(state, room).await?;
    find_conversation(state, caller, room, uuid).await?;
    let guard = busy(uuid)?;

    if db::actions(&state.database, uuid)
        .await?
        .iter()
        .any(|action| action.status == "pending" || action.status == "running")
    {
        return Err(error(
            StatusCode::CONFLICT,
            "Confirm or reject the proposed actions first.",
        ));
    }
    let last = db::messages(&state.database, uuid).await?.pop();
    if !matches!(
        last.map(|message| message.entry),
        Some(Entry::User { .. } | Entry::ToolResult { .. })
    ) {
        return Err(error(
            StatusCode::CONFLICT,
            "The assistant has already answered; send a new message.",
        ));
    }
    check_limits(state, caller, &settings).await?;

    start_turn(state, caller, room, settings, uuid, guard, Vec::new()).await
}

#[derive(Deserialize)]
pub struct RejectPayload {
    #[serde(default)]
    reason: Option<String>,
}

pub enum Decision {
    Confirm,
    Reject(RejectPayload),
}

/// Confirms (runs) or rejects one proposed action.
pub async fn decide(
    state: &State,
    caller: &Caller,
    room: &Room,
    uuid: Uuid,
    action: Uuid,
    decision: Decision,
) -> ApiResponseResult {
    let settings = enabled_settings(state, room).await?;
    find_conversation(state, caller, room, uuid).await?;
    let _guard = busy(uuid)?;
    let not_pending = || error(StatusCode::CONFLICT, "This action was already decided.");

    let (action, message) = match decision {
        Decision::Confirm => {
            let action = db::decide_action(&state.database, uuid, action, "running")
                .await?
                .ok_or_else(not_pending)?;
            let limit = settings.tool_output_limit.max(1000) as usize;
            let outcome = tools::run_confirmed(
                caller.upstream(state),
                &room.scope(),
                &caller.permissions,
                &action.tool,
                &action.arguments,
            )
            .await
            .map(|text| clip(&text, limit))
            .map_err(|text| clip(&text, limit));
            let (status, text) = match &outcome {
                Ok(text) => ("succeeded", text.clone()),
                Err(text) => ("failed", text.clone()),
            };
            let message = store_result(state, uuid, &action, outcome).await?;
            let action = db::finish_action(&state.database, action.uuid, status, &text)
                .await?
                .unwrap_or(action);
            (action, message)
        }
        Decision::Reject(reject) => {
            let action = db::decide_action(&state.database, uuid, action, "rejected")
                .await?
                .ok_or_else(not_pending)?;
            let reason = reject
                .reason
                .map(|reason| reason.trim().chars().take(1000).collect::<String>())
                .filter(|reason| !reason.is_empty());
            let text = match reason {
                Some(reason) => format!("The user rejected this action: {reason}"),
                None => "The user rejected this action.".to_string(),
            };
            let message = store_result(state, uuid, &action, Err(text.clone())).await?;
            let action = db::finish_action(&state.database, action.uuid, "rejected", &text)
                .await?
                .unwrap_or(action);
            (action, message)
        }
    };
    db::touch_conversation(&state.database, uuid, None).await?;

    let pending = db::actions(&state.database, uuid)
        .await?
        .iter()
        .filter(|action| action.status == "pending")
        .count();
    ApiResponse::new_serialized(json!({
        "action": action,
        "message": message,
        "pending": pending,
    }))
    .ok()
}
