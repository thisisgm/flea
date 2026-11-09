//! Paste and Copy As dispatch: every paste verb reads the same file selection, including the no-watcher fallback; a port of Clipboard.paste, Ops.paste, Ops.pasteLink and Ops.copyAs, with link kinds from the Paste As leaves and Copy As formats from mn1's words.
use super::clipboard::Clip;
use super::{Out, PasteAsk, Said};
use crate::i18n::I18n;
use crate::ui_logic_context_menus::copyas::{self, Format};

// Sample input: the pasteLink leaf pastes relative, pasteAbsoluteLink absolute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Relative,
    Absolute,
    Hard,
}

impl LinkKind {
    // Sample input: "pasteHardLink" or "hard" reads Hard; an unknown leaf reads None.
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

// struct tm counts years from 1900.
const TM_YEAR_BASE: i32 = 1900;

// glibc's struct tm, which localtime_r fills whole; the stamp reads the six clock fields.
#[repr(C)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    _wday: i32,
    _yday: i32,
    _isdst: i32,
    _gmtoff: i64,
    _zone: *const u8,
}

extern "C" {
    fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
}

// Sample input: the local wall clock at a Unix second, as the image name's stamp; None when libc cannot place it.
pub fn stamp_at(seconds: i64) -> Option<Stamp> {
    let mut tm = std::mem::MaybeUninit::<Tm>::uninit();
    // SAFETY: both pointers are valid for the call, and a non-null return means tm was written whole.
    let tm = unsafe {
        if localtime_r(&seconds, tm.as_mut_ptr()).is_null() {
            return None;
        }
        tm.assume_init()
    };
    Some(Stamp {
        year: tm.year + TM_YEAR_BASE,
        month: (tm.mon + 1) as u32,
        day: tm.mday as u32,
        hour: tm.hour as u32,
        minute: tm.min as u32,
        second: tm.sec as u32,
    })
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

// Copy As names absolute paths, one per line, through the text clipboard, except Relative, read from base, the listed folder; nothing reaches the file clipboard.
pub fn copy_as_text(paths: &[String], format: Format, base: &str) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    Some(copyas::lines(&refs, format, base))
}

// The answer to the rows request above, read from the base the request kept; an empty resolution stays quiet.
pub fn copy_as_resolved(list: &[String], format: Format, base: &str) -> CopyAsOut {
    match copy_as_text(list, format, base) {
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
        return vec![Out::Ask(PasteAsk::Image {
            dest: pending.dest.clone(),
            name: image_name(stamp),
        })];
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
