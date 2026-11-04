//! Clipboard file selections shared by every front end, ported from ui/js/Clipboard.js, ClipSelection.js and ClipMarks.js; the backend keeps token, paths and transfer authority, and this module is the pane side with no I/O: every effect is a returned Out. X11 stays out.
pub mod carry;
mod clipboard;
mod entry;
mod marks;
mod paste;
mod payload;
mod selection;
mod wire;
mod words;

pub use clipboard::{Clip, GetWait, PasteWait, ReadOut, Session, SetEntry};
pub use entry::EntryHandle;
pub use marks::{row_path, row_path_into, spent as marks_spent, Marks};
pub use paste::{
    copy_as_resolved, copy_as_text, image_name, plan, stamp_at, CopyAsOut, LinkKind, PendingPaste,
    Stamp,
};
pub use payload::{
    build_flea, build_gnome, build_kde_cut, build_plain, build_urilist, choose, is_op, is_token,
    parse_flea, parse_gnome, parse_kde_cut, parse_urilist, ForeignSel, Offer, FLEA, GNOME, KDE_CUT,
    PLAIN, PLAIN_UTF8, URILIST, UTF8_STRING,
};
pub use selection::{Publish, Queue, TakeOut, View};
pub use wire::{
    clip_clear_line, clip_get_line, clip_set_line, clip_watch_line, parse_clip, parse_paths,
    paste_image_line, paths_request_line, ClipEvent,
};
pub use words::{
    clear_failed, dest_changed, landed, reading, recent_paste, resolving, set_failed, stale,
    CLEAR_FAILED, COPIED, CUT, DEST_CHANGED, READING, RECENT_PASTE, RESOLVING, SET_FAILED, STALE,
};

#[cfg(test)]
mod carry_tests;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod harness;
#[cfg(test)]
mod marks_tests;
#[cfg(test)]
mod paste_tests;
#[cfg(test)]
mod payload_tests;
#[cfg(test)]
mod races_get_tests;
#[cfg(test)]
mod races_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod wire_tests;

/// What the host must do: send a backend line, say a status line, resolve row indices to absolute paths, or hand a paste to the transfer authority (op1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Send(String),
    Say(Said),
    AskPaths(Vec<usize>),
    Ask(PasteAsk),
    Text(String),
}

/// One user-visible line with its severity, already resolved through words; paste_hint marks a landed copy or cut so the status slot (G1.13) appends the paste key, as Status.withPaste does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub text: String,
    pub error: bool,
    pub paste_hint: bool,
}

/// A paste the transfer authority owns from here: transfer, link or image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteAsk {
    Transfer {
        op: String,
        paths: Vec<String>,
        dest: String,
        spends_cut: bool,
    },
    Link {
        kind: LinkKind,
        paths: Vec<String>,
        dest: String,
    },
    Image {
        dest: String,
        name: String,
    },
}
