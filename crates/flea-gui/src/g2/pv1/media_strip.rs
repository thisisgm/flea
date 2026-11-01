//! Preview.qml's media-strip lifetime: four seconds after contact, held seek and volume drags stay visible.
use super::{doc::View, QuickLook};
use gpui::Context;
use std::time::Duration;

impl QuickLook {
    pub(super) fn reveal_strip(&mut self, cx: &mut Context<Self>) {
        if !self.subject.as_ref().is_some_and(|subject| {
            subject
                .body
                .doc()
                .is_some_and(|doc| matches!(doc.view, View::Media(_)))
        }) {
            return;
        }
        self.strip_shown = true;
        cx.notify();
        self.strip_hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            let _ = this.update(cx, |look, cx| {
                let held = look.subject.as_ref().is_some_and(|subject| {
                    subject.body.doc().is_some_and(|doc| match &doc.view {
                        View::Media(media) => media.caches.strip.borrow().held(),
                        _ => false,
                    })
                });
                if held {
                    look.reveal_strip(cx);
                } else {
                    look.strip_shown = false;
                    look.strip_hide_task = None;
                    cx.notify();
                }
            });
        }));
    }
}
