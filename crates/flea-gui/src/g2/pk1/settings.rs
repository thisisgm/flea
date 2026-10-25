//! The chooser follows the same persisted code-icon preference as the main window.
use super::Picker;
use crate::window::settings::store::{Update, Worker};
use flea::jsondoc::Json;
use gpui::Context;

impl Picker {
    pub(crate) fn apply_settings(&mut self, data: &Json, cx: &mut Context<Self>) {
        self.browse
            .listing
            .update(cx, |listing, cx| listing.apply_settings(data, cx));
    }

    pub(super) fn follow_settings(&mut self, cx: &mut Context<Self>) {
        if let Ok(worker) = Worker::start() {
            self.watch_settings(worker, cx);
        }
    }

    pub(super) fn watch_settings(&mut self, worker: Worker, cx: &mut Context<Self>) {
        let updates = worker.updates.clone();
        let reader = cx.spawn(async move |this, cx| {
            let _worker = worker;
            while let Ok(update) = updates.recv().await {
                let Update::Loaded { document, .. } = update else {
                    continue;
                };
                if this
                    .update(cx, |picker, cx| picker.apply_settings(&document, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.keep(reader);
    }
}
