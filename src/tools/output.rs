use super::Ctx;
use crate::proxy::{self, Reply};
use axum::http::Method;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// The most text one panel response contributes before [`clip`] cuts it to the admin's limit.
pub const MAX_TEXT: usize = 100_000;
const BINARY_TYPES: [&str; 12] = [
    "image/", "audio/", "video/", "font/", "zip", "gzip", "tar", "pdf", "wasm", "7z", "rar",
    "msgpack",
];

/// Cuts `text` to at most `max` bytes without splitting a character.
pub fn floor_boundary(text: &str, max: usize) -> usize {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

pub fn truncate(text: String) -> String {
    if text.len() <= MAX_TEXT {
        return text;
    }

    let end = floor_boundary(&text, MAX_TEXT);
    format!(
        "{}\n\n[Truncated: showing the first {end} of {} bytes. Narrow the request with query parameters or pagination.]",
        &text[..end],
        text.len()
    )
}

/// Shortens tool output to at most `limit` characters before it goes to the model: the start and
/// the end stay (log tails and file headers both matter), the middle is replaced by a marker.
pub fn clip(text: &str, limit: usize) -> String {
    let total = text.chars().count();
    if total <= limit {
        return text.to_string();
    }
    let head_chars = limit * 2 / 3;
    let tail_chars = limit - head_chars;
    let head: String = text.chars().take(head_chars).collect();
    let tail: String = text.chars().skip(total - tail_chars).collect();
    format!(
        "{head}\n\n[... {} characters omitted; ask for a smaller part, e.g. fewer console lines or a line range ...]\n\n{tail}",
        total - head_chars - tail_chars
    )
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data).as_slice())
}

fn as_text<'a>(content_type: Option<&str>, body: &'a [u8]) -> Option<&'a str> {
    let binary_type = content_type.is_some_and(|content_type| {
        let content_type = content_type.to_ascii_lowercase();
        BINARY_TYPES.iter().any(|kind| content_type.contains(kind))
    });
    if binary_type || body.contains(&0) {
        return None;
    }
    std::str::from_utf8(body).ok()
}

pub fn render(reply: &Reply) -> Result<String, String> {
    let body = if reply.body.is_empty() {
        format!("HTTP {} with an empty body", reply.status)
    } else {
        match as_text(reply.content_type.as_deref(), &reply.body) {
            Some(text) => text.to_string(),
            None => format!(
                "Binary response ({}, {} bytes)",
                reply.content_type.as_deref().unwrap_or("unknown type"),
                reply.body.len()
            ),
        }
    };

    if reply.status >= 400 {
        Err(truncate(format!("HTTP {}: {body}", reply.status)))
    } else {
        Ok(truncate(body))
    }
}

pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

pub async fn request(
    ctx: &Ctx,
    method: Method,
    path: &str,
    query: Option<&Map<String, Value>>,
    body: Option<&Value>,
) -> Result<String, String> {
    render(&proxy::call(ctx, method, path, query, body).await?)
}

/// Like [`request`], and also returns the response parsed as JSON (whole, even when the text had
/// to be truncated), or `None` when it is not JSON.
pub async fn request_json(
    ctx: &Ctx,
    method: Method,
    path: &str,
    query: Option<&Map<String, Value>>,
    body: Option<&Value>,
) -> Result<(String, Option<Value>), String> {
    let reply = proxy::call(ctx, method, path, query, body).await?;
    let text = render(&reply)?;
    Ok((text, serde_json::from_slice(&reply.body).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_boundary_never_splits_a_character() {
        let text = "aé"; // é is two bytes
        assert_eq!(floor_boundary(text, 2), 1);
        assert_eq!(floor_boundary(text, 3), 3);
        assert_eq!(floor_boundary(text, 10), 3);
    }

    #[test]
    fn clip_keeps_start_and_end_within_the_limit() {
        assert_eq!(clip("short", 100), "short");

        let text: String = (0..1000).map(|n| format!("line {n}\n")).collect();
        let clipped = clip(&text, 300);
        assert!(clipped.starts_with("line 0\n"));
        assert!(clipped.ends_with("line 999\n"));
        assert!(clipped.contains("characters omitted"));
        let kept = clipped.chars().count() - clipped.lines().filter(|l| l.contains("omitted")).map(|l| l.chars().count()).sum::<usize>();
        assert!(kept <= 300 + 4, "{kept}");

        // Multi-byte characters are never split.
        let wide = "é".repeat(50);
        assert_eq!(clip(&wide, 10).matches('é').count(), 10);
    }

    #[test]
    fn strip_ansi_removes_colour_codes() {
        assert_eq!(strip_ansi("\u{1b}[33mWARN\u{1b}[0m done"), "WARN done");
    }
}
