//! Join seams: the plain data and the three traits this unit needs from other units' surfaces; payload bytes come from the core shapes, Copy As words from ui_logic_context_menus::copyas, and menus read Clipboard::available and watch_failed.
use flea::ui_logic_file_clipboard::{LinkKind, PasteAsk};

// Sample input: "copyAs:copyName" publishes names, "pasteAs:pasteLink" pastes a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    CopyAs(String),
    /// The Places and Locked menus' Copy path alias: the first path only.
    CopyFirst,
    PasteAs(LinkKind),
    Unhandled,
}

impl MenuAction {
    pub fn parse(action: &str) -> Self {
        const LEAVES: [&str; 7] = [
            "copyPath",
            "copyRelative",
            "copyName",
            "copyStem",
            "copydirpath",
            "copyUri",
            "copyQuoted",
        ];
        if let Some(leaf) = action.strip_prefix("copyAs:") {
            if LEAVES.contains(&leaf) {
                return Self::CopyAs(leaf.into());
            }
        } else if LEAVES.contains(&action) {
            return Self::CopyAs(action.into());
        } else if action == "copypath" {
            return Self::CopyFirst;
        }
        if let Some(leaf) = action.strip_prefix("pasteAs:") {
            if let Some(kind) = LinkKind::parse(leaf) {
                return Self::PasteAs(kind);
            }
        }
        Self::Unhandled
    }
}

/// One MIME representation the fork owner publishes, in serve order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// A validated file selection for the fork owner: the backend's token, paths and cut intent, bridged instead of claimed twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publish {
    pub cut: bool,
    pub paths: Vec<String>,
    pub token: String,
    pub payloads: Vec<Payload>,
}

/// A foreign selection read through the fork, with the generation it arrived in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Foreign {
    pub clip: String,
    pub paths: Vec<String>,
    pub token: String,
    pub generation: u64,
}

/// The backend pipe for clipSet, clipGet, clipClear and clipWatch lines (B's clip authority, through G1.02's backend session).
pub trait Wire {
    fn send(&self, line: String) -> bool;
}

/// Paste operations (op1): the transfer, link and image asks land here and op1's collide host decides through them; a cut paste carries its captured clip so op1 clears exactly it.
pub trait Sink {
    fn paste(&mut self, ask: PasteAsk, captured: Captured);
}

/// The cut a transfer captured, for op1's spent rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    pub paths: Vec<String>,
    pub moving: bool,
    pub token: String,
    /// Holds the captured entry's eventual token; drop the capture on completion or cancellation.
    pub entry: flea::ui_logic_file_clipboard::EntryHandle,
}

/// The fork clipboard owner (g2-fork-dg0): one asynchronous file selection, URI/GNOME payloads and cut identity, mirroring the recorded fork API (new_mime, on_clipboard_change, clear_clipboard_if_owned); bounded to sixteen transfers and 64 MiB per representation.
pub trait Owner {
    fn publish(&mut self, item: Publish) -> bool;
    fn publish_text(&mut self, text: String) -> bool;
    fn clear_if_owned(&mut self, token: &str, cut_paths: &[String]) -> bool;
    fn drain_foreign(&mut self) -> Vec<Foreign>;
}

/// Builds the fork payloads for a validated selection in serve order, through the core payload shapes; KDE's cut mark rides only on a cut.
pub fn payloads(cut: bool, paths: &[String], token: &str) -> Vec<Payload> {
    use flea::ui_logic_file_clipboard as format;
    let op = if cut { "cut" } else { "copy" };
    let mut out = vec![
        Payload {
            mime: format::FLEA,
            bytes: format::build_flea(op, token),
        },
        Payload {
            mime: format::GNOME,
            bytes: format::build_gnome(op, paths),
        },
        Payload {
            mime: format::URILIST,
            bytes: format::build_urilist(paths),
        },
        Payload {
            mime: format::PLAIN_UTF8,
            bytes: format::build_plain(paths),
        },
        Payload {
            mime: format::PLAIN,
            bytes: format::build_plain(paths),
        },
        Payload {
            mime: format::UTF8_STRING,
            bytes: format::build_plain(paths),
        },
    ];
    if cut {
        out.push(Payload {
            mime: format::KDE_CUT,
            bytes: format::build_kde_cut(),
        });
    }
    out
}
