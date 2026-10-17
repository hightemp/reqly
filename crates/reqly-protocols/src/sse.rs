//! Server-sent events parser, following the WHATWG HTML "event stream
//! interpretation" algorithm. Bytes are fed incrementally.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SseEvent {
    /// The last event ID (carried over from earlier events when not set).
    pub id: String,
    /// `message` unless the stream named it.
    pub event: String,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    /// A CR ended the previous chunk; a following LF belongs to it.
    pending_cr: bool,
    bom_checked: bool,
    data: String,
    event_type: String,
    /// The "last event ID buffer".
    id_buffer: String,
    /// The last event ID string (survives across events and reconnects).
    pub last_event_id: String,
    /// Reconnection time in milliseconds, when the stream set one.
    pub retry: Option<u64>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds bytes; returns the events they completed.
    pub fn feed(&mut self, mut bytes: &[u8]) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if self.pending_cr && bytes.first() == Some(&b'\n') {
            bytes = &bytes[1..];
        }
        self.pending_cr = false;
        self.buffer.extend_from_slice(bytes);
        if !self.bom_checked {
            if self.buffer.len() < 3 && b"\xEF\xBB\xBF".starts_with(&self.buffer) {
                return out;
            }
            if self.buffer.starts_with(b"\xEF\xBB\xBF") {
                self.buffer.drain(..3);
            }
            self.bom_checked = true;
        }
        let mut start = 0;
        let mut i = 0;
        while i < self.buffer.len() {
            match self.buffer[i] {
                b'\n' => {
                    let line = String::from_utf8_lossy(&self.buffer[start..i]).into_owned();
                    self.line(&line, &mut out);
                    i += 1;
                    start = i;
                }
                b'\r' => {
                    let line = String::from_utf8_lossy(&self.buffer[start..i]).into_owned();
                    self.line(&line, &mut out);
                    i += 1;
                    if i < self.buffer.len() {
                        if self.buffer[i] == b'\n' {
                            i += 1;
                        }
                    } else {
                        self.pending_cr = true;
                    }
                    start = i;
                }
                _ => i += 1,
            }
        }
        self.buffer.drain(..start);
        out
    }

    /// End of stream: an incomplete event is discarded (per the spec).
    pub fn finish(&mut self) {
        self.buffer.clear();
        self.data.clear();
        self.event_type.clear();
    }

    fn line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.find(':') {
            Some(i) => {
                let value = &line[i + 1..];
                (&line[..i], value.strip_prefix(' ').unwrap_or(value))
            }
            None => (line, ""),
        };
        match field {
            "event" => self.event_type = value.to_string(),
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            "id" if !value.contains('\0') => self.id_buffer = value.to_string(),
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                self.retry = value.parse().ok();
            }
            _ => {}
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        self.last_event_id = self.id_buffer.clone();
        if self.data.is_empty() {
            self.event_type.clear();
            return;
        }
        if self.data.ends_with('\n') {
            self.data.pop();
        }
        let event = if self.event_type.is_empty() {
            "message".to_string()
        } else {
            std::mem::take(&mut self.event_type)
        };
        out.push(SseEvent {
            id: self.last_event_id.clone(),
            event,
            data: std::mem::take(&mut self.data),
        });
        self.event_type.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Vec<SseEvent> {
        SseParser::new().feed(s.as_bytes())
    }

    fn ev(id: &str, event: &str, data: &str) -> SseEvent {
        SseEvent {
            id: id.into(),
            event: event.into(),
            data: data.into(),
        }
    }

    #[test]
    fn spec_examples() {
        // HTML spec §9.2.6 examples.
        assert_eq!(
            parse("data: YHOO\ndata: +2\ndata: 10\n\n"),
            vec![ev("", "message", "YHOO\n+2\n10")]
        );
        assert_eq!(
            parse(
                ": test stream\n\ndata: first event\nid: 1\n\ndata:second event\nid\n\ndata:  third event\n\n"
            ),
            vec![
                ev("1", "message", "first event"),
                ev("", "message", "second event"),
                ev("", "message", " third event")
            ]
        );
        assert_eq!(
            parse("data\n\ndata\ndata\n\ndata:"),
            vec![ev("", "message", ""), ev("", "message", "\n")]
        );
        assert_eq!(
            parse("data:test\n\ndata: test\n\n"),
            vec![ev("", "message", "test"), ev("", "message", "test")]
        );
    }

    #[test]
    fn event_types_retry_and_ids() {
        let mut p = SseParser::new();
        let events =
            p.feed(b"retry: 3000\nevent: add\ndata: 1\nid: 7\n\ndata: 2\n\nretry: x\nid: a\0b\n\n");
        assert_eq!(events, vec![ev("7", "add", "1"), ev("7", "message", "2")]);
        assert_eq!(p.retry, Some(3000));
        assert_eq!(p.last_event_id, "7");
    }

    #[test]
    fn line_endings_bom_and_chunking() {
        let input = "\u{FEFF}data: a\r\n\r\ndata: b\r\rdata: c\n\n";
        let expected = vec![
            ev("", "message", "a"),
            ev("", "message", "b"),
            ev("", "message", "c"),
        ];
        assert_eq!(parse(input), expected);
        // Every split point gives the same events.
        for split in 0..=input.len() {
            let mut p = SseParser::new();
            let bytes = input.as_bytes();
            let mut got = p.feed(&bytes[..split]);
            got.extend(p.feed(&bytes[split..]));
            assert_eq!(got, expected, "split at {split}");
        }
    }

    #[test]
    fn incomplete_events_are_dropped_at_the_end() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: partial").is_empty());
        p.finish();
        assert!(p.feed(b"\n\n").is_empty());
    }
}
