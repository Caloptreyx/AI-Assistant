//! One chat turn: ask the model, stream its answer, run the read tools it calls and turn every
//! change it wants into a pending action, until it answers without tools, waits for the user or
//! runs out of steps.

pub mod entry;
pub mod prompt;

use crate::{
    db,
    provider::{Provider, ToolSpec},
    settings::Settings,
    tools::{self, Permissions, Scope, Unconfirmed, output::clip},
    upstream::Upstream,
};
use axum::response::sse;
use entry::Entry;
use serde_json::json;
use shared::{State, models::user::PermissionManager};
use std::{
    collections::HashSet,
    sync::{Arc, LazyLock, Mutex},
};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

/// What the browser receives while a turn runs.
pub enum Event {
    /// Text the model is writing right now.
    Delta(String),
    /// An entry was stored (the finished assistant message, a tool result).
    Message(db::Message),
    /// A read tool started.
    Tool { call_id: String, name: String },
    /// The model proposed a change; it waits for the user.
    Action(db::Action),
    Error(String),
    /// The turn is over. `waiting` when actions wait for the user's decision.
    Done { waiting: bool },
}

impl Event {
    pub fn into_sse(self) -> sse::Event {
        let (name, data) = match self {
            Self::Delta(text) => ("delta", json!({ "text": text })),
            Self::Message(message) => ("message", json!(message)),
            Self::Tool { call_id, name } => ("tool", json!({ "call_id": call_id, "name": name })),
            Self::Action(action) => ("action", json!(action)),
            Self::Error(message) => ("error", json!({ "message": message })),
            Self::Done { waiting } => ("done", json!({ "waiting": waiting })),
        };
        sse::Event::default().event(name).data(data.to_string())
    }
}

static BUSY: LazyLock<Mutex<HashSet<Uuid>>> = LazyLock::new(Default::default);

/// Held while something changes a conversation (a turn, a decision), so two of them never
/// interleave their entries.
pub struct Busy(Uuid);

impl Busy {
    pub fn acquire(conversation: Uuid) -> Option<Self> {
        let mut busy = BUSY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        busy.insert(conversation).then_some(Self(conversation))
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.0);
    }
}

/// Everything a turn needs, gathered by the route before it starts streaming.
pub struct Turn {
    pub state: State,
    pub settings: Settings,
    pub provider: Provider,
    pub conversation: Uuid,
    pub user: Uuid,
    pub scope: Scope,
    pub permissions: PermissionManager,
    pub upstream: Arc<dyn Upstream>,
    pub system: String,
}

/// Runs the turn to its end, even when the browser goes away, and reports through `events`.
pub async fn run(turn: Turn, events: UnboundedSender<Event>) {
    let waiting = match steps(&turn, &events).await {
        Ok(waiting) => waiting,
        Err(message) => {
            let _ = events.send(Event::Error(message));
            false
        }
    };
    if let Err(err) = db::touch_conversation(&turn.state.database, turn.conversation, None).await {
        tracing::warn!("ai assistant: could not update conversation: {err}");
    }
    let _ = events.send(Event::Done { waiting });
}

fn internal(err: impl std::fmt::Display) -> String {
    tracing::error!("ai assistant: {err}");
    "The assistant hit an internal error; see the panel log.".into()
}

/// Returns whether actions now wait for the user.
async fn steps(turn: &Turn, events: &UnboundedSender<Event>) -> Result<bool, String> {
    let permissions: &dyn Permissions = &turn.permissions;
    let specs: Vec<ToolSpec> = tools::available(&turn.scope, permissions)
        .into_iter()
        .map(|tool| tool.spec(&turn.scope))
        .collect();
    let limit = turn.settings.tool_output_limit.max(1000) as usize;
    let max_steps = turn.settings.max_tool_steps.clamp(1, 25);

    for _ in 0..max_steps {
        let history: Vec<Entry> = db::messages(&turn.state.database, turn.conversation)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|message| message.entry)
            .collect();
        let history = entry::repair(entry::recent(
            history,
            turn.settings.history_limit.max(4) as usize,
        ));

        let body = turn.provider.body(&turn.system, &history, &specs);
        let completion = turn
            .provider
            .complete(&body, |text| {
                let _ = events.send(Event::Delta(text.to_string()));
            })
            .await?;
        let usage = completion.usage_or_estimate(&body);
        if let Err(err) =
            db::record_usage(&turn.state.database, turn.user, 1, usage.input, usage.output).await
        {
            tracing::warn!("ai assistant: could not record usage: {err}");
        }

        if completion.text.trim().is_empty() && completion.tool_calls.is_empty() {
            return Err("The model returned an empty answer.".into());
        }
        let calls = completion.tool_calls.clone();
        let message = db::append(
            &turn.state.database,
            turn.conversation,
            Entry::Assistant {
                text: completion.text,
                tool_calls: completion.tool_calls,
            },
        )
        .await
        .map_err(internal)?;
        let _ = events.send(Event::Message(message));
        if calls.is_empty() {
            return Ok(false);
        }

        let mut waiting = false;
        for call in &calls {
            let _ = events.send(Event::Tool {
                call_id: call.id.clone(),
                name: call.name.clone(),
            });
            match tools::run_unconfirmed(
                turn.upstream.clone(),
                &turn.scope,
                permissions,
                &call.name,
                &call.arguments,
            )
            .await
            {
                Unconfirmed::Ran(outcome) => {
                    let outcome = outcome
                        .map(|text| clip(&text, limit))
                        .map_err(|text| clip(&text, limit));
                    let message = db::append(
                        &turn.state.database,
                        turn.conversation,
                        Entry::tool_result(call, outcome),
                    )
                    .await
                    .map_err(internal)?;
                    let _ = events.send(Event::Message(message));
                }
                Unconfirmed::NeedsConfirmation(tool) => {
                    tracing::debug!(
                        "ai assistant: {} proposed, waiting for confirmation",
                        tool.name
                    );
                    let action = db::create_action(&turn.state.database, turn.conversation, call)
                        .await
                        .map_err(internal)?;
                    let _ = events.send(Event::Action(action));
                    waiting = true;
                }
            }
        }
        if waiting {
            return Ok(true);
        }
    }

    Err(format!(
        "The assistant stopped after {max_steps} steps in a row. Send a message to let it continue."
    ))
}
