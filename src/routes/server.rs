//! The server chat, mounted at `/api/client/servers/{server}/ai-assistant`. The panel resolves the
//! server and the user's permissions on it before these handlers run; using the chat needs the
//! `ai-assistant.chat` server permission.

use super::{
    Caller, CreatePayload, Decision, RejectPayload, RenamePayload, RollbackPayload, Room,
    SendPayload,
};
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use shared::{GetState, Payload, State, response::ApiResponseResult};
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

fn room(caller: &Caller) -> Result<Room, shared::response::ApiResponse> {
    caller.permissions.has_server_permission("ai-assistant.chat")?;
    let server = caller
        .server
        .as_ref()
        .ok_or_else(|| super::error(StatusCode::NOT_FOUND, "server not found"))?;
    Ok(Room::Server {
        uuid: server.uuid,
        name: server.name.to_string(),
        egg: server.egg.name.to_string(),
    })
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

async fn show(
    state: GetState,
    caller: Caller,
    Path((_server, conversation)): Path<(String, Uuid)>,
) -> ApiResponseResult {
    super::get(&state, &caller, &room(&caller)?, conversation).await
}

async fn delete(
    state: GetState,
    caller: Caller,
    Path((_server, conversation)): Path<(String, Uuid)>,
) -> ApiResponseResult {
    super::delete(&state, &caller, &room(&caller)?, conversation).await
}

async fn rename(
    state: GetState,
    caller: Caller,
    Path((_server, conversation)): Path<(String, Uuid)>,
    Payload(data): Payload<RenamePayload>,
) -> ApiResponseResult {
    super::rename(&state, &caller, &room(&caller)?, conversation, data).await
}

async fn stop(
    state: GetState,
    caller: Caller,
    Path((_server, conversation)): Path<(String, Uuid)>,
) -> ApiResponseResult {
    super::stop(&state, &caller, &room(&caller)?, conversation).await
}

async fn send(
    state: GetState,
    caller: Caller,
    Path((_server, conversation)): Path<(String, Uuid)>,
    Payload(data): Payload<SendPayload>,
) -> ApiResponseResult {
    super::send(&state, &caller, &room(&caller)?, conversation, data).await
}

async fn resume(
    state: GetState,
    caller: Caller,
    Path((_server, conversation)): Path<(String, Uuid)>,
) -> ApiResponseResult {
    super::resume(&state, &caller, &room(&caller)?, conversation).await
}

async fn confirm(
    state: GetState,
    caller: Caller,
    Path((_server, conversation, action)): Path<(String, Uuid, Uuid)>,
) -> ApiResponseResult {
    let room = room(&caller)?;
    super::decide(&state, &caller, &room, conversation, action, Decision::Confirm).await
}

async fn reject(
    state: GetState,
    caller: Caller,
    Path((_server, conversation, action)): Path<(String, Uuid, Uuid)>,
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

async fn rollback(
    state: GetState,
    caller: Caller,
    Path((_server, conversation, action)): Path<(String, Uuid, Uuid)>,
    Payload(data): Payload<RollbackPayload>,
) -> ApiResponseResult {
    let room = room(&caller)?;
    super::rollback(&state, &caller, &room, conversation, action, data).await
}

async fn rollback_to(
    state: GetState,
    caller: Caller,
    Path((_server, conversation, action)): Path<(String, Uuid, Uuid)>,
    Payload(data): Payload<RollbackPayload>,
) -> ApiResponseResult {
    let room = room(&caller)?;
    super::rollback_to(&state, &caller, &room, conversation, action, data).await
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", get(info))
        .route("/conversations", get(list).post(create))
        .route(
            "/conversations/{conversation}",
            get(show).delete(delete).patch(rename),
        )
        .route("/conversations/{conversation}/messages", post(send))
        .route("/conversations/{conversation}/continue", post(resume))
        .route("/conversations/{conversation}/stop", post(stop))
        .route(
            "/conversations/{conversation}/actions/{action}/confirm",
            post(confirm),
        )
        .route(
            "/conversations/{conversation}/actions/{action}/reject",
            post(reject),
        )
        .route(
            "/conversations/{conversation}/actions/{action}/rollback",
            post(rollback),
        )
        .route(
            "/conversations/{conversation}/actions/{action}/rollback-to",
            post(rollback_to),
        )
        .with_state(state.clone())
}
