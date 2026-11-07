//! Close input immediately and retain only the fading card until its bounded lifetime ends.
use super::*;

impl QuickLook {
    /// close() in Preview.qml: every per-subject flag goes, so the next open starts clean.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        let Some(subject) = self.subject.take() else {
            return;
        };
        let now = self.observation_time(cx);
        self.observations.borrow_mut().closed(now);
        self.closing = Some(subject);
        self.strip_hide_task = None;
        self.strip_shown = false;
        self.ground.reset();
        self.revealed = false;
        // A held card is what shows, so it fades out in the new subject's place.
        self.kept_shown = false;
        self.focused = None;
        self.hovered = None;
        self.pressed = None;
        self.chrome_armed = false;
        let reduced = cx.reduce_motion();
        self.motion
            .sample(false, cx.background_executor().now(), reduced);
        if reduced {
            self.finish_close(cx);
        } else {
            self.close_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(flea::ui_logic_fb1::CLOSE_MS))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    if view.subject.is_none() {
                        view.finish_close(cx);
                        cx.notify();
                    }
                });
            }));
        }
        cx.emit(Event::Closed);
        cx.notify();
    }

    pub(super) fn finish_close(&mut self, cx: &mut Context<Self>) {
        self.close_task = None;
        self.doc_watch = None;
        if let Some(doc) = self
            .closing
            .take()
            .and_then(|subject| subject.body.doc().cloned())
            .filter(|doc| self.kept != Some(doc.id()))
        {
            doc.close(cx);
        }
        self.ground.reset();
        self.release_held(cx);
        self.retire_svg(cx);
        self.zoom = 1.;
        self.revealed = false;
        self.expanded = false;
        self.strip_shown = false;
        self.waiting_due = false;
        self.waiting_hold = None;
        self.focused = None;
        self.hovered = None;
        self.pressed = None;
        self.chrome_armed = false;
        self.scroll = Scroll::default();
    }
}
