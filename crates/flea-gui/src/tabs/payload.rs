use super::{text, Info, TearOff, TAB_SORT_ENV, TEAR_OFF_ENV};
use crate::navigation::{
    folder_state::{Order, ORDERS},
    sort::Snapshot,
};
use flea::jsondoc;
use std::process::Command;

// JSON carries every path byte verbatim, CR and LF included.
pub fn payload(pid: &str, token: &str, path: &str, view: &str, cursor: &str) -> String {
    let fields: Vec<String> = [pid, token, path, view, cursor]
        .iter()
        .map(|field| format!("\"{}\"", flea::json::escape(field)))
        .collect();
    format!("[{}]", fields.join(","))
}

// The optional sixth field extends the private handoff; legacy five-field offers still parse.
pub fn payload_with_sort(
    pid: &str,
    token: &str,
    view: &str,
    cursor: &str,
    snapshot: &Snapshot,
) -> String {
    let mut text = payload(pid, token, &snapshot.path, view, cursor);
    text.pop();
    text.push_str(&format!(
        ",{{\"path\":\"{}\",\"key\":\"{}\",\"desc\":{}}}]",
        flea::json::escape(&snapshot.path),
        snapshot.order.key,
        snapshot.order.desc
    ));
    text
}

// Sample input: ["111","lift-1","/tmp/folder","list","note.txt"].
pub fn parse(payload: &str) -> Option<Info> {
    let fields = jsondoc::parse(payload).ok()?;
    let fields = fields.as_array()?;
    if !matches!(fields.len(), 5 | 6) {
        return None;
    }
    let pid = text(&fields[0]);
    let token = text(&fields[1]);
    let path = text(&fields[2]);
    if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if token.is_empty() || token.chars().any(|c| c < ' ' || c == '\u{7f}') {
        return None;
    }
    if !path.starts_with('/') || path.contains('\0') {
        return None;
    }
    let sort = if let Some(value) = fields.get(5) {
        let snapshot_path = value.get("path")?.as_str()?;
        if snapshot_path != path {
            return None;
        }
        let key = value.get("key")?.as_str()?;
        let key = ORDERS.iter().copied().find(|order| *order == key)?;
        Some(Snapshot {
            path: snapshot_path.into(),
            order: Order {
                key,
                desc: value.get("desc")?.as_bool()?,
            },
        })
    } else {
        None
    };
    Some(Info {
        pid,
        token,
        path,
        view: text(&fields[3]),
        cursor: text(&fields[4]),
        sort,
    })
}

// The torn-off window opens the folder and acknowledges the lift once its listing lands; a failed spawn keeps the source.
pub fn tear_off_command(bin: &str, tear: &TearOff) -> Command {
    let mut command = Command::new(bin);
    command
        .args(["--gui", &tear.path])
        .env("FLEA_FRONTEND", "gpui")
        .env_remove("FLEA_PATH")
        .env_remove("FLEA_SELECT")
        .env(TEAR_OFF_ENV[0], &tear.pid)
        .env(TEAR_OFF_ENV[1], &tear.cursor)
        .env(TEAR_OFF_ENV[2], &tear.token);
    command.env_remove(TAB_SORT_ENV);
    if let Some(snapshot) = &tear.sort {
        command.env(
            TAB_SORT_ENV,
            payload_with_sort(&tear.pid, &tear.token, "list", &tear.cursor, snapshot),
        );
    }
    command
}

// A new window is not a tear-off, so it never inherits the hand-off its own window was started with.
pub fn fresh_launch(command: &mut Command) {
    command.env_remove(TAB_SORT_ENV);
    for name in TEAR_OFF_ENV {
        command.env_remove(name);
    }
}

// The hand-off a tear-off launch read once; it acknowledges when the primary pane opens the folder it names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub pid: String,
    pub token: String,
    pub cursor: String,
    pub path: String,
    pub sort: Option<Snapshot>,
}

impl Launch {
    pub fn read(get: impl Fn(&str) -> Option<String>, path: &str) -> Self {
        let pid = get(TEAR_OFF_ENV[0]).unwrap_or_default();
        let token = get(TEAR_OFF_ENV[2]).unwrap_or_default();
        let sort = get(TAB_SORT_ENV)
            .and_then(|text| parse(&text))
            .filter(|info| info.path == path && info.pid == pid && info.token == token)
            .and_then(|info| info.sort);
        Self {
            pid,
            cursor: get(TEAR_OFF_ENV[1]).unwrap_or_default(),
            token,
            path: path.into(),
            sort,
        }
    }

    pub fn opened(&mut self, path: &str) -> Option<(String, String)> {
        let valid = !self.pid.is_empty() && self.pid.bytes().all(|byte| byte.is_ascii_digit());
        if path != self.path || !valid || self.token.is_empty() {
            return None;
        }
        Some((self.pid.clone(), std::mem::take(&mut self.token)))
    }
}
