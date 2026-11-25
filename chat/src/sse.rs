//! An incremental parser for server-sent events (the WHATWG
//! `text/event-stream` format). Bytes go in as they arrive, in pieces of any
//! size; whole events come out.

/// One event: its `event:` name (if any) and its `data:` lines joined by
/// newlines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug)]
pub struct SseParser {
    line: Vec<u8>,
    /// The last byte was a CR, so a following LF belongs to it.
    after_cr: bool,
    event: Option<String>,
    data: Vec<String>,
    has_data: bool,
    /// Any byte of an event not yet dispatched.
    pending: bool,
    /// The most bytes one event (all its lines) may hold.
    max_event: usize,
    event_bytes: usize,
    overflow: bool,
}

/// The default cap on one event.
pub const DEFAULT_MAX_EVENT: usize = 2 << 20;

impl Default for SseParser {
    fn default() -> Self {
        SseParser::with_max_line(DEFAULT_MAX_EVENT)
    }
}

impl SseParser {
    pub fn new() -> SseParser {
        SseParser::default()
    }

    /// A parser that gives up on an event larger than `max` bytes.
    pub fn with_max_line(max: usize) -> SseParser {
        SseParser {
            line: Vec::new(),
            after_cr: false,
            event: None,
            data: Vec::new(),
            has_data: false,
            pending: false,
            max_event: max,
            event_bytes: 0,
            overflow: false,
        }
    }

    /// True once an event went over the cap; nothing more is parsed.
    pub fn overflowed(&self) -> bool {
        self.overflow
    }

    /// Feed bytes; returns the events they completed.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        let mut out = Vec::new();
        for &b in bytes {
            if self.overflow {
                break;
            }
            if b != b'\r' && b != b'\n' {
                self.event_bytes += 1;
                if self.event_bytes > self.max_event {
                    self.overflow = true;
                    self.line = Vec::new();
                    self.data = Vec::new();
                    break;
                }
            }
            if self.after_cr {
                self.after_cr = false;
                if b == b'\n' {
                    continue;
                }
            }
            match b {
                b'\r' => {
                    self.after_cr = true;
                    self.end_line(&mut out);
                }
                b'\n' => self.end_line(&mut out),
                _ => {
                    self.line.push(b);
                    self.pending = true;
                }
            }
        }
        out
    }

    /// True when the stream ended inside an event (bytes since the last
    /// complete event).
    pub fn has_partial(&self) -> bool {
        self.pending || !self.line.is_empty()
    }

    fn end_line(&mut self, out: &mut Vec<SseEvent>) {
        let line = std::mem::take(&mut self.line);
        if line.is_empty() {
            if self.has_data {
                out.push(SseEvent {
                    event: self.event.take(),
                    data: std::mem::take(&mut self.data).join("\n"),
                });
            }
            self.event = None;
            self.data.clear();
            self.has_data = false;
            self.pending = false;
            self.event_bytes = 0;
            return;
        }
        self.pending = true;
        let line = String::from_utf8_lossy(&line);
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line.as_ref(), ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => {
                self.data.push(value.to_string());
                self.has_data = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events_with_any_line_ending() {
        let mut p = SseParser::new();
        let ev = p.push(
            b"event: a\ndata: 1\n\n: comment\r\ndata: x\r\ndata: y\r\n\r\nevent:b\rdata:z\r\r",
        );
        assert_eq!(
            ev,
            vec![
                SseEvent {
                    event: Some("a".into()),
                    data: "1".into()
                },
                SseEvent {
                    event: None,
                    data: "x\ny".into()
                },
                SseEvent {
                    event: Some("b".into()),
                    data: "z".into()
                },
            ]
        );
        assert!(!p.has_partial());
    }

    #[test]
    fn same_events_at_every_split_point() {
        let stream = b"event: a\r\ndata: {\"k\":\"\xc3\xa9\"}\r\n\r\nevent: b\ndata: 2\n\n";
        let whole = SseParser::new().push(stream);
        assert_eq!(whole.len(), 2);
        for i in 0..=stream.len() {
            let mut p = SseParser::new();
            let mut got = p.push(&stream[..i]);
            got.extend(p.push(&stream[i..]));
            assert_eq!(got, whole, "split at {i}");
        }
        // One byte at a time.
        let mut p = SseParser::new();
        let got: Vec<_> = stream.iter().flat_map(|b| p.push(&[*b])).collect();
        assert_eq!(got, whole);
    }

    #[test]
    fn reports_a_cut_off_event() {
        let mut p = SseParser::new();
        assert!(p.push(b"event: a\ndata: {\"par").is_empty());
        assert!(p.has_partial());
        let mut p = SseParser::new();
        p.push(b"data: 1\n");
        assert!(p.has_partial());
    }

    #[test]
    fn an_event_over_the_cap_stops_the_parser() {
        let mut p = SseParser::with_max_line(32);
        assert_eq!(p.push(b"data: short\n\n").len(), 1);
        assert!(!p.overflowed());
        // Each event is counted on its own, not the stream.
        for _ in 0..10 {
            assert_eq!(p.push(b"data: again\n\n").len(), 1);
        }
        // One long line, fed in pieces.
        for _ in 0..10 {
            p.push(b"data: xxxxxxxx");
        }
        assert!(p.overflowed());
        assert!(p.push(b"\n\ndata: later\n\n").is_empty());
        // Many lines of one event count together.
        let mut p = SseParser::with_max_line(32);
        p.push(b"data: 0123456789\ndata: 0123456789\ndata: 0123456789\n");
        assert!(p.overflowed());
    }

    #[test]
    fn event_without_data_is_not_dispatched() {
        let mut p = SseParser::new();
        assert!(p.push(b"event: ping\n\n").is_empty());
    }
}
