use super::tmux::Transport;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    #[default]
    None,
    Sixel,
    Kitty,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Keyboard {
    #[default]
    Unknown,
    Legacy,
    Kitty,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub graphics: Protocol,
    pub shared_memory: bool,
    pub keyboard: Keyboard,
    pub transport: Transport,
}

// Unnumbered DA1 and keyboard replies are accepted only during this launch's one query window.
pub const QUERY_TIME: Duration = Duration::from_millis(500);
pub const KEYBOARD_ENABLE: &[u8] = b"\x1b[>29u";
pub const KEYBOARD_DISABLE: &[u8] = b"\x1b[<u";
#[derive(Default)]
pub struct Negotiation {
    pub detected: Capabilities,
    deadline: Option<Instant>,
    ids: (u32, u32),
    graphics: bool,
    shared: bool,
    attributes: bool,
    keyboard: bool,
    keyboard_pending: bool,
}
impl Negotiation {
    pub fn begin(transport: Transport, ids: (u32, u32), shared: bool, now: Instant) -> Self {
        Self {
            detected: Capabilities { transport, ..Capabilities::default() },
            deadline: Some(now + QUERY_TIME),
            ids,
            graphics: transport.graphics(),
            shared: shared && transport == Transport::Direct,
            attributes: true,
            keyboard: true,
            keyboard_pending: false,
        }
    }
    pub fn queries(&self, shared_query: &str) -> Vec<u8> {
        let transport = self.detected.transport;
        let mut bytes = Vec::new();
        if self.shared { transport.query(&mut bytes, shared_query.as_bytes()); }
        if self.graphics {
            transport.query(&mut bytes, format!("\x1b_Gi={},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\", self.ids.0).as_bytes());
        }
        bytes.extend_from_slice(b"\x1b[?u");
        if self.attributes { transport.query(&mut bytes, b"\x1b[c"); }
        bytes
    }
    pub fn expire(&mut self, now: Instant) {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = None;
            self.graphics = false;
            self.shared = false;
            self.attributes = false;
            self.keyboard = false;
            if self.detected.keyboard == Keyboard::Unknown { self.detected.keyboard = Keyboard::Legacy; }
        }
    }
    pub fn wait_ms(&self, now: Instant, maximum: i32) -> i32 {
        self.deadline.map_or(maximum, |deadline| {
            maximum.min(deadline.saturating_duration_since(now).as_micros().div_ceil(1000).min(i32::MAX as u128) as i32)
        })
    }
    pub fn take_keyboard_enable(&mut self) -> bool {
        std::mem::take(&mut self.keyboard_pending)
    }
    // Sample input: ?0u reports keyboard support; ?62;4c reports sixel support.
    pub fn csi(&mut self, sequence: &str, now: Instant) {
        self.expire(now);
        if sequence.len() > 128 { return; }
        if let Some(body) = sequence.strip_prefix('?').and_then(|s| s.strip_suffix('u')) {
            if self.keyboard && body.parse::<u32>().is_ok() {
                self.keyboard = false;
                self.detected.keyboard = Keyboard::Kitty;
                self.keyboard_pending = true;
            }
        } else if let Some(body) = sequence.strip_prefix('?').and_then(|s| s.strip_suffix('c')) {
            let numbers = body.split(';').map(str::parse::<u16>).collect::<Result<Vec<_>, _>>();
            if self.attributes {
                if let Ok(numbers) = numbers {
                    if numbers.len() >= 2 && matches!(numbers[0], 62..=65) {
                        self.attributes = false;
                        if numbers[1..].contains(&4) && self.detected.graphics != Protocol::Kitty {
                            self.detected.graphics = Protocol::Sixel;
                        }
                    }
                }
            }
        }
    }
    // Sample input: kind '_' and body Gi=31;OK acknowledge this launch's graphics query.
    pub fn string(&mut self, kind: u8, body: &[u8], now: Instant) {
        self.expire(now);
        if kind == b'_' {
            let Some(at) = body.iter().position(|b| *b == b';') else { return; };
            let (header, result) = (&body[..at], &body[at + 1..]);
            let Some(id) = header.strip_prefix(b"Gi=").and_then(|s| std::str::from_utf8(s).ok()).and_then(|s| s.parse::<u32>().ok()) else { return; };
            if self.graphics && id == self.ids.0 {
                self.graphics = false;
                if result == b"OK" { self.detected.graphics = Protocol::Kitty; }
            } else if self.shared && id == self.ids.1 {
                self.shared = false;
                if result == b"OK" { self.detected.shared_memory = true; }
            }
        }
    }
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
