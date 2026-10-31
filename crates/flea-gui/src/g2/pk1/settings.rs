//! The chooser restores its view, follows the browser's persisted listing preferences and persists its view and thumbnail choices.
use super::Picker;
use crate::grid::geometry::ThumbSize;
use crate::window::settings::store::{Update, Worker};
use flea::jsondoc::Json;
use gpui::Context;

pub(super) struct Preferences {
    pub(super) hidden: bool,
    pub(super) view_chosen: bool,
    relist: bool,
    writes: Option<std::sync::mpsc::Sender<(u64, Json, Option<Json>)>>,
    revision: u64,
    pending: Option<String>,
    pending_thumb: Option<(u64, ThumbSize)>,
    unsaved_thumb: Option<ThumbSize>,
    pub(super) ctrl_zoom: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            hidden: false,
            view_chosen: false,
            relist: false,
            writes: None,
            revision: 0,
            pending: None,
            pending_thumb: None,
            unsaved_thumb: None,
            ctrl_zoom: true,
        }
    }
}

impl Picker {
    pub(crate) fn apply_settings(&mut self, data: &Json, cx: &mut Context<Self>) {
        self.jump_settings(data, cx);
        self.rail_favourites(data, cx);
        self.chrome.preset = data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default")
            .to_owned();
        let preview = data.get("preview");
        self.preferences.ctrl_zoom = preview
            .and_then(|preview| preview.get("ctrlZoom"))
            .and_then(Json::as_bool)
            != Some(false);
        let size = self
            .preferences
            .pending_thumb
            .map(|(_, size)| size)
            .or(self.preferences.unsaved_thumb)
            .unwrap_or_else(|| {
                ThumbSize::named(
                    preview
                        .and_then(|preview| preview.get("thumbSize"))
                        .and_then(Json::as_str)
                        .unwrap_or("medium"),
                )
            });
        let hidden = self.req.hidden.unwrap_or(self.preferences.hidden);
        self.preferences.hidden = data.get("hidden").and_then(Json::as_bool) == Some(true);
        if !self.preferences.view_chosen && self.preferences.pending.is_none() {
            self.set_view(
                data.get("pickerView")
                    .and_then(Json::as_str)
                    .unwrap_or("list"),
                cx,
            );
        }
        let changed = self.browse.listing.update(cx, |listing, cx| {
            listing.set_thumbnail(size);
            listing.apply_settings(data, cx)
        });
        if changed || hidden != self.req.hidden.unwrap_or(self.preferences.hidden) {
            self.preferences.relist = true;
        }
        self.relist_settings(cx);
        self.refill_grid(cx);
    }

    // A submission in flight keeps its listing, so a settings re-list waits until it ends unanswered.
    pub(super) fn relist_settings(&mut self, cx: &mut Context<Self>) {
        if self.preferences.relist && !self.accept_state().submitting() && !self.answered() {
            self.preferences.relist = false;
            let path = self.listing.location.clone();
            self.open_folder(&path, cx);
        }
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
                if let Update::Failed { through, .. } = &update {
                    let through = *through;
                    if this
                        .update(cx, |picker, cx| {
                            let preferences = &mut picker.preferences;
                            // A size the worker failed to save stays drawn for this chooser; the next wheel step writes again.
                            if let Some((revision, size)) = preferences.pending_thumb {
                                if through >= revision {
                                    preferences.pending_thumb = None;
                                    preferences.unsaved_thumb = Some(size);
                                }
                            }
                            picker.say_key("settings.error.not_saved", true, cx);
                        })
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
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
                        if picker
                            .preferences
                            .pending_thumb
                            .is_some_and(|(revision, _)| through >= revision)
                        {
                            picker.preferences.pending_thumb = None;
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

    // A title tap's sort, written as the browser's folder and default sort.
    pub(super) fn persist_sort(&mut self, patch: Json, cx: &mut Context<Self>) {
        let Some(writes) = &self.preferences.writes else {
            return;
        };
        self.preferences.revision += 1;
        if writes
            .send((self.preferences.revision, patch, None))
            .is_err()
        {
            self.say_key("settings.error.not_saved", true, cx);
        }
    }

    pub(super) fn persist_thumbnail(&mut self, size: ThumbSize, cx: &mut Context<Self>) {
        let Some(writes) = &self.preferences.writes else {
            return;
        };
        self.preferences.revision += 1;
        let patch = Json::Obj(vec![(
            "preview".into(),
            Json::Obj(vec![("thumbSize".into(), Json::Str(size.name().into()))]),
        )]);
        if writes
            .send((self.preferences.revision, patch, None))
            .is_ok()
        {
            self.preferences.pending_thumb = Some((self.preferences.revision, size));
            self.preferences.unsaved_thumb = None;
        } else {
            self.say_key("settings.error.not_saved", true, cx);
        }
    }
}
