//! The key verbs this unit answers: copy, cut, paste, forced move and the three link pastes in every preset spelling; mn1 and G1.05 read `available` and route picks back through `menu_action`.
use super::seams::{Owner, Sink, Wire};
use super::{Clipboard, Landing, Take};
use flea::i18n::I18n;
use flea::ui_logic_dg1::{self as core, LinkKind};

// Sample input: "pasteArm" pastes, "cutArm" cuts, "pasteHardLink" links hard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Copy,
    Cut,
    Paste,
    MovePaste,
    Link(LinkKind),
}

impl Verb {
    pub fn parse(action: &str) -> Option<Self> {
        Some(match action {
            "copy" => Self::Copy,
            "cut" | "cutArm" => Self::Cut,
            "paste" | "pasteArm" => Self::Paste,
            "movePaste" => Self::MovePaste,
            "pasteLink" => Self::Link(LinkKind::Relative),
            "pasteAbsoluteLink" => Self::Link(LinkKind::Absolute),
            "pasteHardLink" => Self::Link(LinkKind::Hard),
            _ => return None,
        })
    }
}

impl Clipboard {
    /// One key verb over rows, or over menu snapshot paths when the menu took them; None when another unit owns the verb.
    #[allow(clippy::too_many_arguments)]
    pub fn act(
        &mut self,
        i18n: &I18n,
        backend: u64,
        verb: Verb,
        paths: Option<Vec<String>>,
        rows: Vec<usize>,
        take: Take<'_>,
        landing: Landing<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Option<Vec<core::Said>> {
        // Recent is a history, not a folder: every paste verb refuses before any read (ui/js/Focus.js:217,235,237; ui/js/RecentMode.js:14).
        if landing.recent && !matches!(verb, Verb::Copy | Verb::Cut) {
            return Some(vec![core::recent_paste(i18n)]);
        }
        match verb {
            Verb::Copy => {
                Some(self.take(i18n, backend, false, paths, rows, take, wire, sink, owner))
            }
            Verb::Cut => Some(self.take(i18n, backend, true, paths, rows, take, wire, sink, owner)),
            Verb::Paste => Some(self.paste(
                i18n,
                backend,
                core::PendingPaste::transfer(landing.path),
                landing,
                wire,
                sink,
                owner,
            )),
            Verb::MovePaste => Some(self.paste(
                i18n,
                backend,
                core::PendingPaste::forced_move(landing.path),
                landing,
                wire,
                sink,
                owner,
            )),
            Verb::Link(kind) => Some(self.paste(
                i18n,
                backend,
                core::PendingPaste::link(landing.path, kind),
                landing,
                wire,
                sink,
                owner,
            )),
        }
    }
}
