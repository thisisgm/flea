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
}
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    string_control: bool,
    kitty_reply: Option<Vec<u8>>,
    pub kitty: bool,
    pub sixel: bool,
    paste: Option<Vec<u8>>,
}
impl Decoder {
    pub fn feed(&mut self, bytes: &[u8], idle: bool) -> Vec<Key> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        while !self.pending.is_empty() {
            if let Some(paste) = &mut self.paste {
                const PASTE_END: &[u8] = b"\x1b[201~";
                const PASTE_LIMIT: usize = 64 * 1024;
                if let Some(end) = self
                    .pending
                    .windows(PASTE_END.len())
                    .position(|w| w == PASTE_END)
                {
                    paste.extend(
                        self.pending[..end]
                            .iter()
                            .take(PASTE_LIMIT.saturating_sub(paste.len())),
                    );
                    let mut key = Key::named("Paste", "");
                    key.text = String::from_utf8_lossy(paste)
                        .chars()
                        .filter(|c| !c.is_control())
                        .collect();
                    out.push(key);
                    self.pending.drain(..end + PASTE_END.len());
                    self.paste = None;
                    continue;
                }
                let keep = PASTE_END.len() - 1;
                let consume = self.pending.len().saturating_sub(keep);
                paste.extend(
                    self.pending[..consume]
                        .iter()
                        .take(PASTE_LIMIT.saturating_sub(paste.len())),
                );
                self.pending.drain(..consume);
                break;
            }
            if self.string_control {
                let end = self.pending.iter().enumerate().find_map(|(i, byte)| {
                    if *byte == 7 || *byte == 0x9c {
                        Some((i, i + 1))
                    } else if *byte == 27 && self.pending.get(i + 1) == Some(&b'\\') {
                        Some((i, i + 2))
                    } else {
                        None
                    }
                });
                let keep = usize::from(end.is_none() && self.pending.last() == Some(&27));
                let content = end.map_or(self.pending.len() - keep, |(content, _)| content);
                if let Some(reply) = &mut self.kitty_reply {
                    const REPLY_LIMIT: usize = 128;
                    if reply.len() + content <= REPLY_LIMIT { reply.extend_from_slice(&self.pending[..content]); }
                    else { self.kitty_reply = None; }
                }
                if let Some((_, consumed)) = end {
                    if self.kitty_reply.take().is_some_and(|reply| reply == b"Gi=31;OK") { self.kitty = true; }
                    self.pending.drain(..consumed);
                    self.string_control = false;
                    continue;
                }
                // Keep only a split ST introducer; terminal reply payloads never become listing keys.
                let escape = self.pending.last() == Some(&27);
                self.pending.clear();
                if escape {
                    self.pending.push(27);
                }
                break;
            }
            if matches!(self.pending[0], 0x90 | 0x9d | 0x9e | 0x9f) {
                self.kitty_reply = (self.pending[0] == 0x9f).then(Vec::new);
                self.pending.remove(0);
                self.string_control = true;
                continue;
            }
            if self.pending[0] == 27 {
                if self.pending.len() == 1 {
                    if idle {
                        self.pending.remove(0);
                        out.push(Key::named("Escape", ""));
                    }
                    break;
                }
                if matches!(self.pending[1], b']' | b'P' | b'_' | b'^') {
                    self.kitty_reply = (self.pending[1] == b'_').then(Vec::new);
                    self.pending.drain(..2);
                    self.string_control = true;
                    continue;
                }
                if self.pending[1] == b'[' {
                    let Some(end) = self.pending[2..]
                        .iter()
                        .position(|c| (0x40..=0x7e).contains(c))
                        .map(|i| i + 2)
                    else {
                        if self.pending.len() > 64 {
                            self.pending.clear();
                        }
                        break;
                    };
                    let sequence = String::from_utf8_lossy(&self.pending[2..=end]).into_owned();
                    self.pending.drain(..=end);
                    if sequence == "200~" {
                        self.paste = Some(Vec::new());
                        continue;
                    }
                    if sequence.starts_with('?')
                        && sequence.ends_with('c')
                        && sequence[1..sequence.len() - 1].split(';').any(|p| p == "4")
                    {
                        self.sixel = true;
                    }
                    if let Some(key) = csi(&sequence) {
                        out.push(key);
                    }
                    continue;
                }
                if self.pending[1] == b'O' && self.pending.len() < 3 {
                    break;
                }
                if self.pending[1] == b'O' && self.pending.len() >= 3 {
                    let name = match self.pending[2] {
                        b'A' => "Up",
                        b'B' => "Down",
                        b'C' => "Right",
                        b'D' => "Left",
                        b'H' => "Home",
                        b'F' => "End",
                        _ => "",
                    };
                    if !name.is_empty() {
                        out.push(Key::named(name, ""));
                    }
                    self.pending.drain(..3);
                    continue;
                }
                let c = self.pending[1];
                self.pending.drain(..2);
                if c.is_ascii() {
                    out.push(Key::character(c as char, "alt"));
                }
                continue;
            }
            let first = self.pending[0];
            let named = match first {
                9 => Some("Tab"),
                10 | 13 => Some("Return"),
                127 | 8 => Some("Backspace"),
                _ => None,
            };
            if let Some(name) = named {
                self.pending.remove(0);
                out.push(Key::named(name, ""));
                continue;
            }
            if first < 32 {
                self.pending.remove(0);
                out.push(if first == 0 {
                    Key::named("Space", "ctrl")
                } else {
                    Key::character((first + 64) as char, "ctrl")
                });
                continue;
            }
            let width = if first < 128 {
                1
            } else if first < 224 {
                2
            } else if first < 240 {
                3
            } else {
                4
            };
            if self.pending.len() < width {
                break;
            }
            if let Ok(s) = std::str::from_utf8(&self.pending[..width]) {
                if let Some(c) = s.chars().next() {
                    out.push(Key::character(c, ""));
                }
            }
            self.pending.drain(..width);
        }
        out
    }
}
// Sample input: 1;5D, 2;2~, 32;5u; kitty flag 1 leaves ordinary text in legacy form.
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
    let values: Vec<u32> = text[..text.len() - 1]
        .split(';')
        .map(|v| v.split(':').next().unwrap_or("").parse().unwrap_or(0))
        .collect();
    let code = *values.first().unwrap_or(&0);
    let modifier = values.get(1).copied().unwrap_or(1).saturating_sub(1);
    let mods = match modifier & 15 {
        0 => "",
        1 => "shift",
        2 => "alt",
        4 => "ctrl",
        5 => "ctrlshift",
        8 => "super",
        9 => "supershift",
        10 => "superalt",
        _ => "unsupported",
    };
    if last == 'u' {
        // Sample input: 113;1:3u is a kitty key release, which must never repeat an action.
        if text[..text.len() - 1]
            .split(';')
            .nth(1)
            .and_then(|v| v.split(':').nth(1))
            == Some("3")
        {
            return None;
        }
        let functional = match code {
            57348 => "Insert",
            57349 => "Delete",
            57350 => "Left",
            57351 => "Right",
            57352 => "Up",
            57353 => "Down",
            57354 => "PageUp",
            57355 => "PageDown",
            57356 => "Home",
            57357 => "End",
            57363 => "Menu",
            57365 => "F2",
            57373 => "F10",
            127 => "Backspace",
            _ => "",
        };
        if !functional.is_empty() {
            return Some(Key::named(functional, mods));
        }
        return char::from_u32(code).map(|c| match c {
            '\r' => Key::named("Return", mods),
            '\t' => Key::named("Tab", mods),
            '\u{1b}' => Key::named("Escape", mods),
            _ => Key::character(c, mods),
        });
    }
    let name = match last {
        'A' => "Up",
        'B' => "Down",
        'C' => "Right",
        'D' => "Left",
        'H' => "Home",
        'F' => "End",
        'Q' => "F2",
        'Z' => return Some(Key::named("Tab", "shift")),
        '~' => match code {
            1 | 7 => "Home",
            2 => "Insert",
            3 => "Delete",
            4 | 8 => "End",
            5 => "PageUp",
            6 => "PageDown",
            12 => "F2",
            21 => "F10",
            _ => return None,
        },
        _ => return None,
    };
    Some(Key::named(name, mods))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_replies_never_become_file_actions() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b_Gi=42;error: ddd", false).is_empty());
        assert!(decoder.feed(&vec![b'd'; 4096], false).is_empty());
        assert!(decoder.feed(b"\x1b", false).is_empty());
        assert_eq!(decoder.feed(b"\\j", false), vec![Key::character('j', "")]);
        assert!(decoder
            .feed(b"\x1b]52;c;ddd\x07\x1bPddd\x1b\\", false)
            .is_empty());
    }

    #[test]
    fn split_sequences_and_utf8_are_retained() {
        let mut d = Decoder::default();
        assert!(d.feed(b"\x1b[1;", false).is_empty());
        assert_eq!(d.feed(b"5D", false), vec![Key::named("Left", "ctrl")]);
        assert!(d.feed(&[0xc3], false).is_empty());
        assert_eq!(d.feed(&[0xa9], false)[0].text, "é");
    }
    #[test]
    fn legacy_and_kitty_forms_coexist() {
        let mut d = Decoder::default();
        assert_eq!(
            d.feed(b" \x1b[32;5u\x1b[2;2~", false),
            vec![
                Key::character(' ', ""),
                Key::character(' ', "ctrl"),
                Key::named("Insert", "shift")
            ]
        );
        assert!(d.feed(b"\x1b", false).is_empty());
        assert_eq!(d.feed(b"", true), vec![Key::named("Escape", "")]);
    }
    #[test]
    fn mouse_modifiers_paste_and_release_keep_separate_meanings() {
        let mut decoder = Decoder::default();
        let mouse = decoder.feed(b"\x1b[<4;12;3M", false);
        assert_eq!(mouse[0].mods, "shift");
        assert_eq!(
            mouse[0].pointer,
            Some(Pointer {
                button: 0,
                x: 12,
                y: 3,
                released: false,
                motion: false
            })
        );
        assert!(decoder.feed(b"\x1b[200~ddq", false).is_empty());
        let pasted = decoder.feed(b"\x1b[201~", false);
        assert_eq!(pasted.len(), 1);
        assert_eq!(pasted[0].name, "Paste");
        assert_eq!(pasted[0].text, "ddq");
        assert!(decoder.feed(b"\x1b[113;1:3u", false).is_empty());
        assert_eq!(
            decoder.feed(b"\x1b[57365u\x1b[1;11D", false),
            vec![Key::named("F2", ""), Key::named("Left", "superalt")]
        );
        assert_eq!(decoder.feed(b"\x1b[1;7D", false)[0].mods, "unsupported");
    }
    #[test]
    fn graphics_capability_requires_the_requested_terminal_reply() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b_Gi=30;OK\x1b\\", false).is_empty());
        assert!(!decoder.kitty);
        assert!(decoder.feed(b"\x1b_Gi=31;ENOENT\x1b\\", false).is_empty());
        assert!(!decoder.kitty);
        assert!(decoder.feed(b"\x1b_Gi=31;O", false).is_empty());
        assert!(decoder.feed(b"K\x1b", false).is_empty());
        assert!(decoder.feed(b"\\", false).is_empty());
        assert!(decoder.kitty);
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b[?62;4;22c", false).is_empty());
        assert!(decoder.sixel);
        assert!(!decoder.kitty);
    }
    #[test]
    fn native_menu_keys_keep_their_context_modifiers() {
        let mut decoder = Decoder::default();
        // Kitty with NumLock enabled sends CSI 1;129Q for physical F2.
        assert_eq!(decoder.feed(b"\x1b[1;129Q", false), vec![Key::named("F2", "")]);
        assert_eq!(
            decoder.feed(b"\x1b[21;2~\x1b[57373;2u\x1b[57363u", false),
            vec![
                Key::named("F10", "shift"),
                Key::named("F10", "shift"),
                Key::named("Menu", ""),
            ]
        );
    }
}
