//! Retired frames and cancellation live with the retained selection owner.
use super::*;
use gpui::{ImageAssetLoader, Window};

impl SelectionPreview {
    pub(crate) fn dispose(
        &mut self,
        mut frame: Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &mut frame.mounted {
            Mounted::Pdf(pdf) => pdf.update(cx, |pdf, cx| pdf.close(cx)),
            Mounted::Model(model) => model.update(cx, |model, cx| model.hide(cx)),
            Mounted::Media(player) => {
                if let Some(player) = player.take() {
                    player.update(cx, |player, cx| player.close(cx));
                }
            }
            Mounted::Basic => {}
        }
        if let Some(source) = frame.source.take() {
            cx.remove_asset::<ImageAssetLoader>(&source);
        }
        if let Some(image) = frame.image.take() {
            window.defer(cx, move |_, cx| cx.drop_image(image, None));
        }
        self.retired.push(frame);
        let entity = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            let _ = entity.update(cx, |view, _| view.retired.clear());
        });
    }
    pub(super) fn cancel_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(frame) = self.pending.take() {
            self.outbox.borrow_mut().push(format!(
                r#"{{"c":"thumbcancel","rows":[{}]}}"#,
                frame.subject.index
            ));
            self.outbox.borrow_mut().push(format!(
                r#"{{"c":"code","slot":"column","token":{},"text":"","count":1}}"#,
                token()
            ));
            self.dispose(frame, window, cx);
        }
    }
    pub(crate) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending(window, cx);
        if let Some(frame) = self.drawn.take() {
            self.dispose(frame, window, cx);
        }
        self.visible = false;
        self.timer = None;
        self.port.cancel();
        self.port.0.borrow_mut().ready = false;
        *self.rect.borrow_mut() = None;
        *self.frame_rect.borrow_mut() = None;
        *self.play_rect.borrow_mut() = None;
    }
    pub(super) fn arm(&mut self, due: Option<u64>, cx: &mut Context<Self>) {
        let Some(due) = due else {
            self.timer = None;
            return;
        };
        if self.timer.as_ref().is_some_and(|(at, _)| *at == due) {
            return;
        }
        let wait = due.saturating_sub(self.now(cx));
        self.timer = Some((
            due,
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(wait))
                    .await;
                let _ = this.update(cx, |view, cx| {
                    view.timer = None;
                    cx.notify();
                });
            }),
        ));
    }
}
