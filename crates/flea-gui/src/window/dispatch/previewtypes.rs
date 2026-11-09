//! The previewtypes family: the keys the preview column runs on the kind it shows (SelectionPreview.qml Keys.onPressed).
use super::Order;
use crate::{
    g2::media_preview::{sound::MediaSound, strip::StripInput},
    state::Shell,
    window::selection_preview::{Mounted, SelectionPreview},
};
use flea::{
    keys::Context as Keys, ui_logic_doc_preview::viewer, ui_logic_media_preview::player::MediaKey,
};
use gpui::{Context, Entity, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Action(event) = order else {
        return false;
    };
    let action = event.action;
    let page = viewer::page_direction(action);
    if page.is_none() && !matches!(action, "playPause" | "mute" | "seekBack" | "seekForward") {
        return false;
    }
    let Some(preview) = shown(shell, event.seat.listing, cx) else {
        return false;
    };
    let focused = preview.read(cx).focus.is_focused(window);
    let ran = if let Some(direction) = page {
        if matches!(action, "pageDown" | "pageUp") && !focused {
            return false;
        }
        preview.update(cx, |view, cx| view.page(direction, cx))
    } else if preview.read(cx).focus.is_focused(window) {
        preview.update(cx, |view, cx| view.media_key(action, window, cx))
    } else {
        false
    };
    if ran {
        cx.notify();
    }
    ran
}

/// The focused pane's column while it shows the listing the key was pressed on.
fn shown(shell: &Shell, listing: u64, cx: &gpui::App) -> Option<Entity<SelectionPreview>> {
    if shell.input_blocked(cx) {
        return None;
    }
    let host = shell.panes[shell.focus_side].as_ref().filter(|host| {
        !host.pane.busy() && !host.stopped && listing == host.pane.held.listing as u64
    })?;
    host.preview.clone().filter(|p| p.read(cx).visible)
}

impl SelectionPreview {
    /// The key context a focused column takes from its row: pdf, media, or preview.
    pub(crate) fn key_context(&self) -> Keys {
        match self.drawn.as_ref().map(|frame| &frame.mounted) {
            Some(Mounted::Pdf(_)) => Keys::Pdf,
            Some(Mounted::Media(_)) => Keys::Media,
            _ => Keys::Preview,
        }
    }

    /// Page the mounted document; a PDF falls through to the preview family's turn.
    fn page(&mut self, direction: i64, cx: &mut Context<Self>) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let Some(frame) = self.drawn.as_ref().filter(|f| !f.manual) else {
            return false;
        };
        let Mounted::Doc(doc) = &frame.mounted else {
            return false;
        };
        let height = self
            .frame_rect
            .borrow()
            .map_or(0., |b| f32::from(b.size.height));
        doc.page(direction, height, cx);
        cx.notify();
        true
    }

    /// p, m and the seek arrows in the media context; p builds the player the way the strip's play mark does.
    fn media_key(&mut self, action: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(frame) = self.drawn.as_ref().filter(|f| !f.manual) else {
            return false;
        };
        let Mounted::Media(player) = &frame.mounted else {
            return false;
        };
        let player = player.clone();
        match (
            flea::ui_logic_media_preview::player::media_key(action, true),
            player,
        ) {
            (Some(MediaKey::TogglePlay), _) => self.play(StripInput::Toggle, window, cx),
            (Some(MediaKey::Mute), None) => {
                MediaSound::update(cx, |sound| {
                    sound.toggle();
                });
            }
            (Some(key), Some(player)) => player.update(cx, |player, cx| player.key(key, cx)),
            (Some(MediaKey::Seek(_)), None) | (None, _) => return false,
        }
        cx.notify();
        true
    }
}
