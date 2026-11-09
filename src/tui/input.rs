#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub text: String,
    pub mods: String,
    pub pointer: Option<Pointer>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub button: u32,
    pub x: usize,
    pub y: usize,
    pub released: bool,
    pub motion: bool,
}
// A wheel turn arrives as a pointer button, one per notch.
pub const WHEEL_UP: u32 = 64;
pub const WHEEL_DOWN: u32 = 65;
impl Key {
    pub fn named(name: &str, mods: &str) -> Self {
        Self {
            name: name.into(),
            text: String::new(),
            mods: mods.into(),
            pointer: None,
        }
    }
    pub fn character(c: char, mods: &str) -> Self {
        let name = match c {
            ' ' => "Space".into(),
            ',' => "Comma".into(),
            '.' => "Period".into(),
            '[' => "BracketLeft".into(),
            ']' => "BracketRight".into(),
            '-' => "Minus".into(),
            '+' => "Plus".into(),
            '=' => "Equal".into(),
            '_' => "Underscore".into(),
            '>' => "Greater".into(),
            _ => c.to_uppercase().to_string(),
        };
        Self {
            name,
            text: c.to_string(),
            mods: mods.into(),
            pointer: None,
        }
    }
    pub fn for_action(&self) -> std::borrow::Cow<'_, Self> {
        if !self.text.is_empty() && !self.text.is_ascii() && self.name.len() == 1 && self.name.as_bytes()[0].is_ascii_alphabetic() {
            let mut key = self.clone();
            key.text = if self.mods.contains("shift") || self.text.chars().next().is_some_and(char::is_uppercase) {
                self.name.clone()
            } else { self.name.to_ascii_lowercase() };
            std::borrow::Cow::Owned(key)
        } else { std::borrow::Cow::Borrowed(self) }
    }

}
// Capability controls and associated text have separate encoded-byte budgets.
const CSI_CONTROL_LIMIT: usize = 128;
const CSI_TEXT_LIMIT: usize = 64 * 1024;
const REPLY_CAPTURE_LIMIT: usize = 512;
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    string_kind: Option<u8>,
    reply: Vec<u8>,
    oversized: bool,
    discard_csi: bool,
    pub negotiation: super::capabilities::Negotiation,
    paste: Option<Vec<u8>>,
}
impl Decoder {
    pub fn new(negotiation: super::capabilities::Negotiation) -> Self {
        Self { negotiation, ..Self::default() }
    }
    pub fn waiting(&self) -> bool {
        self.string_kind.is_none() && self.pending.as_slice() == [27]
    }
    pub fn feed(&mut self, bytes: &[u8], idle: bool) -> Vec<Key> {
        self.feed_at(bytes, idle, std::time::Instant::now())
    }
    // Sample input: j ESC_Gi=31;OK ST k emits j and k while retaining fragmented replies.
    pub fn feed_at(&mut self, bytes: &[u8], idle: bool, now: std::time::Instant) -> Vec<Key> {
        self.negotiation.expire(now);
        // A late reply's tail stays inside its control string; Escape can cancel an abandoned string.
        if idle && bytes.is_empty() && self.string_kind.is_some() && self.pending.as_slice() == [27] {
            self.string_kind = None;
            self.reply.clear();
        }
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        while !self.pending.is_empty() {
            if let Some(paste) = &mut self.paste {
                const PASTE_END: &[u8] = b"\x1b[201~";
                const PASTE_LIMIT: usize = 64 * 1024;
                if let Some(end) = self.pending.windows(PASTE_END.len()).position(|w| w == PASTE_END) {
                    paste.extend(self.pending[..end].iter().take(PASTE_LIMIT.saturating_sub(paste.len())));
                    let mut key = Key::named("Paste", "");
                    key.text = String::from_utf8_lossy(paste).chars().filter(|c| !c.is_control()).collect();
                    out.push(key);
                    self.pending.drain(..end + PASTE_END.len());
                    self.paste = None;
                    continue;
                }
                let consume = self.pending.len().saturating_sub(PASTE_END.len() - 1);
                paste.extend(self.pending[..consume].iter().take(PASTE_LIMIT.saturating_sub(paste.len())));
                self.pending.drain(..consume);
                break;
            }
            if self.discard_csi {
                if let Some(end) = self.pending.iter().position(|c| (0x40..=0x7e).contains(c) || *c == 27) {
                    let consumed = end + usize::from(self.pending[end] != 27);
                    self.pending.drain(..consumed);
                    self.discard_csi = false;
                    continue;
                }
                self.pending.clear();
                break;
            }
            if let Some(kind) = self.string_kind {
                let byte = self.pending[0];
                let wrapped = kind == b'P' && self.reply.starts_with(b"tmux;");
                if byte == 27 && self.pending.len() == 1 { break; }
                let terminated = byte == 0x9c || (byte == 7 && kind == b']')
                    || (byte == 27 && self.pending.get(1) == Some(&b'\\'));
                if terminated {
                    self.pending.drain(..if byte == 27 { 2 } else { 1 });
                    if !self.oversized {
                        if wrapped {
                            // tmux's transport quotes ESC bytes; only capability packets are admitted from it.
                            let inner = &self.reply[5..];
                            if let Some(body) = inner.strip_prefix(b"\x1b_G").and_then(|s| s.strip_suffix(b"\x1b\\")) {
                                let mut body = body.to_vec();
                                body.insert(0, b'G');
                                self.negotiation.string(b'_', &body, now);
                            } else if let Some(body) = inner.strip_prefix(b"\x1b[").and_then(|s| std::str::from_utf8(s).ok()) {
                                self.negotiation.csi(body, now);
                            }
                        } else { self.negotiation.string(kind, &self.reply, now); }
                    }
                    self.string_kind = None;
                    self.reply.clear();
                    continue;
                }
                if byte == 27 && (!wrapped || self.pending.get(1) != Some(&27)) {
                    self.string_kind = None;
                    self.reply.clear();
                    continue;
                }
                if self.reply.len() < REPLY_CAPTURE_LIMIT { self.reply.push(byte); } else { self.oversized = true; }
                self.pending.drain(..if byte == 27 && wrapped { 2 } else { 1 });
                continue;
            }
            let kind = match self.pending[0] { 0x90 => b'P', 0x9d => b']', 0x9e => b'^', 0x9f => b'_', _ => 0 };
            if kind != 0 {
                self.pending.remove(0);
                self.string_kind = Some(kind);
                self.oversized = false;
                continue;
            }
            if self.pending[0] == 27 {
                if self.pending.len() == 1 {
                    if idle { self.pending.remove(0); out.push(Key::named("Escape", "")); }
                    break;
                }
                if matches!(self.pending[1], b']' | b'P' | b'_' | b'^') {
                    self.string_kind = Some(self.pending[1]);
                    self.reply.clear();
                    self.oversized = false;
                    self.pending.drain(..2);
                    continue;
                }
                if self.pending[1] == b'[' {
                    let Some(end) = self.pending[2..].iter().position(|c| (0x40..=0x7e).contains(c) || *c == 27).map(|i| i + 2) else {
                        if self.pending.len() > csi_limit(&self.pending) {
                            self.discard_csi = true;
                            self.pending.clear();
                        }
                        break;
                    };
                    if self.pending[end] == 27 { self.pending.drain(..end); continue; }
                    if end + 1 > csi_limit(&self.pending[..=end]) {
                        self.pending.drain(..=end);
                        continue;
                    }
                    let sequence = String::from_utf8_lossy(&self.pending[2..=end]).into_owned();
                    self.pending.drain(..=end);
                    if sequence == "200~" { self.paste = Some(Vec::new()); continue; }
                    self.negotiation.csi(&sequence, now);
                    if let Some(key) = csi(&sequence) { out.push(key); }
                    continue;
                }
                if self.pending[1] == b'O' {
                    if self.pending.len() < 3 { break; }
                    let name = match self.pending[2] {
                        b'A' => "Up", b'B' => "Down", b'C' => "Right", b'D' => "Left",
                        b'H' => "Home", b'F' => "End", b'P' => "F1", b'Q' => "F2", b'R' => "F3", b'S' => "F4", _ => "",
                    };
                    if !name.is_empty() { out.push(Key::named(name, "")); }
                    self.pending.drain(..3);
                    continue;
                }
                self.pending.remove(0);
                if self.pending[0] == 27 { out.push(Key::named("Escape", "")); continue; }
                let width = utf8_width(self.pending[0]);
                if self.pending.len() < width { self.pending.insert(0, 27); break; }
                match std::str::from_utf8(&self.pending[..width]) {
                    Ok(s) => { if let Some(c) = s.chars().next() { out.push(Key::character(c, "alt")); } self.pending.drain(..width); }
                    Err(_) => { self.pending.remove(0); }
                }
                continue;
            }
            let first = self.pending[0];
            let named = match first { 9 => Some("Tab"), 10 | 13 => Some("Return"), 127 | 8 => Some("Backspace"), _ => None };
            if let Some(name) = named {
                self.pending.remove(0);
                out.push(Key::named(name, ""));
                continue;
            }
            if first < 32 {
                self.pending.remove(0);
                out.push(if first == 0 { Key::named("Space", "ctrl") } else { Key::character((first + 64) as char, "ctrl") });
                continue;
            }
            let width = utf8_width(first);
            if self.pending.len() < width { break; }
            match std::str::from_utf8(&self.pending[..width]) {
                Ok(s) => { if let Some(c) = s.chars().next() { out.push(Key::character(c, "")); } self.pending.drain(..width); }
                Err(_) => { self.pending.remove(0); }
            }
        }
        out
    }
}
// Sample input: ESC[0;1;30028:30028u uses the text budget; ESC[?62;4c uses the control budget.
fn csi_limit(bytes: &[u8]) -> usize {
    let body = &bytes[2..];
    if body.first().is_some_and(u8::is_ascii_digit) {
        if let Some((end, _)) = body.iter().enumerate().filter(|(_, byte)| **byte == b';').nth(1) {
            if end < CSI_CONTROL_LIMIT && body[..end].iter().all(|byte| byte.is_ascii_digit() || matches!(byte, b':' | b';')) {
                return 2 + end + 1 + CSI_TEXT_LIMIT + 1;
            }
        }
    }
    2 + CSI_CONTROL_LIMIT
}
// Sample input: 0xc3 begins a two-byte UTF-8 character; ASCII consumes one byte.
fn utf8_width(first: u8) -> usize {
    match first { 0xc2..=0xdf => 2, 0xe0..=0xef => 3, 0xf0..=0xf4 => 4, _ => 1 }
}
// Sample input: 1;5D, 3;2~, 103:71;2;71u and 57400u retain legacy key meanings.
fn csi(text: &str) -> Option<Key> {
    let last = text.chars().last()?;
    if let Some(body) = text.strip_prefix('<') {
        if !matches!(last, 'M' | 'm') {
            return None;
        }
        let numbers: Vec<u32> = body[..body.len() - 1]
            .split(';')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        if numbers.len() != 3 || numbers[1] == 0 || numbers[2] == 0 {
            return None;
        }
        let button = numbers[0];
        let mut key = Key::named(
            "Pointer",
            match button & 28 {
                4 => "shift",
                8 => "alt",
                16 => "ctrl",
                0 => "",
                _ => "unsupported",
            },
        );
        key.pointer = Some(Pointer {
            button: button & 67,
            x: numbers[1] as usize,
            y: numbers[2] as usize,
            released: last == 'm',
            motion: button & 32 != 0,
        });
        return Some(key);
    }
    let body = &text[..text.len() - 1];
    let fields: Vec<&str> = body.split(';').collect();
    let codes = fields[0].split(':').collect::<Vec<_>>();
    let code = if codes[0].is_empty() { 1 } else { codes[0].parse::<u32>().ok()? };
    let modifiers = fields.get(1).copied().unwrap_or("1").split(':').collect::<Vec<_>>();
    let modifier = if modifiers[0].is_empty() { 1 } else { modifiers[0].parse::<u32>().ok()? }.checked_sub(1)?;
    let mods = if modifier & !(15 | 64 | 128) != 0 { "unsupported" } else { match modifier & 15 {
        0 => "", 1 => "shift", 2 => "alt", 4 => "ctrl", 5 => "ctrlshift",
        8 => "super", 9 => "supershift", 10 => "superalt", _ => "unsupported",
    }};
    if let Some(event) = modifiers.get(1) {
        if !matches!(*event, "" | "1" | "2") { return None; }
    }
    if last == 'u' || (last == '~' && code == 27 && fields.len() == 3) {
        let code = if last == '~' { fields[2].parse::<u32>().ok()? } else { code };
        let functional = match code {
            57348 => "Insert", 57349 => "Delete", 57350 => "Left", 57351 => "Right",
            57352 => "Up", 57353 => "Down", 57354 => "PageUp", 57355 => "PageDown",
            57356 => "Home", 57357 => "End", 57363 => "Menu", 57365 => "F2", 57373 => "F10",
            57417 => "Left", 57418 => "Right", 57419 => "Up", 57420 => "Down",
            57421 => "PageUp", 57422 => "PageDown", 57423 => "Home", 57424 => "End",
            57425 => "Insert", 57426 => "Delete",
            127 => "Backspace", 13 | 57414 => "Return", 9 => "Tab", 27 => "Escape", _ => "",
        };
        if !functional.is_empty() { return Some(Key::named(functional, mods)); }
        let keypad = (57399..=57416).contains(&code);
        let code = match code {
            57399..=57408 => code - 57399 + u32::from(b'0'),
            57409 => u32::from(b'.'), 57410 => u32::from(b'/'), 57411 => u32::from(b'*'),
            57412 => u32::from(b'-'), 57413 => u32::from(b'+'), 57415 => u32::from(b'='),
            57416 => u32::from(b','), _ => code,
        };
        if (57344..=63743).contains(&code) { return None; }
        if code == 0 {
            let text = fields.get(2)?.split(':').map(|s| s.parse::<u32>().ok().and_then(char::from_u32)).collect::<Option<String>>()?;
            if text.is_empty() || text.chars().any(char::is_control) { return None; }
            let mut key = Key::named("Text", mods);
            key.text = text;
            return Some(key);
        }
        let c = char::from_u32(code).filter(|c| !c.is_control())?;
        let shifted = codes.get(1).and_then(|s| s.parse::<u32>().ok()).and_then(char::from_u32);
        let associated = if last == 'u' {
            if let Some(text) = fields.get(2) {
                Some(text.split(':').map(|s| s.parse::<u32>().ok().and_then(char::from_u32)).collect::<Option<String>>()?)
            } else { None }
        } else { None };
        let mut printable = if modifier & 1 != 0 { shifted.unwrap_or(c) } else { c };
        if keypad || mods == "shift" {
            if let Some(text) = associated.as_deref().filter(|text| text.chars().count() == 1) {
                printable = text.chars().next().filter(|c| !c.is_control())?;
            }
        }
        let mut key = Key::character(
            printable,
            if mods == "shift" && (shifted.is_some() || associated.is_some()) {
                ""
            } else {
                mods
            },
        );
        if last == 'u' {
            if let Some(text) = associated {
                key.text = text.chars().filter(|c| !c.is_control()).collect();
            }
            let physical = codes.get(2).or_else(|| codes.get(1)).and_then(|s| s.parse::<u32>().ok()).and_then(char::from_u32);
            if !c.is_ascii() {
                if let Some(base) = physical.filter(char::is_ascii_alphabetic) { key.name = base.to_ascii_uppercase().to_string(); }
            }
        }
        return Some(key);
    }
    let name = match last {
        'A' => "Up",
        'B' => "Down",
        'C' => "Right",
        'D' => "Left",
        'H' => "Home",
        'F' => "End",
        // F1, F2 and F4 in their CSI form, as a modifier or kitty sends them; CSI R stays a cursor report, never F3.
        'P' => "F1",
        'Q' => "F2",
        'S' => "F4",
        'Z' => return Some(Key::named("Tab", "shift")),
        '~' => match code {
            1 | 7 => "Home",
            2 => "Insert",
            3 => "Delete",
            4 | 8 => "End",
            5 => "PageUp",
            6 => "PageDown",
            // The function keys' legacy codes, as xterm, tmux and kitty without its protocol send them.
            11 => "F1",
            12 => "F2",
            13 => "F3",
            14 => "F4",
            15 => "F5",
            17 => "F6",
            18 => "F7",
            19 => "F8",
            20 => "F9",
            21 => "F10",
            23 => "F11",
            24 => "F12",
            _ => return None,
        },
        _ => return None,
    };
    Some(Key::named(name, mods))
}
#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
