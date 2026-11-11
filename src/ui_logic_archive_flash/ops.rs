//! The password flows: what a submit sends, what it sticks, and what an empty selection says.
use super::cards;
use crate::i18n::I18n;

/// What asking for the With-password compress does: resolve the selection first, or say there is nothing to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Paths,
    NothingToActOn,
}

// Sample input: an empty selection says there is nothing to act on and opens no card.
pub fn open_compress_password(indices: &[usize]) -> Ask {
    if indices.is_empty() { Ask::NothingToActOn } else { Ask::Paths }
}

// Sample input: an empty resolved selection says the same and opens no card.
pub fn compress_password_resolved(paths: &[&str]) -> Ask {
    if paths.is_empty() { Ask::NothingToActOn } else { Ask::Paths }
}

// Sample input: no rows acts on nothing, and the host says so.
pub fn nothing_to_act_on(i18n: &I18n) -> String {
    i18n.tr("archive.no_target")
}

/// What a With-password submit sends: the wire line, and the sticky line only when the send went out at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submit {
    pub line: String,
    pub sticky: Option<String>,
}

// Sample input: two paths to 7z send the secret to the wire, never the card, and stick the running line.
pub struct CompressRequest<'a> {
    pub paths: &'a [&'a str],
    pub dest: &'a str,
    pub format: &'a str,
    pub password: &'a str,
    pub encrypt_names: bool,
    pub menu_id: u64,
}

pub fn submit_compress_password(i18n: &I18n, request: &CompressRequest<'_>, queueing: bool) -> Option<Submit> {
    if request.paths.is_empty() {
        return None;
    }
    let line = super::wire::compress_password_request(
        request.paths,
        request.dest,
        request.format,
        request.password,
        request.encrypt_names,
        request.menu_id,
    );
    let sticky = if queueing { None } else { Some(cards::compress_sticky(i18n, request.paths.len(), request.format)) };
    Some(Submit { line, sticky })
}

// Sample input: the retry goes to the source the card was opened over; a retry with no source sends nothing.
pub fn submit_extract_password(path: Option<&str>, dest: &str, password: &str, menu_id: u64) -> Option<String> {
    let path = path?;
    if path.is_empty() {
        return None;
    }
    Some(super::wire::extract_password_request(path, dest, password, menu_id))
}
