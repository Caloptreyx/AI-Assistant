//! One chat turn: ask the model, stream its answer, run the read tools it calls and turn every
//! change it wants into a pending action, until it answers without tools, waits for the user or
//! runs out of steps.

pub mod entry;
pub mod prompt;

use crate::{
    db,
    keys::KeySource,
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
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
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
    Action(Box<db::Action>),
    Error(String),
    /// The user stopped the turn.
    Stopped,
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
            Self::Stopped => ("stopped", json!({})),
            Self::Done { waiting } => ("done", json!({ "waiting": waiting })),
        };
        sse::Event::default().event(name).data(data.to_string())
    }
}

static BUSY: LazyLock<Mutex<HashMap<Uuid, CancellationToken>>> = LazyLock::new(Default::default);

fn busy_map() -> std::sync::MutexGuard<'static, HashMap<Uuid, CancellationToken>> {
    BUSY.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Held while something changes a conversation (a turn, a decision), so two of them never
/// interleave their entries.
pub struct Busy {
    conversation: Uuid,
    cancel: CancellationToken,
}

impl Busy {
    pub fn acquire(conversation: Uuid) -> Option<Self> {
        let mut busy = busy_map();
        if busy.contains_key(&conversation) {
            return None;
        }
        let cancel = CancellationToken::new();
        busy.insert(conversation, cancel.clone());
        Some(Self {
            conversation,
            cancel,
        })
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        busy_map().remove(&self.conversation);
    }
}

/// Asks the turn running in a conversation to stop. False when nothing runs there.
pub fn stop(conversation: Uuid) -> bool {
    match busy_map().get(&conversation) {
        Some(cancel) => {
            cancel.cancel();
            true
        }
        None => false,
    }
}

/// Everything a turn needs, gathered by the route before it starts streaming.
pub struct Turn {
    pub state: State,
    pub settings: Settings,
    pub provider: Provider,
    /// Whose key pays for this turn.
    pub source: KeySource,
    pub conversation: Uuid,
    pub user: Uuid,
    pub scope: Scope,
    pub permissions: PermissionManager,
    pub upstream: Arc<dyn Upstream>,
    pub system: String,
}

/// Runs the turn to its end (or until the user stops it), even when the browser goes away, and
/// reports through `events`. Stopping drops the model call in flight; changes never run here, so
/// nothing is left half done.
pub async fn run(turn: Turn, events: UnboundedSender<Event>, cancel: CancellationToken) {
    let waiting = tokio::select! {
        outcome = steps(&turn, &events) => match outcome {
            Ok(waiting) => waiting,
            Err(message) => {
                let _ = events.send(Event::Error(message));
                false
            }
        },
        () = cancel.cancelled() => {
            let _ = events.send(Event::Stopped);
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

/// Annotates the in-memory history and system prompt with actions the user later rolled back,
/// without mutating stored messages or breaking provider turn ordering.
pub(crate) fn apply_rollback_context(
    system: &str,
    history: Vec<Entry>,
    actions: &[db::Action],
) -> (String, Vec<Entry>) {
    let rolled_back: Vec<&db::Action> = actions
        .iter()
        .filter(|action| action.rollback_status == "rolled_back")
        .collect();
    if rolled_back.is_empty() {
        return (system.to_string(), history);
    }

    let by_call_id: HashMap<&str, &db::Action> = rolled_back
        .iter()
        .map(|action| (action.call_id.as_str(), *action))
        .collect();
    let mut annotated: HashSet<String> = HashSet::new();

    let history = history
        .into_iter()
        .map(|entry| match entry {
            Entry::ToolResult {
                call_id,
                name,
                mut content,
                is_error,
            } => {
                if let Some(action) = by_call_id.get(call_id.as_str()) {
                    annotated.insert(call_id.clone());
                    let detail = action
                        .rollback_result
                        .as_deref()
                        .filter(|text| !text.is_empty())
                        .unwrap_or("reverted to previous state");
                    content.push_str(&format!(
                        "\n\n[Note: the user later rolled back this action: {detail}]"
                    ));
                }
                Entry::ToolResult {
                    call_id,
                    name,
                    content,
                    is_error,
                }
            }
            other => other,
        })
        .collect();

    let out_of_window: Vec<&db::Action> = rolled_back
        .into_iter()
        .filter(|action| !annotated.contains(&action.call_id))
        .collect();
    let mut out_system = system.to_string();
    if !out_of_window.is_empty() {
        out_system.push_str(
            "\n\nEarlier actions in this conversation that the user later rolled back (do not assume these changes are still active):",
        );
        for action in out_of_window {
            let detail = action
                .rollback_result
                .as_deref()
                .or(action.rollback_summary.as_deref())
                .filter(|text| !text.is_empty())
                .unwrap_or("rolled back");
            out_system.push_str(&format!("\n- {} ({}): {}", action.title, action.tool, detail));
        }
        out_system.push('\n');
    }

    (out_system, history)
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
        let actions = db::actions(&turn.state.database, turn.conversation)
            .await
            .map_err(internal)?;
        let (system, history) = apply_rollback_context(&turn.system, history, &actions);

        let body = turn.provider.body(&system, &history, &specs);
        let completion = turn
            .provider
            .complete(&body, |text| {
                let _ = events.send(Event::Delta(text.to_string()));
            })
            .await?;
        let usage = completion.usage_or_estimate(&body);
        if let Err(err) = db::record_usage(
            &turn.state.database,
            turn.user,
            turn.source,
            1,
            usage.input,
            usage.output,
        )
        .await
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
                    let _ = events.send(Event::Action(Box::new(action)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use entry::ToolCall;

    fn sample_action(
        seq: i64,
        call_id: &str,
        tool: &str,
        rollback_status: &str,
        rollback_result: Option<&str>,
    ) -> db::Action {
        db::Action {
            uuid: Uuid::new_v4(),
            seq,
            call_id: call_id.into(),
            tool: tool.into(),
            title: crate::tools::find(tool)
                .map(|t| t.title.to_string())
                .unwrap_or_else(|| tool.into()),
            arguments: json!({}),
            status: "succeeded".into(),
            result: Some("ok".into()),
            created: Utc::now(),
            decided: Some(Utc::now()),
            rollback_status: rollback_status.into(),
            rollback_summary: None,
            rollback_result: rollback_result.map(str::to_string),
            rolled_back: (rollback_status == "rolled_back").then(Utc::now),
        }
    }

    fn tool_call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: json!({}),
            extra: None,
        }
    }

    #[test]
    fn apply_rollback_context_annotates_in_window_and_out_of_window_actions() {
        let actions = vec![
            // Out of window (its ToolResult was trimmed by history_limit)
            sample_action(
                1,
                "call_old",
                "rename_server",
                "rolled_back",
                Some("Restored server name to \"Survival\"."),
            ),
            // In window and rolled back
            sample_action(
                2,
                "call_recent",
                "edit_file",
                "rolled_back",
                Some("Restored /server.properties to its previous contents."),
            ),
            // In window and NOT rolled back
            sample_action(3, "call_active", "write_file", "available", None),
        ];

        let history = vec![
            Entry::User {
                text: "Edit server.properties".into(),
            },
            Entry::Assistant {
                text: String::new(),
                tool_calls: vec![
                    tool_call("call_recent", "edit_file"),
                    tool_call("call_active", "write_file"),
                ],
            },
            Entry::ToolResult {
                call_id: "call_recent".into(),
                name: "edit_file".into(),
                content: "Replaced 1 occurrence.".into(),
                is_error: false,
            },
            Entry::ToolResult {
                call_id: "call_active".into(),
                name: "write_file".into(),
                content: "Wrote 10 bytes.".into(),
                is_error: false,
            },
        ];

        let (system, annotated) = apply_rollback_context("Base prompt.", history.clone(), &actions);

        // Turn ordering and count are preserved.
        assert_eq!(annotated.len(), history.len());

        // In-window rolled-back ToolResult gets the note appended.
        match &annotated[2] {
            Entry::ToolResult {
                call_id, content, ..
            } => {
                assert_eq!(call_id, "call_recent");
                assert!(content.starts_with("Replaced 1 occurrence."));
                assert!(content.contains("[Note: the user later rolled back this action: Restored /server.properties"));
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }

        // Active action's ToolResult is untouched.
        assert_eq!(annotated[3], history[3]);

        // Out-of-window rolled-back action appears in system prompt, while in-window one does not duplicate there.
        assert!(system.starts_with("Base prompt."));
        assert!(system.contains("Rename the server (rename_server)"));
        assert!(system.contains("Restored server name to \"Survival\"."));
        assert!(!system.contains("edit_file"));
    }

    #[test]
    fn apply_rollback_context_is_noop_when_nothing_was_rolled_back() {
        let actions = vec![sample_action(1, "c1", "write_file", "available", None)];
        let history = vec![Entry::User { text: "hi".into() }];
        let (system, out) = apply_rollback_context("Prompt", history.clone(), &actions);
        assert_eq!(system, "Prompt");
        assert_eq!(out, history);
    }
}
