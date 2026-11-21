//! The provider-neutral chat history. Each entry is one row in the messages table; the provider
//! adapters turn a list of them into OpenAI or Anthropic requests.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments as the model sent them: normally an object, a string when the model sent
    /// JSON that does not parse.
    pub arguments: Value,
    /// Provider data that has to go back with the call in later requests: the `extra_content`
    /// with the thought signature that Gemini's OpenAI-compatible endpoint attaches to calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    User {
        text: String,
    },
    Assistant {
        text: String,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        call_id: String,
        name: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl Entry {
    pub fn tool_result(call: &ToolCall, outcome: Result<String, String>) -> Self {
        let (content, is_error) = match outcome {
            Ok(content) => (content, false),
            Err(content) => (content, true),
        };
        Self::ToolResult {
            call_id: call.id.clone(),
            name: call.name.clone(),
            content,
            is_error,
        }
    }
}

/// The last `limit` entries, starting at a user message so that no tool result is separated from
/// the call it answers.
pub fn recent(entries: Vec<Entry>, limit: usize) -> Vec<Entry> {
    let start = entries.len().saturating_sub(limit.max(1));
    let start = entries[start..]
        .iter()
        .position(|entry| matches!(entry, Entry::User { .. }))
        .map(|offset| start + offset)
        // A single exchange longer than the limit: keep it from its user message.
        .or_else(|| {
            entries[..start]
                .iter()
                .rposition(|entry| matches!(entry, Entry::User { .. }))
        })
        .unwrap_or(0);
    entries.into_iter().skip(start).collect()
}

/// Makes a history valid for both providers: every tool call is directly followed by exactly one
/// result for it, and no result appears without its call. Calls without a recorded result (an
/// action that was still pending, a restart in the middle of a tool) get a placeholder result.
pub fn repair(entries: Vec<Entry>) -> Vec<Entry> {
    let mut repaired = Vec::with_capacity(entries.len());
    let mut open: Vec<ToolCall> = Vec::new();
    let mut answered: HashSet<String> = HashSet::new();

    let close = |repaired: &mut Vec<Entry>, open: &mut Vec<ToolCall>, answered: &HashSet<String>| {
        for call in open.drain(..) {
            if !answered.contains(&call.id) {
                repaired.push(Entry::ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: "No result was recorded for this call.".into(),
                    is_error: true,
                });
            }
        }
    };

    for entry in entries {
        match entry {
            Entry::ToolResult { ref call_id, .. } => {
                if open.iter().any(|call| &call.id == call_id) && answered.insert(call_id.clone())
                {
                    repaired.push(entry);
                }
            }
            Entry::Assistant { ref tool_calls, .. } => {
                close(&mut repaired, &mut open, &answered);
                answered.clear();
                open = tool_calls.clone();
                repaired.push(entry);
            }
            Entry::User { .. } => {
                close(&mut repaired, &mut open, &answered);
                answered.clear();
                repaired.push(entry);
            }
        }
    }
    close(&mut repaired, &mut open, &answered);

    repaired
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(text: &str) -> Entry {
        Entry::User { text: text.into() }
    }

    fn call(id: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: "read_console".into(),
            arguments: json!({}),
            extra: None,
        }
    }

    fn assistant(calls: &[&str]) -> Entry {
        Entry::Assistant {
            text: String::new(),
            tool_calls: calls.iter().map(|id| call(id)).collect(),
        }
    }

    fn result(id: &str) -> Entry {
        Entry::tool_result(&call(id), Ok("ok".into()))
    }

    #[test]
    fn entries_round_trip_through_json() {
        let entries = vec![user("hi"), assistant(&["a"]), result("a")];
        for entry in entries {
            let text = serde_json::to_string(&entry).unwrap();
            assert_eq!(serde_json::from_str::<Entry>(&text).unwrap(), entry);
        }
        let text = serde_json::to_value(user("hi")).unwrap();
        assert_eq!(text, json!({ "type": "user", "text": "hi" }));
    }

    #[test]
    fn calls_keep_provider_data_and_older_calls_still_read() {
        let mut call = call("a");
        call.extra = Some(json!({ "google": { "thought_signature": "c2ln" } }));
        let entry = Entry::Assistant {
            text: String::new(),
            tool_calls: vec![call],
        };
        let stored = serde_json::to_string(&entry).unwrap();
        assert_eq!(serde_json::from_str::<Entry>(&stored).unwrap(), entry);

        // Rows written before `extra` existed read fine, and calls without it store as before.
        let old = json!({ "type": "assistant", "text": "", "tool_calls": [{ "id": "a", "name": "read_console", "arguments": {} }] });
        let entry: Entry = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(entry, assistant(&["a"]));
        assert_eq!(serde_json::to_value(&entry).unwrap(), old);
    }

    #[test]
    fn recent_starts_at_a_user_message() {
        let entries = vec![
            user("1"),
            assistant(&["a"]),
            result("a"),
            user("2"),
            assistant(&[]),
        ];
        // The last three would start with a tool result; it starts at "2" instead.
        assert_eq!(recent(entries.clone(), 3), entries[3..].to_vec());
        assert_eq!(recent(entries.clone(), 10), entries);
        // An exchange longer than the limit is kept whole.
        let long = vec![user("1"), assistant(&["a"]), result("a"), assistant(&[])];
        assert_eq!(recent(long.clone(), 2), long);
    }

    #[test]
    fn repair_adds_missing_results_and_drops_orphans() {
        let entries = vec![
            user("1"),
            assistant(&["a", "b"]),
            result("b"),
            // "a" was never answered (a pending action); the user wrote again.
            user("2"),
            // A late result for "a" after the user's message is an orphan.
            result("a"),
            assistant(&[]),
        ];
        let repaired = repair(entries);
        assert_eq!(repaired.len(), 6);
        assert_eq!(repaired[2], result("b"));
        match &repaired[3] {
            Entry::ToolResult {
                call_id, is_error, ..
            } => {
                assert_eq!(call_id, "a");
                assert!(is_error);
            }
            other => panic!("expected a placeholder result, got {other:?}"),
        }
        assert_eq!(repaired[4], user("2"));
    }

    #[test]
    fn repair_keeps_a_valid_history_unchanged_and_drops_duplicate_results() {
        let entries = vec![user("1"), assistant(&["a"]), result("a"), assistant(&[])];
        assert_eq!(repair(entries.clone()), entries);

        let duplicated = vec![user("1"), assistant(&["a"]), result("a"), result("a")];
        assert_eq!(repair(duplicated).len(), 3);
    }
}
