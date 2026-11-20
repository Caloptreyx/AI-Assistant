//! Anthropic's Messages API.

use super::{Completion, ToolSpec, Usage, error_message, parse_arguments, sse::SseEvent};
use crate::chat::entry::{Entry, ToolCall};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const URL: &str = "https://api.anthropic.com/v1/messages";
pub const VERSION: &str = "2023-06-01";

fn blocks(entry: &Entry) -> (&'static str, Vec<Value>) {
    match entry {
        Entry::User { text } => ("user", vec![json!({ "type": "text", "text": text })]),
        Entry::Assistant { text, tool_calls } => {
            let mut blocks = Vec::new();
            if !text.trim().is_empty() {
                blocks.push(json!({ "type": "text", "text": text }));
            }
            for call in tool_calls {
                let input = if call.arguments.is_object() {
                    call.arguments.clone()
                } else {
                    json!({})
                };
                blocks.push(json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": input,
                }));
            }
            ("assistant", blocks)
        }
        Entry::ToolResult {
            call_id,
            content,
            is_error,
            ..
        } => {
            let content = if content.is_empty() {
                "(no output)".to_string()
            } else {
                content.clone()
            };
            (
                "user",
                vec![json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                    "is_error": is_error,
                })],
            )
        }
    }
}

pub fn body(
    model: &str,
    max_tokens: u32,
    system: &str,
    history: &[Entry],
    tools: &[ToolSpec],
) -> Value {
    // Anthropic wants strictly alternating roles, so consecutive entries of the same role
    // (tool results followed by a new user message) share one message.
    let mut messages: Vec<Value> = Vec::new();
    for entry in history {
        let (role, content) = blocks(entry);
        if content.is_empty() {
            continue;
        }
        match messages.last_mut() {
            Some(last) if last["role"] == role => {
                if let Some(existing) = last["content"].as_array_mut() {
                    existing.extend(content);
                }
            }
            _ => messages.push(json!({ "role": role, "content": content })),
        }
    }

    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "stream": true,
        "system": system,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.schema,
                })
            })
            .collect();
    }
    body
}

enum Block {
    Text,
    Tool {
        id: String,
        name: String,
        json: String,
    },
    Other,
}

#[derive(Default)]
pub struct Parser {
    text: String,
    blocks: BTreeMap<u64, Block>,
    usage: Usage,
    done: bool,
}

fn input_tokens(usage: &Value) -> Option<u64> {
    let base = usage.get("input_tokens").and_then(Value::as_u64)?;
    let cached = ["cache_creation_input_tokens", "cache_read_input_tokens"]
        .iter()
        .filter_map(|key| usage.get(*key).and_then(Value::as_u64))
        .sum::<u64>();
    Some(base + cached)
}

impl Parser {
    pub fn done(&self) -> bool {
        self.done
    }

    /// Takes one stream event and returns the text it added.
    pub fn feed(&mut self, event: &SseEvent) -> Result<Option<String>, String> {
        let value: Value = serde_json::from_str(event.data.trim())
            .map_err(|err| format!("The AI provider sent an unreadable stream event: {err}"))?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .or(event.event.as_deref())
            .unwrap_or_default();
        let index = value.get("index").and_then(Value::as_u64).unwrap_or(0);

        match kind {
            "message_start" => {
                if let Some(usage) = value.pointer("/message/usage") {
                    if let Some(input) = input_tokens(usage) {
                        self.usage.input = input;
                    }
                    if let Some(output) = usage.get("output_tokens").and_then(Value::as_u64) {
                        self.usage.output = output;
                    }
                }
            }
            "content_block_start" => {
                let block = &value["content_block"];
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        self.blocks.insert(index, Block::Text);
                        let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                        if !text.is_empty() {
                            self.text.push_str(text);
                            return Ok(Some(text.to_string()));
                        }
                    }
                    Some("tool_use") => {
                        self.blocks.insert(
                            index,
                            Block::Tool {
                                id: block["id"].as_str().unwrap_or_default().to_string(),
                                name: block["name"].as_str().unwrap_or_default().to_string(),
                                json: String::new(),
                            },
                        );
                    }
                    _ => {
                        self.blocks.insert(index, Block::Other);
                    }
                }
            }
            "content_block_delta" => {
                let delta = &value["delta"];
                match (
                    delta.get("type").and_then(Value::as_str),
                    self.blocks.get_mut(&index),
                ) {
                    (Some("text_delta"), Some(Block::Text)) => {
                        let text = delta["text"].as_str().unwrap_or_default();
                        if !text.is_empty() {
                            self.text.push_str(text);
                            return Ok(Some(text.to_string()));
                        }
                    }
                    (Some("input_json_delta"), Some(Block::Tool { json, .. })) => {
                        json.push_str(delta["partial_json"].as_str().unwrap_or_default());
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(usage) = value.get("usage") {
                    if let Some(input) = input_tokens(usage) {
                        self.usage.input = input;
                    }
                    if let Some(output) = usage.get("output_tokens").and_then(Value::as_u64) {
                        self.usage.output = output;
                    }
                }
            }
            "message_stop" => self.done = true,
            "error" => {
                return Err(format!(
                    "The AI provider reported an error: {}",
                    error_message(value.get("error").unwrap_or(&value))
                ));
            }
            _ => {}
        }
        Ok(None)
    }

    pub fn finish(self) -> Completion {
        let tool_calls = self
            .blocks
            .into_values()
            .filter_map(|block| match block {
                Block::Tool { id, name, json } if !name.is_empty() => Some(ToolCall {
                    id,
                    name,
                    arguments: parse_arguments(&json),
                }),
                _ => None,
            })
            .collect();
        Completion {
            text: self.text,
            tool_calls,
            usage: self.usage,
        }
    }
}
