//! The chooser follows the same persisted code-icon preference as the main window.
use super::Picker;
use crate::window::settings::store::{Update, Worker};
use flea::jsondoc::Json;
use gpui::Context;

#[derive(Default)]
pub(super) struct Preferences {
    writes: Option<std::sync::mpsc::Sender<(u64, Json, Option<Json>)>>,
    revision: u64,
    pending: Option<String>,
}

impl Picker {
    pub(crate) fn apply_settings(&mut self, data: &Json, cx: &mut Context<Self>) {
        self.jump_settings(data, cx);
        self.chrome.preset = data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default")
            .to_owned();
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
        self.preferences.writes = Some(worker.writes.clone());
        let updates = worker.updates.clone();
        let reader = cx.spawn(async move |this, cx| {
            let _worker = worker;
            while let Ok(update) = updates.recv().await {
                let Update::Loaded {
                    document, through, ..
                } = update
                else {
                    continue;
                };
                if this
                    .update(cx, |picker, cx| {
                        if through >= picker.preferences.revision {
                            picker.preferences.pending = None;
                        }
                        picker.apply_settings(&document, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.keep(reader);
    }
    pub fn persist_picker_view(&mut self, mode: &str, cx: &mut Context<Self>) {
        if !matches!(mode, "list" | "grid") {
            return;
        }
        let Some(writes) = &self.preferences.writes else {
            return;
        };
        self.preferences.revision += 1;
        let patch = Json::Obj(vec![("pickerView".into(), Json::Str(mode.into()))]);
        if writes
            .send((self.preferences.revision, patch, None))
            .is_ok()
        {
            self.preferences.pending = Some(mode.into());
        } else {
            self.say_key("settings.error.not_saved", true, cx);
        }
    }
}
