//! The file chooser's portal request, its answer and its lifetime, shared by every chooser front end.
use crate::json::escape;

mod answer;
mod focus;
mod lifetime;
mod request;
mod result;
pub use answer::{
    accept_step, can_accept, double_action, replace_allowed, same_tap, AcceptState, AcceptStep,
    DoubleAction, Mark, SaveReview,
};
pub use focus::{step_focus, tab_order, Stop};
pub use lifetime::{check_line, Check, Child, Effect, Lifetime, CLOSE_LINE};
pub use request::{request, AcceptLabel, Filter, Mode, Request, Text};
pub use result::{check_result, CheckResult};

// The portal's own response codes, mirrored from tools/flea-portal: a window that opened only picks or refuses.
pub const RESPONSE_OK: i64 = 0;
pub const RESPONSE_CANCELLED: i64 = 1;

// The rail's Recent location; every listing path is absolute, so a token without a leading slash cannot collide.
pub const RECENT: &str = "flea:recent";

// How long a footer notice that is not an error stays.
pub const MESSAGE_LIFE_MS: u64 = 4000;

pub fn saving(req: &Request) -> bool {
    req.mode == Mode::Save
}

pub fn folder_mode(req: &Request) -> bool {
    req.directory || req.mode == Mode::SaveFiles
}

// Marks belong to several-file requests that are not saves.
pub fn marks_allowed(req: &Request) -> bool {
    req.multiple && !saving(req)
}

// The caller's own title wins; the fallback says what the window is for rather than naming Flea.
pub fn title(req: &Request) -> Text {
    if !req.title.is_empty() {
        Text::Caller(req.title.clone())
    } else if req.mode != Mode::Open {
        Text::Key("picker.portal.title.save")
    } else if req.directory {
        Text::Key("picker.portal.title.folder")
    } else {
        Text::Key("picker.portal.title.file")
    }
}

// Only a portal identity is ever shown; a host application has none and nothing is invented for it.
pub fn requester(req: &Request) -> Option<&str> {
    (!req.app.is_empty()).then_some(req.app.as_str())
}

pub fn accept_label(req: &Request, count: usize) -> AcceptLabel {
    let base = if !req.accept.is_empty() {
        Text::Caller(req.accept.clone())
    } else if req.mode == Mode::Save {
        Text::Key("picker.portal.accept.save")
    } else if folder_mode(req) {
        Text::Key("picker.portal.accept.folder")
    } else {
        Text::Key("picker.portal.accept.open")
    };
    AcceptLabel {
        base,
        count: (req.multiple && count > 1).then_some(count),
    }
}

// The footer's keys, naming only what this mode answers; a lost backend leaves Escape alone.
pub fn hints(req: &Request, backend_unavailable: bool) -> &'static str {
    if backend_unavailable {
        "picker.portal.hints.unavailable"
    } else if req.mode == Mode::Save {
        "picker.portal.hints.save"
    } else if !req.multiple {
        "picker.portal.hints.open"
    } else if req.directory {
        "picker.portal.hints.mark_folders"
    } else {
        "picker.portal.hints.mark_files"
    }
}

// The caller's current_filter when it names one of its filters, else the first; no filters is -1.
pub fn current_chip(req: &Request) -> i64 {
    let count = req.filters.len() as i64;
    if count == 0 {
        return -1;
    }
    if (0..count).contains(&req.current_index) {
        return req.current_index;
    }
    req.filters
        .iter()
        .position(|filter| filter.label == req.current)
        .map_or(0, |index| index as i64)
}

// Only an absolute path is a folder, so the caller cannot name the Recent token or other text as the start.
pub fn start_folder<'a>(req: &'a Request, home: &'a str) -> &'a str {
    if req.folder.starts_with('/') {
        &req.folder
    } else {
        home
    }
}

// A one-time seed of the save field from the caller's name, refused like any typed name.
pub fn initial_save_name(req: &Request) -> &str {
    if valid_name(&req.name) {
        &req.name
    } else {
        ""
    }
}

pub fn initial_hidden(req: &Request, stored: bool) -> bool {
    req.hidden.unwrap_or(stored)
}

// The save name is a trust boundary; identical to src/backend/ops.rs valid_name so the two cannot drift.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

pub fn is_recent(location: &str) -> bool {
    location == RECENT
}

pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

// A Recent row is named by its path under the listing base "/", never joined onto the location token.
pub fn row_path(location: &str, name: &str) -> String {
    if is_recent(location) {
        format!("/{name}")
    } else {
        join(location, name)
    }
}

// A symlink whose icon is a folder stays navigable without changing the link's identity.
pub fn directory(dir: bool, mode: u32, icon: &str) -> bool {
    dir || (mode & 0o170000 == 0o120000 && icon == "folder")
}

// encodeURI, then # and ?, the one place a path becomes a URI GLib and the portal accept.
pub fn file_uri(path: &str) -> String {
    const KEPT: &[u8] = b"-_.!~*'();,/:@&=+$";
    let mut out = String::with_capacity(path.len() + 7);
    out.push_str("file://");
    for &byte in
        std::os::unix::ffi::OsStrExt::as_bytes(crate::bytename::text_path(path).as_os_str())
    {
        if byte.is_ascii_alphanumeric() || KEPT.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// What tools/flea-portal reads out of the reply file; a refusal carries no URI and no filter.
pub fn reply(response: i64, uris: &[String], filter: Option<i64>) -> String {
    if response != RESPONSE_OK {
        return format!("{{\"response\":{response}}}");
    }
    let uris = uris
        .iter()
        .map(|uri| format!("\"{}\"", escape(uri)))
        .collect::<Vec<_>>()
        .join(",");
    match filter {
        Some(index) => format!("{{\"response\":{response},\"uris\":[{uris}],\"filter\":{index}}}"),
        None => format!("{{\"response\":{response},\"uris\":[{uris}]}}"),
    }
}

#[cfg(test)]
#[path = "ui_logic_pk1/answer_tests.rs"]
mod answer_tests;
#[cfg(test)]
#[path = "ui_logic_pk1/lifetime_tests.rs"]
mod lifetime_tests;
#[cfg(test)]
#[path = "ui_logic_pk1/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "ui_logic_pk1/exact_name_tests.rs"]
mod exact_name_tests;
