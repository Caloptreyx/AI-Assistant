use super::*;
use crate::chat::entry::{Entry, ToolCall};
use serde_json::json;

const OPENAI: &str = include_str!("testdata/openai_tool_calls.sse");
const OLLAMA: &str = include_str!("testdata/ollama_tool_call.sse");
const ANTHROPIC: &str = include_str!("testdata/anthropic_tool_use.sse");

/// Runs a recorded stream through the reader in chunks of `size` bytes and returns the text
/// deltas joined together plus the result.
fn read(kind: ProviderKind, input: &str, size: usize) -> (String, Result<Completion, String>) {
    let mut streamed = String::new();
    let mut on_text = |text: &str| streamed.push_str(text);
    let mut reader = StreamReader::new(kind);
    for chunk in input.as_bytes().chunks(size) {
        if let Err(err) = reader.push(chunk, &mut on_text) {
            return (streamed, Err(err));
        }
        if reader.parser.done() {
            break;
        }
    }
    let result = reader.finish(&mut on_text);
    (streamed, result)
}

fn call(id: &str, name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    }
}

#[test]
fn openai_stream_yields_text_tool_calls_and_usage() {
    for size in [1, 7, 64, 4096] {
        let (streamed, result) = read(ProviderKind::Openai, OPENAI, size);
        let completion = result.unwrap();
        assert_eq!(streamed, "Let me check the console.");
        assert_eq!(completion.text, streamed);
        assert_eq!(
            completion.tool_calls,
            vec![
                call("call_Qx1", "read_console", json!({ "lines": 50 })),
                call("call_Rt2", "power_server", json!({ "action": "restart" })),
            ]
        );
        assert_eq!(
            completion.usage,
            Usage {
                input: 812,
                output: 41
            }
        );
    }
}

#[test]
fn ollama_style_calls_without_index_or_id_get_an_id() {
    let completion = read(ProviderKind::Openai, OLLAMA, 33).1.unwrap();
    assert_eq!(completion.text, "");
    assert_eq!(
        completion.tool_calls,
        vec![call(
            "call_0",
            "list_files",
            json!({ "directory": "/plugins" })
        )]
    );
}

#[test]
fn anthropic_stream_yields_text_tool_use_and_usage_but_no_thinking() {
    for size in [1, 13, 4096] {
        let (streamed, result) = read(ProviderKind::Anthropic, ANTHROPIC, size);
        let completion = result.unwrap();
        assert_eq!(streamed, "I'll restart the server.");
        assert!(!completion.text.contains("wants a restart"));
        assert_eq!(
            completion.tool_calls,
            vec![call(
                "toolu_01T1x1fJ34qAmk2tNTrN7Up6",
                "power_server",
                json!({ "action": "restart" })
            )]
        );
        assert_eq!(
            completion.usage,
            Usage {
                input: 1124,
                output: 57
            }
        );
    }
}

#[test]
fn errors_inside_the_stream_are_reported() {
    let openai = "data: {\"error\":{\"message\":\"Rate limit reached for gpt-4o\",\"type\":\"requests\"}}\n\n";
    let err = read(ProviderKind::Openai, openai, 10).1.unwrap_err();
    assert!(err.contains("Rate limit reached for gpt-4o"), "{err}");

    let anthropic = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
    let err = read(ProviderKind::Anthropic, anthropic, 10).1.unwrap_err();
    assert!(err.contains("Overloaded"), "{err}");
}

#[test]
fn missing_usage_is_estimated_so_the_token_limit_still_counts() {
    // The Ollama sample reports no usage at all.
    let completion = read(ProviderKind::Openai, OLLAMA, 4096).1.unwrap();
    assert_eq!(completion.usage, Usage::default());
    let body = json!({ "messages": [{ "role": "user", "content": "x".repeat(400) }] });
    let usage = completion.usage_or_estimate(&body);
    assert!(usage.input >= 100, "{usage:?}");
    assert!(usage.output > 0, "{usage:?}");

    // Reported usage is used as it is.
    let completion = read(ProviderKind::Openai, OPENAI, 4096).1.unwrap();
    assert_eq!(
        completion.usage_or_estimate(&body),
        Usage {
            input: 812,
            output: 41
        }
    );
}

#[test]
fn a_body_without_events_is_an_error() {
    let err = read(ProviderKind::Openai, "{\"choices\":[]}", 100)
        .1
        .unwrap_err();
    assert!(err.contains("without streaming"), "{err}");
}

#[test]
fn broken_tool_arguments_are_kept_as_text() {
    assert_eq!(parse_arguments(""), json!({}));
    assert_eq!(parse_arguments("{\"a\":1}"), json!({ "a": 1 }));
    assert_eq!(parse_arguments("{\"a\":"), json!("{\"a\":"));
}

#[test]
fn http_error_bodies_are_reduced_to_their_message() {
    assert_eq!(
        error_text(r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#),
        "invalid x-api-key"
    );
    assert_eq!(
        error_text(r#"{"error":{"message":"Incorrect API key provided","code":"invalid_api_key"}}"#),
        "Incorrect API key provided"
    );
    assert_eq!(error_text("Bad Gateway"), "Bad Gateway");
    assert_eq!(error_text(""), "no details");
}

fn history() -> Vec<Entry> {
    vec![
        Entry::User {
            text: "Restart it".into(),
        },
        Entry::Assistant {
            text: "Checking first.".into(),
            tool_calls: vec![
                call("c1", "read_console", json!({ "lines": 20 })),
                call("c2", "power_server", json!("{broken")),
            ],
        },
        Entry::ToolResult {
            call_id: "c1".into(),
            name: "read_console".into(),
            content: "Done (3.2s)!".into(),
            is_error: false,
        },
        Entry::ToolResult {
            call_id: "c2".into(),
            name: "power_server".into(),
            content: "The user rejected this action.".into(),
            is_error: true,
        },
        Entry::User {
            text: "Never mind".into(),
        },
    ]
}

fn tools() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "read_console".into(),
        description: "Read the console".into(),
        schema: json!({ "type": "object", "properties": {} }),
    }]
}

fn provider(kind: ProviderKind) -> Provider {
    Provider {
        kind,
        url: String::new(),
        api_key: String::new(),
        model: "test-model".into(),
        max_tokens: 1024,
        pin: None,
    }
}

#[test]
fn openai_request_carries_tool_calls_and_results() {
    let body = provider(ProviderKind::Openai).body("SYSTEM", &history(), &tools());
    assert_eq!(body["model"], json!("test-model"));
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["stream_options"], json!({ "include_usage": true }));

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 6);
    assert_eq!(messages[0], json!({ "role": "system", "content": "SYSTEM" }));
    assert_eq!(messages[1], json!({ "role": "user", "content": "Restart it" }));
    assert_eq!(messages[2]["content"], json!("Checking first."));
    assert_eq!(
        messages[2]["tool_calls"][0],
        json!({
            "id": "c1",
            "type": "function",
            "function": { "name": "read_console", "arguments": "{\"lines\":20}" },
        })
    );
    // Arguments that never parsed go back exactly as the model sent them.
    assert_eq!(
        messages[2]["tool_calls"][1]["function"]["arguments"],
        json!("{broken")
    );
    assert_eq!(
        messages[3],
        json!({ "role": "tool", "tool_call_id": "c1", "content": "Done (3.2s)!" })
    );
    assert_eq!(
        messages[4]["content"],
        json!("Error: The user rejected this action.")
    );
    assert_eq!(messages[5], json!({ "role": "user", "content": "Never mind" }));

    assert_eq!(
        body["tools"],
        json!([{
            "type": "function",
            "function": {
                "name": "read_console",
                "description": "Read the console",
                "parameters": { "type": "object", "properties": {} },
            },
        }])
    );
    let without_tools = provider(ProviderKind::Openai).body("S", &history(), &[]);
    assert!(without_tools.get("tools").is_none());
}

#[test]
fn anthropic_request_alternates_roles_and_nests_tool_results() {
    let body = provider(ProviderKind::Anthropic).body("SYSTEM", &history(), &tools());
    assert_eq!(body["system"], json!("SYSTEM"));
    assert_eq!(body["max_tokens"], json!(1024));

    let messages = body["messages"].as_array().unwrap();
    // user, assistant, then one user message with both results and the new text.
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], json!("user"));
    assert_eq!(messages[1]["role"], json!("assistant"));
    assert_eq!(
        messages[1]["content"],
        json!([
            { "type": "text", "text": "Checking first." },
            { "type": "tool_use", "id": "c1", "name": "read_console", "input": { "lines": 20 } },
            { "type": "tool_use", "id": "c2", "name": "power_server", "input": {} },
        ])
    );
    assert_eq!(
        messages[2],
        json!({
            "role": "user",
            "content": [
                { "type": "tool_result", "tool_use_id": "c1", "content": "Done (3.2s)!", "is_error": false },
                { "type": "tool_result", "tool_use_id": "c2", "content": "The user rejected this action.", "is_error": true },
                { "type": "text", "text": "Never mind" },
            ],
        })
    );
    assert_eq!(
        body["tools"],
        json!([{
            "name": "read_console",
            "description": "Read the console",
            "input_schema": { "type": "object", "properties": {} },
        }])
    );
}
