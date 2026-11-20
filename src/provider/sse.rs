//! A server-sent events decoder for provider streams. Chunks may split lines (and UTF-8
//! characters) anywhere; events come out once their blank line arrived.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=end).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.line(&String::from_utf8_lossy(&line)) {
                events.push(event);
            }
        }
        events
    }

    /// The event still being collected when the stream ended without a final blank line.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buffer.is_empty() {
            let line = String::from_utf8_lossy(&std::mem::take(&mut self.buffer)).into_owned();
            let line = line.trim_end_matches('\r').to_string();
            if let Some(event) = self.line(&line) {
                return Some(event);
            }
        }
        self.dispatch()
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        if self.data.is_empty() {
            return None;
        }
        Some(SseEvent {
            event,
            data: std::mem::take(&mut self.data).join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_in_chunks(input: &[u8], size: usize) -> Vec<SseEvent> {
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        for chunk in input.chunks(size) {
            events.extend(decoder.push(chunk));
        }
        events.extend(decoder.finish());
        events
    }

    #[test]
    fn events_survive_any_chunk_boundary() {
        let input = "event: message_start\r\ndata: {\"a\":\"é\"}\r\n\r\n: keep-alive\n\ndata: one\ndata: two\n\ndata: [DONE]"
            .as_bytes();
        let expected = vec![
            SseEvent {
                event: Some("message_start".into()),
                data: "{\"a\":\"é\"}".into(),
            },
            SseEvent {
                event: None,
                data: "one\ntwo".into(),
            },
            SseEvent {
                event: None,
                data: "[DONE]".into(),
            },
        ];
        for size in 1..input.len() {
            assert_eq!(decode_in_chunks(input, size), expected, "chunk size {size}");
        }
    }
}
