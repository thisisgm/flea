//! Paste and Copy As dispatch: every paste verb reads the same file selection, including the no-watcher fallback; a port of Clipboard.paste, Ops.paste, Ops.pasteLink and Ops.copyAs, with link kinds from the Paste As leaves and Copy As formats from mn1's words.
use super::clipboard::Clip;
use super::wire::paste_image_line;
use super::{Out, PasteAsk, Said};
use crate::i18n::I18n;
use crate::ui_logic_mn1::copyas::{self, Format};

// Sample input: the pasteLink leaf pastes relative, pasteAbsoluteLink absolute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Relative,
    Absolute,
    Hard,
}

impl LinkKind {
    pub fn parse(leaf: &str) -> Option<Self> {
        Some(match leaf {
            "pasteLink" | "relative" => Self::Relative,
            "pasteAbsoluteLink" | "absolute" => Self::Absolute,
            "pasteHardLink" | "hard" => Self::Hard,
            _ => return None,
        })
    }

    pub fn op(self) -> &'static str {
        match self {
            Self::Relative => "relative",
            Self::Absolute => "absolute",
            Self::Hard => "hard",
        }
    }
}

/// A paste with the destination it captured; the host checks it still stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingPaste {
    pub dest: String,
    pub link: Option<LinkKind>,
    pub force_move: bool,
}

impl PendingPaste {
    pub fn transfer(dest: &str) -> Self {
        Self { dest: dest.into(), link: None, force_move: false }
    }

    pub fn forced_move(dest: &str) -> Self {
        Self { dest: dest.into(), link: None, force_move: true }
    }

    pub fn link(dest: &str, kind: LinkKind) -> Self {
        Self { dest: dest.into(), link: Some(kind), force_move: false }
    }
}

/// The clock face a stamped image name is built from, so tests pin it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

// Sample input: 2026-09-23 at 10.14.32 reads "Pasted image 2026-09-23 at 10.14.32.png".
pub fn image_name(stamp: Stamp) -> String {
    format!(
        "Pasted image {:04}-{:02}-{:02} at {:02}.{:02}.{:02}.png",
        stamp.year, stamp.month, stamp.day, stamp.hour, stamp.minute, stamp.second
    )
}

/// What Copy As decided: publish text at once, ask for paths, or stay quiet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyAsOut {
    Text(String),
    AskPaths,
    Quiet,
}

// Copy As names absolute paths, one per line, through the text clipboard; nothing reaches the file clipboard.
pub fn copy_as_text(paths: &[String], format: Format) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    Some(copyas::lines(&refs, format))
}

// With rows instead of paths the backend resolves them first, as clip does; the host keeps the one shared pending slot (G1 pane).
pub fn copy_as_rows() -> CopyAsOut {
    CopyAsOut::AskPaths
}

// The answer to the rows request above; an empty resolution stays quiet.
pub fn copy_as_resolved(list: &[String], format: Format) -> CopyAsOut {
    match copy_as_text(list, format) {
        Some(text) => CopyAsOut::Text(text),
        None => CopyAsOut::Quiet,
    }
}

// Every paste verb reads the same file selection, including the no-watcher fallback; a paste whose destination moved, listed or entered Recent refuses instead of landing elsewhere.
pub fn plan(
    current: &Clip,
    i18n: &I18n,
    pending: &PendingPaste,
    path_now: &str,
    list_in_flight: bool,
    recent_mode: bool,
    stamp: Stamp,
) -> Vec<Out> {
    use super::words;
    if pending.dest != path_now || list_in_flight || recent_mode {
        let said = Said { text: words::dest_changed(i18n), error: false, paste_hint: false };
        return vec![Out::Say(said)];
    }
    if current.paths.is_empty() && pending.link.is_none() {
        // No paths: the backend pastes an image as a stamped file, or answers its own sentence.
        let line = paste_image_line(&pending.dest, &image_name(stamp));
        return vec![Out::Send(line)];
    }
    if current.paths.is_empty() {
        let said = Said { text: i18n.tr("menu.refusal.empty_clipboard"), error: false, paste_hint: false };
        return vec![Out::Say(said)];
    }
    if let Some(kind) = pending.link {
        return vec![Out::Ask(PasteAsk::Link {
            kind,
            paths: current.paths.clone(),
            dest: pending.dest.clone(),
        })];
    }
    let op = if pending.force_move || current.moving { "move" } else { "copy" };
    vec![Out::Ask(PasteAsk::Transfer {
        op: op.into(),
        paths: current.paths.clone(),
        dest: pending.dest.clone(),
        spends_cut: current.moving,
    })]
}
