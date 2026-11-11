//! LoadingState.qml for every card kind: nothing for HOLD_MS, then the kind's mark over "reading" until the first answer lands.
use super::{
    bar::mark,
    card::text,
    look::{self, Look},
    Body, QuickLook,
};
use gpui::{div, prelude::*, px, AnyElement, App, Context};
use std::time::Duration;

impl QuickLook {
    /// A new subject starts its hold; the mark may draw once HOLD_MS passes with the answer still out.
    pub(super) fn arm_waiting(&mut self, cx: &mut Context<Self>) {
        self.waiting_due = false;
        self.waiting_hold = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(crate::pane::swap::HOLD_MS))
                .await;
            let _ = this.update(cx, |look, cx| {
                look.waiting_due = true;
                look.waiting_hold = None;
                if look.waiting(cx) {
                    cx.notify();
                }
            });
        }));
    }

    /// A step inside the subject that takes its answer back, a sheet turn or a word, holds the mark as a new subject does.
    pub(super) fn rearm_waiting(&mut self, waiting: bool, cx: &mut Context<Self>) {
        if !waiting && self.waiting(cx) {
            self.arm_waiting(cx);
        }
    }

    /// The subject has nothing of its answer yet: no lines, no picture size, no first screen, no Compare reply.
    pub fn waiting(&self, cx: &App) -> bool {
        let Some(subject) = self.subject.as_ref() else {
            return false;
        };
        match &subject.body {
            Body::Text { lines, count } => lines.is_empty() && count.is_empty(),
            // A sizeless meta reply mounts CannotShow, so an unsized picture still waits on meta or its header.
            Body::Image { w, .. } | Body::Link { w, .. } | Body::Raw { w, .. } => *w <= 0.,
            Body::Doc(doc) => !doc.ready(cx),
            Body::Refused { sentence, .. } => sentence.is_empty(),
            Body::CannotShow { .. } | Body::Diff { .. } => false,
        }
    }

    /// The waiting mark over the card, once its hold has run out with the answer still out.
    pub(super) fn waiting_mark(&self, look: &Look, cx: &App) -> Option<AnyElement> {
        if !self.waiting_due || !self.waiting(cx) {
            return None;
        }
        let subject = self.subject.as_ref()?;
        Some(
            div()
                .debug_selector(|| "pv1-waiting".into())
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(look::GAP))
                .child(mark(
                    subject.mark_with_languages(self.languages),
                    look::MARK,
                    look.muted,
                ))
                .child(text(
                    &self.i18n.tr("pane.reading"),
                    look::CAPTION,
                    look.muted,
                ))
                .into_any_element(),
        )
    }
}
