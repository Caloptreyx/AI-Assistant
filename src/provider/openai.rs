//! OpenAI-compatible chat completions (OpenAI, OpenRouter, Groq, Gemini, Ollama, LM Studio, vLLM,
//! ...).

use super::{Completion, ToolSpec, Usage, error_message, parse_arguments, sse::SseEvent};
use crate::chat::entry::{Entry, ToolCall};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Google's documented stand-in for a missing thought signature, for calls Gemini did not make
/// itself (another model earlier in the same conversation).
const SKIP_SIGNATURE: &str = "skip_thought_signature_validator";

fn arguments_text(arguments: &Value) -> String {
    match arguments {
        Value::String(raw) => raw.clone(),
        other => other.to_string(),
    }
}

/// One call of an assistant message in OpenAI's shape. For Gemini (`google`) it carries the
/// `extra_content` with the thought signature Gemini attached to it: Gemini 3 rejects the next
/// request of a tool-using turn with a 400 when the first call of a step comes back without it.
/// Other servers never see that field.
fn tool_call(call: &ToolCall, first: bool, google: bool) -> Value {
    let mut out = json!({
        "id": call.id,
        "type": "function",
        "function": {
            "name": call.name,
            "arguments": arguments_text(&call.arguments),
        },
    });
    if google {
        match &call.extra {
            Some(extra) => out["extra_content"] = extra.clone(),
            None if first => {
                out["extra_content"] =
                    json!({ "google": { "thought_signature": SKIP_SIGNATURE } });
            }
            None => {}
        }
    }
    out
}

pub fn body(
    model: &str,
    system: &str,
    history: &[Entry],
    tools: &[ToolSpec],
    google: bool,
) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for entry in history {
        messages.push(match entry {
            Entry::User { text } => json!({ "role": "user", "content": text }),
            Entry::Assistant { text, tool_calls } => {
                let content = if text.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                };
                let mut message = json!({ "role": "assistant", "content": content });
                if !tool_calls.is_empty() {
                    message["tool_calls"] = tool_calls
                        .iter()
                        .enumerate()
                        .map(|(position, call)| tool_call(call, position == 0, google))
                        .collect();
                }
                message
            }
            Entry::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => {
                let content = if *is_error {
                    format!("Error: {content}")
                } else {
                    content.clone()
                };
                json!({ "role": "tool", "tool_call_id": call_id, "content": content })
            }
        });
    }

    let mut body = json!({
        "model": model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.schema,
                    },
                })
            })
            .collect();
    }
    body
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
    extra: Option<Value>,
}

#[derive(Default)]
pub struct Parser {
    text: String,
    calls: BTreeMap<u64, PartialCall>,
    usage: Usage,
    done: bool,
}

impl Parser {
    pub fn done(&self) -> bool {
        self.done
    }

    /// Takes one stream event and returns the text it added.
    pub fn feed(&mut self, event: &SseEvent) -> Result<Option<String>, String> {
        let data = event.data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(None);
        }
        let value: Value = serde_json::from_str(data)
            .map_err(|err| format!("The AI provider sent an unreadable stream event: {err}"))?;
        if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
            return Err(format!(
                "The AI provider reported an error: {}",
                error_message(error)
            ));
        }

        if let Some(usage) = value.get("usage").filter(|usage| usage.is_object()) {
            if let Some(input) = usage.get("prompt_tokens").and_then(Value::as_u64) {
                self.usage.input = input;
            }
            if let Some(output) = usage.get("completion_tokens").and_then(Value::as_u64) {
                self.usage.output = output;
            }
        }

        let mut text = String::new();
        let choices = value.get("choices").and_then(Value::as_array);
        for choice in choices.into_iter().flatten() {
            if choice.get("index").and_then(Value::as_u64).unwrap_or(0) != 0 {
                continue;
            }
            let Some(delta) = choice.get("delta") else {
                continue;
            };
            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                text.push_str(content);
            }
            let calls = delta.get("tool_calls").and_then(Value::as_array);
            for call in calls.into_iter().flatten() {
                self.add_call_delta(call);
            }
        }

        self.text.push_str(&text);
        Ok((!text.is_empty()).then_some(text))
    }

    fn add_call_delta(&mut self, call: &Value) {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let mut index = call
            .get("index")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| self.calls.keys().next_back().copied().unwrap_or(0));
        // Some servers send every call with the same index (or none); a new id starts a new call.
        if let (Some(id), Some(existing)) = (id, self.calls.get(&index))
            && !existing.id.is_empty()
            && existing.id != id
        {
            index = self.calls.keys().next_back().map_or(0, |last| last + 1);
        }

        let entry = self.calls.entry(index).or_default();
        if let Some(id) = id {
            entry.id = id.to_string();
        }
        if let Some(name) = call.pointer("/function/name").and_then(Value::as_str)
            && entry.name.is_empty()
        {
            entry.name = name.to_string();
        }
        if let Some(arguments) = call.pointer("/function/arguments") {
            match arguments {
                Value::String(fragment) => entry.arguments.push_str(fragment),
                // A few servers send the arguments as an object in one piece.
                Value::Object(_) => entry.arguments = arguments.to_string(),
                _ => {}
            }
        }
        if let Some(extra) = call.get("extra_content").filter(|extra| extra.is_object()) {
            entry.extra = Some(extra.clone());
        }
    }

    pub fn finish(self) -> Completion {
        let tool_calls = self
            .calls
            .into_values()
            .filter(|call| !call.name.is_empty())
            .enumerate()
            .map(|(position, call)| ToolCall {
                id: if call.id.is_empty() {
                    format!("call_{position}")
                } else {
                    call.id
                },
                name: call.name,
                arguments: parse_arguments(&call.arguments),
                extra: call.extra,
            })
            .collect();
        Completion {
            text: self.text,
            tool_calls,
            usage: self.usage,
        }
    }
}
