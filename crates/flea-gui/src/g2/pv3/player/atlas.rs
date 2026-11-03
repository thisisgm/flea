//! What a player's pictures hold in the atlas: the shown one, at most one retired, and nothing once it hides, closes or drops.
use super::MediaPlayer;
use crate::g2::pv3::view::Shown;
use gpui::{App, RenderImage};
use std::sync::Arc;

impl MediaPlayer {
    /// Shows a new picture; the old one waits for the next paint only when a paint uploaded it, else it goes at once.
    pub(super) fn replace_frame(&mut self, frame: Arc<RenderImage>, cx: &mut App) {
        let Some(old) = self.frame.replace(frame) else {
            return;
        };
        let mut painted = self.painted.borrow_mut();
        if painted.uploaded != Some(old.id) {
            return;
        }
        painted.uploaded = None;
        if let Some(stale) = painted.retired.replace(old) {
            release(stale, cx);
        }
    }

    /// Takes every picture out of the atlas, as a hidden Quick Look must; the next paint uploads the shown one again.
    pub fn hide(&mut self, cx: &mut App) {
        release_painted(&self.painted, self.frame.as_ref(), cx);
    }
}

/// Frees the retired picture, and `frame` too when a paint uploaded it.
pub(super) fn release_painted(painted: &Shown, frame: Option<&Arc<RenderImage>>, cx: &mut App) {
    let mut painted = painted.borrow_mut();
    if let Some(retired) = painted.retired.take() {
        release(retired, cx);
    }
    if let (Some(uploaded), Some(frame)) = (painted.uploaded.take(), frame) {
        if frame.id == uploaded {
            release(frame.clone(), cx);
        }
    }
}

/// Frees a picture from every window's atlas once no window is mid-draw.
fn release(image: Arc<RenderImage>, cx: &mut App) {
    cx.defer(move |cx| cx.drop_image(image, None));
}
