// What localsend-cli draws, read back. It is a full-screen program, so its panel, its event log and
// its refusals all arrive as one stream of escape sequences and text; this turns that stream into the
// rows a menu can offer and the sentence a refusal is. Split out of src/backend/localsend.rs, which
// owns the terminal those bytes come through and the keys that go back.

// One device the CLI has discovered, as its panel numbers it: the number is what navigation counts.
pub struct Peer {
    pub index: usize,
    pub name: String,
    pub address: String,
}

// Sample panel text, with the escape sequences already gone. The rows are numbered by the CLI itself
// and the paired devices are numbered before the discovered ones:
// Paired
//   (none)
// Discovered
//   [1] Clean Lemon (192.168.21.23)
pub fn parse_peers(text: &str) -> Vec<Peer> {
    let mut out: Vec<Peer> = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 3 < bytes.len() {
        // The number can run past one digit: a tenth device is "[10] ".
        let digits = bytes[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        let close = i + 1 + digits;
        if bytes[i] != '[' || digits == 0 || close + 1 >= bytes.len() || bytes[close] != ']' || bytes[close + 1] != ' ' {
            i += 1;
            continue;
        }
        let index = bytes[i + 1..close].iter().collect::<String>().parse().unwrap_or(0);
        let rest: String = bytes[close + 2..].iter().collect();
        // A device name can hold anything but a newline, so the address is taken from the last " ("
        // of the row and the name is everything before it.
        let row = match rest.find(|c| c == '\n' || c == '│') {
            Some(end) => rest[..end].to_string(),
            None => rest,
        };
        if let (Some(open), Some(close)) = (row.rfind(" ("), row.rfind(')')) {
            if open < close {
                let name = row[..open].trim().to_string();
                let address = row[open + 2..close].trim().to_string();
                if index > 0 && !name.is_empty() && !out.iter().any(|p| p.index == index) {
                    out.push(Peer { index, name, address });
                }
            }
        }
        i = close + 2;
    }
    out.sort_by_key(|p| p.index);
    out
}

// Sample: "ESC[7;4H[2]ESC[7;8HPeer" gives "[2]Peer", which is why the bridge reads localsendscreen.rs.
// A sequence is ESC [ , any parameter bytes, one final letter; this stays only as the tests' contrast.
#[cfg(test)]
pub fn strip_ansi(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') | Some(']') | Some('(') | Some(')') => {
                let opener = chars.next().unwrap_or(' ');
                for c in chars.by_ref() {
                    // A CSI ends on its final byte, and the two-character sets end on their one letter.
                    if opener == '[' && c.is_ascii_alphabetic() { break }
                    if opener == ']' && (c == '\u{7}' || c == '\u{1b}') { break }
                    if opener != '[' && opener != ']' { break }
                }
            }
            _ => { chars.next(); }
        }
    }
    out
}

// The CLI prints one line and gives up when it cannot start. The port is the case that happens here:
// a LocalSend already running on this box owns 53317, and only one program can receive on it.
// Sample: "Error: Address already in use (os error 98)"
pub fn refusal(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.trim_start().starts_with("Error:"))?;
    let said = line.trim().trim_start_matches("Error:").trim();
    if said.contains("Address already in use") {
        return Some("LocalSend is already running. Close it, then try again.".into())
    }
    Some(format!("LocalSend could not start: {}.", said.trim_end_matches('.')))
}

// The words of the device's last event line. Sample: "S Clean Lemon: Declined" gives "Declined".
pub fn said<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let head = format!("S {}: ", name);
    let line = text.lines().map(str::trim).rev().find(|l| l.starts_with(&head))?;
    Some(line[head.len()..].trim())
}

// A failed upload names its request URL, session token included, which is no help to a person.
// Sample: "Failed to upload x.bin: error sending request for url (https://...&token=...)".
pub fn could_not(name: &str, said: &str) -> String {
    let said = said.split(" for url (").next().unwrap_or(said);
    let said = said.split(" (http").next().unwrap_or(said);
    format!("LocalSend could not send to {}: {}.", name, said.trim_end_matches('.'))
}

// What a running transfer has moved, from its progress line. Sample line, the bar shortened:
// "S Clean Lemon [####----] 241.2 MB / 307.2 MB [12.5 MB/s] [ETA: 5s]" gives "241.2 MB / 307.2 MB".
pub fn progress(text: &str, name: &str) -> Option<String> {
    let head = format!("S {} [", name);
    let line = text.lines().map(str::trim).rev().find(|l| l.starts_with(&head))?;
    let moved = line[head.len()..].split_once("] ")?.1;
    Some(moved.split(" [").next().unwrap_or(moved).to_string())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_gives_up_its_devices_however_it_was_drawn() {
        let panel = "\u{1b}[?1049h\u{1b}[39m\u{1b}[2J┌Devices─────┐│Paired││  (none)││Discovered││  [1] Clean Lemon (192.168.21.23)│└────┘";
        let peers = parse_peers(&strip_ansi(panel));
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].index, 1);
        assert_eq!(peers[0].name, "Clean Lemon");
        assert_eq!(peers[0].address, "192.168.21.23");
    }

    #[test]
    fn a_repainted_panel_names_each_device_once_and_in_its_own_order() {
        let panel = "[2] Second (10.0.0.2)\n[1] First (10.0.0.1)\n[2] Second (10.0.0.2)\n[1] First (10.0.0.1)";
        let peers = parse_peers(panel);
        assert_eq!(peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["First", "Second"]);
    }

    #[test]
    fn a_name_with_its_own_brackets_keeps_them_and_the_address_is_the_last_pair() {
        let peers = parse_peers("[3] GM (work) laptop (192.168.21.7)");
        assert_eq!(peers[0].name, "GM (work) laptop");
        assert_eq!(peers[0].address, "192.168.21.7");
        assert_eq!(peers[0].index, 3);
    }

    #[test]
    fn a_tenth_device_keeps_its_two_digit_number() {
        let peers = parse_peers("[9] Ninth (10.0.0.9)\n[10] Tenth (10.0.0.10)\n[11] Eleventh (10.0.0.11)");
        assert_eq!(peers.iter().map(|p| (p.index, p.name.as_str())).collect::<Vec<_>>(),
                   vec![(9, "Ninth"), (10, "Tenth"), (11, "Eleventh")]);
    }

    #[test]
    fn a_row_that_is_not_a_device_is_not_one() {
        assert!(parse_peers("Paired\n  (none)\nDiscovered\n  (none)").is_empty());
        assert!(parse_peers("[0] Nobody (10.0.0.1)").is_empty());
        assert!(parse_peers("[1] no address here").is_empty());
    }

    #[test]
    fn escape_sequences_leave_and_the_text_between_them_stays() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\u{1b}(Bkept"), "kept");
    }

    // A device is named the same way wherever the CLI drew it, in its panel or in the log under it,
    // which is what lets discovery read the whole stream and ignore where on screen the row landed.
    #[test]
    fn a_device_reads_the_same_from_the_panel_and_from_the_log() {
        let logged = "┌Devices┐│Paired││  (none)││Discovered││  (none)│└────┘  [1] Clean Lemon (192.168.21.23)";
        let listed = "┌Devices┐│Paired││  (none)││Discovered││  [1] Clean Lemon (192.168.21.23)│└────┘";
        for drawn in [logged, listed] {
            let peers = parse_peers(drawn);
            assert_eq!(peers.len(), 1, "{}", drawn);
            assert_eq!(peers[0].name, "Clean Lemon");
            assert_eq!(peers[0].address, "192.168.21.23");
        }
    }

    #[test]
    fn a_cli_that_could_not_start_says_which_refusal_it_was() {
        assert_eq!(refusal("Error: Address already in use (os error 98)\r\nD").as_deref(),
                   Some("LocalSend is already running. Close it, then try again."));
        assert_eq!(refusal("Error: no route to host.").as_deref(),
                   Some("LocalSend could not start: no route to host."));
        assert!(refusal("┌Devices┐│Discovered││  [1] Clean Lemon (192.168.21.23)│").is_none());
    }
}
