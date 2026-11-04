//! Fixtures the ported menu cases share: the oracle's file-row state, an English catalog and row readers.
use super::build::{Context, Peer};
use super::entry::Entry;
use crate::i18n::{I18n, Paths};
use std::fmt::Debug;
use std::path::PathBuf;

pub fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent"),
        catalogs: vec![crate::i18n::test_catalogs::english_only()],
    })
}

// Each case keeps the oracle's own sentence as its name, so a row id resolves to the case that carries it.
pub fn check<T: PartialEq + Debug>(name: &str, got: T, want: T) {
    assert_eq!(got, want, "{name}");
}

pub fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

pub fn peer(id: &str, label: &str) -> Peer {
    Peer {
        id: id.into(),
        label: label.into(),
    }
}

// tests/js/menu.js state(): an image file row with the shipped Settings > Menus switches.
pub fn state() -> Context {
    Context {
        has_row: true,
        selection_count: 1,
        row_mode: 0o100644,
        row_is_file: true,
        clipboard_available: false,
        row_is_symlink: false,
        row_name: "debian-13.iso".into(),
        hidden: strings(&[
            "delete",
            "openTerminal",
            "moveto",
            "copyto",
            "properties",
            "permissions",
            "copyAs",
            "pasteAs",
            "invertSelection",
        ]),
        archive_formats: strings(&["zip"]),
        can_extract: true,
        row_is_archive: false,
        row_is_image: true,
        can_convert: true,
        taildrop_installed: true,
        taildrop_peers: vec![peer("box", "Box")],
        dropbox_installed: true,
        dropbox_path: "/tmp/Dropbox".into(),
        row_in_dropbox: false,
        ..Context::default()
    }
}

pub fn actions(rows: &[Entry]) -> String {
    rows.iter()
        .filter(|r| !r.separator)
        .map(|r| r.action.as_ref())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn entry<'a>(rows: &'a [Entry], action: &str) -> Option<&'a Entry> {
    rows.iter().find(|r| !r.separator && r.action == action)
}

pub fn label(i18n: &I18n, entry: Option<&Entry>) -> String {
    entry
        .and_then(|e| e.label.as_ref())
        .map(|t| t.resolve(i18n))
        .unwrap_or_default()
}

pub fn labels(i18n: &I18n, rows: &[Entry]) -> String {
    rows.iter()
        .filter(|r| !r.separator)
        .map(|r| label(i18n, Some(r)))
        .collect::<Vec<_>>()
        .join("|")
}

pub fn hint(i18n: &I18n, entry: Option<&Entry>) -> String {
    entry
        .and_then(|e| e.hint.as_ref())
        .map_or_else(|| "undefined".into(), |t| t.resolve(i18n))
}

pub fn separated(rows: &[Entry]) -> bool {
    rows.is_empty()
        || (!rows[0].separator
            && !rows[rows.len() - 1].separator
            && !rows
                .windows(2)
                .any(|pair| pair[0].separator && pair[1].separator))
}
