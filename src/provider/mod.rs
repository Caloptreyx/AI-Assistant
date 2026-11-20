//! Talks to the configured LLM: builds the request for the provider's API and turns its streamed
//! answer into text deltas plus a finished [`Completion`].

pub mod anthropic;
pub mod openai;
pub mod sse;

use crate::{
    chat::entry::{Entry, ToolCall},
    settings::ProviderKind,
};
use futures_util::StreamExt;
use serde_json::Value;
use sse::{SseDecoder, SseEvent};
use std::{sync::LazyLock, time::Duration};

fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // Reasoning models can think for a while before the first token.
        .read_timeout(Duration::from_secs(180))
        // Providers answer directly; a redirect could lead a vetted request somewhere else.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!(
            "Calagopus-AI-Assistant/",
            env!("CARGO_PKG_VERSION")
        ))
}

static CLIENT: LazyLock<reqwest::Client> =
    LazyLock::new(|| client_builder().build().unwrap_or_default());

pub struct Provider {
    pub kind: ProviderKind,
    /// The full endpoint URL.
    pub url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    /// For user-supplied URLs: connect only to these vetted addresses.
    pub pin: Option<crate::ssrf::Pin>,
}

pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

#[derive(Debug, PartialEq)]
pub struct Completion {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
}

impl Completion {
    /// The tokens the provider reported, or a rough estimate (four characters per token) when it
    /// reported none, as some OpenAI-compatible servers do in streams, so the daily token limit
    /// still applies to them.
    pub fn usage_or_estimate(&self, body: &Value) -> Usage {
        if self.usage != Usage::default() {
            return self.usage;
        }
        let output = self.text.len()
            + self
                .tool_calls
                .iter()
                .map(|call| call.name.len() + call.arguments.to_string().len())
                .sum::<usize>();
        Usage {
            input: (body.to_string().len() / 4) as u64,
            output: output.div_ceil(4) as u64,
        }
    }
}

/// Tool arguments as JSON: `{}` for nothing, the raw text when it does not parse (the tool then
/// reports that to the model).
pub fn parse_arguments(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// The readable part of a provider error object or body.
pub fn error_message(error: &Value) -> String {
    let message = error
        .pointer("/error/message")
        .or_else(|| error.get("message"))
        .or_else(|| error.get("error"))
        .and_then(Value::as_str);
    match message {
        Some(message) => message.chars().take(500).collect(),
        None => error.to_string().chars().take(500).collect(),
    }
}

fn error_text(body: &str) -> String {
    match serde_json::from_str::<Value>(body) {
        Ok(value) => error_message(&value),
        Err(_) if body.trim().is_empty() => "no details".into(),
        Err(_) => body.trim().chars().take(500).collect(),
    }
}

enum Parser {
    Openai(openai::Parser),
    Anthropic(anthropic::Parser),
}

impl Parser {
    fn new(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::Openai => Self::Openai(Default::default()),
            ProviderKind::Anthropic => Self::Anthropic(Default::default()),
        }
    }

    fn feed(&mut self, event: &SseEvent) -> Result<Option<String>, String> {
        match self {
            Self::Openai(parser) => parser.feed(event),
            Self::Anthropic(parser) => parser.feed(event),
        }
    }

    fn done(&self) -> bool {
        match self {
            Self::Openai(parser) => parser.done(),
            Self::Anthropic(parser) => parser.done(),
        }
    }

    fn finish(self) -> Completion {
        match self {
            Self::Openai(parser) => parser.finish(),
            Self::Anthropic(parser) => parser.finish(),
        }
    }
}

/// Decodes a whole streamed answer. `on_text` gets every text delta as it arrives.
struct StreamReader {
    decoder: SseDecoder,
    parser: Parser,
    events: usize,
}

impl StreamReader {
    fn new(kind: ProviderKind) -> Self {
        Self {
            decoder: SseDecoder::default(),
            parser: Parser::new(kind),
            events: 0,
        }
    }

    fn push(&mut self, chunk: &[u8], on_text: &mut impl FnMut(&str)) -> Result<(), String> {
        for event in self.decoder.push(chunk) {
            self.events += 1;
            if let Some(text) = self.parser.feed(&event)? {
                on_text(&text);
            }
        }
        Ok(())
    }

    fn finish(mut self, on_text: &mut impl FnMut(&str)) -> Result<Completion, String> {
        if let Some(event) = self.decoder.finish() {
            self.events += 1;
            if let Some(text) = self.parser.feed(&event)? {
                on_text(&text);
            }
        }
        if self.events == 0 {
            return Err(
                "The AI provider answered without streaming any events. Check that the endpoint supports streamed chat completions."
                    .into(),
            );
        }
        Ok(self.parser.finish())
    }
}

impl Provider {
    pub fn body(&self, system: &str, history: &[Entry], tools: &[ToolSpec]) -> Value {
        match self.kind {
            ProviderKind::Openai => openai::body(&self.model, system, history, tools),
            ProviderKind::Anthropic => {
                anthropic::body(&self.model, self.max_tokens, system, history, tools)
            }
        }
    }

    fn request(&self, body: &Value) -> Result<reqwest::RequestBuilder, String> {
        let client = match &self.pin {
            None => CLIENT.clone(),
            Some(pin) => client_builder()
                .resolve_to_addrs(&pin.host, &pin.addrs)
                .build()
                .map_err(|err| format!("Could not prepare the request: {err}"))?,
        };
        let request = client.post(&self.url).json(body);
        Ok(match self.kind {
            // Local servers such as Ollama need no key.
            ProviderKind::Openai if self.api_key.is_empty() => request,
            ProviderKind::Openai => request.bearer_auth(&self.api_key),
            ProviderKind::Anthropic => request
                .header("x-api-key", self.api_key.as_str())
                .header("anthropic-version", anthropic::VERSION),
        })
    }

    /// Sends `body` and streams the answer, calling `on_text` with every text delta.
    pub async fn complete(
        &self,
        body: &Value,
        mut on_text: impl FnMut(&str),
    ) -> Result<Completion, String> {
        let response = self
            .request(body)?
            .send()
            .await
            .map_err(|err| format!("Could not reach the AI provider: {err}"))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "The AI provider answered HTTP {}: {}",
                status.as_u16(),
                error_text(&text)
            ));
        }

        let mut reader = StreamReader::new(self.kind);
        let mut stream = std::pin::pin!(response.bytes_stream());
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|err| format!("The AI provider's answer broke off: {err}"))?;
            reader.push(&chunk, &mut on_text)?;
            if reader.parser.done() {
                break;
            }
        }
        reader.finish(&mut on_text)
    }
}

#[cfg(test)]
mod tests;
