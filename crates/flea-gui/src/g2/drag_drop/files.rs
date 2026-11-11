//! The window's file client on the process drag router: which offers it claims, which drops it hands to the Shell, and when its window's lift stops being its own.
use super::router::{Client, Native, Offered};
use flea::ui_logic_drag_drop::{self as core, PLAIN, ROWS_MIME, URILIST};
use gpui::{NativeDragAction, NativeDragActions};
use std::cell::RefCell;
use std::rc::Rc;

/// A native drop this window's client accepted, owned so it can wait for the disk facts.
#[derive(Clone, Debug)]
pub struct Arrived {
    pub position: (f32, f32),
    pub marker: Option<String>,
    pub urls: Vec<String>,
    pub plain: Option<String>,
    /// The drop shelf's token and intent, which the drop redeems rather than reading the URIs.
    pub shelf: Option<String>,
    pub action: Option<NativeDragAction>,
    pub ctrl: bool,
    pub shift: bool,
}

/// A window's current lift as its file client knows it: the marker's tag and the lift serial.
pub(super) type Held = Rc<RefCell<Option<(String, u64)>>>;

/// What another window's drag offered when it entered this window, so a folder row under it can say what a release does.
pub(super) type Hover = Rc<RefCell<Option<Arrived>>>;

// The window's file client: it claims drags carrying files, Flea's marker or plain text over this window only; text without a local path is refused with its sentence.
pub(super) struct Files {
    pub(super) window: u64,
    pub(super) send: async_channel::Sender<Arrived>,
    pub(super) held: Held,
    pub(super) hover: Hover,
    /// Told when this window's lift ends in another window or app, so its drag line leaves the status bar.
    pub(super) ended: Option<async_channel::Sender<()>>,
}

// A tab lift is the tab strip's alone, whatever else it offers.
fn carries_files(types: &[String]) -> bool {
    !types
        .iter()
        .any(|kind| kind == crate::tabs::transfer::TAB_MIME)
        && types.iter().any(|kind| {
            kind == URILIST
                || kind == ROWS_MIME
                || kind == PLAIN
                || kind == core::PLAIN_UTF8
                || kind == core::SHELF_MIME
        })
}

impl Client for Files {
    fn offer(&self, offered: &Offered<'_>) -> Option<NativeDragActions> {
        // Copy preferred, as DropInto accepts CopyAction; move too, so a move-only (Shift) source still negotiates, and the transfer stays this window's.
        (offered.window == self.window && carries_files(offered.types)).then_some(
            NativeDragActions {
                copy: true,
                move_: true,
                preferred: NativeDragAction::Copy,
            },
        )
    }

    fn land(&self, native: &Native<'_>) -> Option<bool> {
        if native.window != self.window || !carries_files(native.types) {
            return None;
        }
        let text = |mime: &str| {
            native
                .get(mime)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        };
        let arrived = || Arrived {
            position: native.position,
            marker: text(ROWS_MIME),
            urls: native
                .get(URILIST)
                .map_or(Vec::new(), |bytes| core::uri_lines(&bytes)),
            plain: text(PLAIN).or_else(|| text(core::PLAIN_UTF8)),
            shelf: text(core::SHELF_MIME),
            action: native.action,
            ctrl: native.ctrl,
            shift: native.shift,
        };
        if !native.dropped {
            *self.hover.borrow_mut() = Some(arrived());
            return Some(true);
        }
        *self.hover.borrow_mut() = None;
        // This window's current lift came back in: GPUI restores it and its pane on_drop lands it, so the native copy is not landed twice; any other lift's marker lands natively.
        let current = self.held.borrow().as_ref().map(|(tag, _)| tag.clone());
        let own = text(ROWS_MIME)
            .and_then(|marker| flea::ui_logic_drag_drop::Marker::parse(&marker))
            .is_some_and(|marker| current.as_deref() == Some(marker.instance.as_str()));
        if own && native.get(URILIST).is_some() {
            return Some(true);
        }
        Some(self.send.try_send(arrived()).is_ok())
    }

    // The lift's native session ended elsewhere, so this window no longer holds it.
    fn source(&self, event: &gpui::NativeDragEvent) {
        let ended = matches!(
            event.phase,
            gpui::NativeDragPhase::Finished | gpui::NativeDragPhase::Cancelled
        );
        let mut held = self.held.borrow_mut();
        if ended
            && held
                .as_ref()
                .is_some_and(|(_, serial)| *serial == event.source_id)
        {
            *held = None;
            if let Some(ended) = &self.ended {
                let _ = ended.try_send(());
            }
        }
    }
}
