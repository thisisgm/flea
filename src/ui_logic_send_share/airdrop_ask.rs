//! Receiving's answer to each request: owly asks `accept? [y/N]` on stderr, and Flea asks the user in a notification.
use super::airdrop::quoted;
use crate::i18n::I18n;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

// owly prints its prompt alone at a line start with no newline; a name quoting it never starts a line.
pub const PROMPT: &str = "accept? [y/N] ";
// owly declines by itself 15 s after its prompt; Flea answers inside that, so no late answer lands on the next request.
pub const ASK_SECS: u64 = 12;
const READ_CHUNK: usize = 4096;
// owly receive's stderr when a transfer it accepted stops, and when the sender takes a request back.
const FAILED: &str = "transfer failed: ";
const WITHDRAWN: &str = "the request was withdrawn";
// The footer's catalog key for a saved file or link.
const RECEIVED: &str = "send.providers.airdrop_received";

// One request as owly printed it: the sender's own name, empty when it gave none, and each offered file or link.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub sender: String,
    pub files: Vec<String>,
}

// Sample input: `Some("Jane's iPhone") (Some("iPhone15,2")) wants to send:`, owly's Debug print of the sender and model.
pub fn request_line(line: &str) -> Option<Request> {
    let head = line.trim_end().strip_suffix(" wants to send:")?;
    let sender = head
        .strip_prefix("Some(")
        .and_then(quoted)
        .map(|(name, _)| name)
        .unwrap_or_default();
    Some(Request {
        sender,
        files: Vec::new(),
    })
}

// Sample input: `  "IMG_0001.HEIC" public.heic 2400000 bytes`, one offered file under its request.
pub fn offer_line(line: &str) -> Option<String> {
    line.strip_prefix("  ")
        .and_then(quoted)
        .map(|(name, _)| name)
}

// Reads owly's stderr to its end, answers each prompt as `answer` decides, and hands every line the window says to `heard`.
pub fn serve(
    mut stderr: impl Read,
    mut stdin: impl Write,
    mut answer: impl FnMut(&Request) -> bool,
    mut heard: impl FnMut(String),
) {
    let (mut line, mut request, mut buf) = (Vec::new(), Request::default(), [0u8; READ_CHUNK]);
    while let Ok(n) = stderr.read(&mut buf) {
        if n == 0 {
            break;
        }
        for &byte in &buf[..n] {
            if byte == b'\n' {
                let text = String::from_utf8_lossy(&line).into_owned();
                if let Some(next) = request_line(&text) {
                    request = next;
                } else if let Some(file) = offer_line(&text) {
                    request.files.push(file);
                } else if text.starts_with("owly: ")
                    || text.starts_with(FAILED)
                    || text == WITHDRAWN
                {
                    heard(text);
                }
                line.clear();
                continue;
            }
            line.push(byte);
            if line == PROMPT.as_bytes() {
                let reply: &[u8] = if answer(&request) { b"y\n" } else { b"n\n" };
                let _ = stdin.write_all(reply).and_then(|()| stdin.flush());
                request = Request::default();
                line.clear();
            }
        }
    }
}

// What the receiver said after an Accept: a saved file by its name, a link's text, or why the transfer stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heard {
    File(String),
    Link(String),
    Failed(String),
    Withdrawn,
}

// Sample input: `/home/u/Downloads/IMG_0001.HEIC` or `"https://example.com/"` on stdout, `transfer failed: reset` on stderr.
pub fn heard(line: &str) -> Option<Heard> {
    if let Some(reason) = line.strip_prefix(FAILED) {
        return Some(Heard::Failed(reason.trim().to_string()));
    }
    if line == WITHDRAWN {
        return Some(Heard::Withdrawn);
    }
    if line.starts_with('"') {
        return quoted(line).map(|(link, _)| Heard::Link(link));
    }
    let name = line.strip_prefix('/')?.rsplit('/').next()?;
    (!name.is_empty()).then(|| Heard::File(name.to_string()))
}

// The footer's sentence and whether it is an error; `count` files of one transfer are counted after the first.
pub fn heard_said(i18n: &I18n, heard: &Heard, count: usize) -> (String, bool) {
    match heard {
        Heard::File(name) if count == 1 => (i18n.tr_args(RECEIVED, &[name]), false),
        Heard::File(_) => {
            let what = i18n.trn("items.count", count as i64);
            (i18n.tr_args(RECEIVED, &[&what]), false)
        }
        Heard::Link(link) => (i18n.tr_args(RECEIVED, &[link]), false),
        Heard::Failed(reason) if !reason.is_empty() => (
            i18n.tr_args("send.providers.airdrop_reason", &[reason]),
            true,
        ),
        Heard::Failed(_) | Heard::Withdrawn => (i18n.tr("send.providers.airdrop_unfinished"), true),
    }
}

// The body is notification markup, so the sender's and the files' own text is escaped.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// Who is sending and what: one file by name, several counted.
pub fn ask_body(i18n: &I18n, request: &Request) -> String {
    let who = match request.sender.as_str() {
        "" => i18n.tr("send.providers.airdrop_someone"),
        name => escaped(name),
    };
    let what = match request.files.as_slice() {
        [only] => escaped(only),
        files => i18n.trn("items.count", files.len() as i64),
    };
    i18n.tr_args("send.providers.airdrop_asks", &[&who, &what])
}

// Accept and Decline as the notification's actions; notify-send prints its id at once, then the chosen one's name.
pub fn ask_argv(body: &str, accept: &str, decline: &str) -> Vec<String> {
    vec![
        "timeout".into(),
        ASK_SECS.to_string(),
        "notify-send".into(),
        "--app-name=Flea".into(),
        "--print-id".into(),
        format!("--expire-time={}", ASK_SECS * 1000),
        format!("--action=accept={accept}"),
        format!("--action=decline={decline}"),
        "--".into(),
        "AirDrop".into(),
        body.into(),
    ]
}

// The close runs before owly hears the answer, so it is bounded too.
pub const CLOSE_SECS: u64 = 1;

// Sample output: `42\naccept\n`, the notification's id and then the action, which is absent when the ask timed out.
pub fn asked(stdout: &str) -> (Option<u32>, bool) {
    let mut lines = stdout.lines().map(str::trim);
    let id = lines.next().and_then(|id| id.parse().ok());
    (id, id.is_some() && lines.next() == Some("accept"))
}

// The notification spec's own close by id; it touches no other sender's notification.
pub fn close_argv(id: u32) -> Vec<String> {
    vec![
        "gdbus".into(),
        "call".into(),
        "--session".into(),
        format!("--timeout={CLOSE_SECS}"),
        "--dest=org.freedesktop.Notifications".into(),
        "--object-path=/org/freedesktop/Notifications".into(),
        "--method=org.freedesktop.Notifications.CloseNotification".into(),
        id.to_string(),
    ]
}

// Only the user's Accept accepts; Decline, a dismissed or expired notification, or no notify-send declines.
pub fn ask(i18n: &I18n, request: &Request) -> bool {
    let argv = ask_argv(
        &ask_body(i18n, request),
        &i18n.tr("send.providers.airdrop_accept"),
        &i18n.tr("send.providers.airdrop_decline"),
    );
    let Ok(out) = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    let (id, accepted) = asked(&String::from_utf8_lossy(&out.stdout));
    // An answered or expired ask is withdrawn so it never lingers; a failed close leaves only the card's own 12 s.
    if let Some(close) = id.map(close_argv) {
        let _ = Command::new(&close[0])
            .args(&close[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    accepted
}

#[cfg(test)]
#[path = "airdrop_ask_tests.rs"]
mod tests;
