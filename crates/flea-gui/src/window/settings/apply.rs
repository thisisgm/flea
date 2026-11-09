use super::{store, SessionStore};
use crate::{
    navigation::folder_state::Folders,
    state::Shell,
    visual::view_name,
    window::startup::{Settings, View},
};
use flea::{
    jsondoc::Json,
    ui_logic_settings_pages::{pages, Text},
};
use gpui::{Context, Window};

#[cfg(test)]
#[path = "m9_g2rows_02_writer_tests.rs"]
mod m9_g2rows_02_writer;

impl Shell {
    pub(crate) fn sync_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_settings_claim(window, cx);
        self.sync_settings_plugins(window, cx);
        if self.settings.opened {
            let focus = self.settings.panel.as_ref().and_then(|view| {
                let view = view.read(cx);
                view.panel.query.as_ref().map(|_| view.focus.clone())
            });
            if let Some(focus) = focus {
                window.focus(&focus, cx);
            }
        }
        let patches = std::mem::take(&mut *self.settings.patches.borrow_mut());
        for patch in patches {
            self.change_settings(patch, window, cx);
        }
    }

    pub(crate) fn change_settings(
        &mut self,
        patch: Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.write_settings(patch, None, window, cx);
    }

    pub(super) fn write_settings(
        &mut self,
        patch: Json,
        operation: Option<Json>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(data) = flea::uistate::patched(&self.launch.settings.data, &patch) else {
            return;
        };
        self.settings.revision += 1;
        let revision = self.settings.revision;
        let mut saving = Json::Obj(Vec::new());
        for (_, failed) in &self.settings.failed {
            crate::g2::settings_pages::seams::merge(&mut saving, failed);
        }
        for (_, pending) in &self.settings.pending {
            store::overlay_retry(&mut saving, pending);
        }
        crate::g2::settings_pages::seams::merge(&mut saving, &patch);
        if let Some(worker) = &self.settings.worker {
            if operation.is_none() {
                let through = self
                    .settings
                    .failed
                    .iter()
                    .map(|(id, _)| *id)
                    .max()
                    .unwrap_or(0);
                self.settings.retried.push((revision, through));
            }
            self.settings.pending.push((
                revision,
                if operation.is_some() {
                    Json::Obj(Vec::new())
                } else {
                    saving.clone()
                },
            ));
            let favourite = operation.is_some();
            self.settings
                .favourite_writes
                .extend(favourite.then_some(revision));
            if worker.writes.send((revision, saving, operation)).is_err() {
                self.finish_network_favourite(revision, Err(""), cx);
                self.settings_save_error(cx);
            }
            if favourite {
                return;
            }
        }
        let view = patch.get("view").and_then(Json::as_str).map(str::to_owned);
        self.apply_settings(data, window, cx);
        if let Some(view) = view {
            self.set_view(view_kind(&view), window, cx);
        }
    }

    pub(crate) fn receive_settings(
        &mut self,
        update: store::Update,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match update {
            store::Update::FavouriteCommitted { through } => {
                self.finish_committed_favourite(through, cx)
            }
            store::Update::FavouriteFailed { through, detail } => {
                self.finish_network_favourite(through, Err(&detail), cx);
                self.receive_settings(
                    store::Update::Failed {
                        through,
                        keep: false,
                    },
                    window,
                    cx,
                );
            }
            store::Update::Unreadable => {
                self.settings_error("settings.error.favourites_unread", cx)
            }
            store::Update::Corner(corner) => self.apply_corner(corner, window, cx),
            store::Update::Failed { through, keep } => {
                self.finish_network_favourite(through, Err(""), cx);
                if self
                    .settings
                    .favourite_focus
                    .is_some_and(|(id, _)| id <= through)
                {
                    self.settings.favourite_focus = None;
                }
                if keep {
                    self.settings.failed.extend(
                        self.settings
                            .pending
                            .iter()
                            .filter(|(id, _)| *id <= through)
                            .cloned(),
                    );
                }
                self.settings.answered(through);
                self.settings_save_error(cx);
            }
            store::Update::Loaded {
                mut document,
                through,
                saved,
            } => {
                if saved {
                    let retried = self
                        .settings
                        .retried
                        .iter()
                        .find(|(id, _)| *id == through)
                        .map_or(0, |(_, retried)| *retried);
                    self.settings.failed.retain(|(id, _)| *id > retried);
                    if let Some((_, written)) =
                        self.settings.pending.iter().find(|(id, _)| *id == through)
                    {
                        for (id, patch) in &mut self.settings.failed {
                            if *id < through {
                                store::unwritten(patch, written);
                            }
                        }
                        self.settings.failed.retain(|(_, patch)| {
                            patch.as_object().is_some_and(|fields| !fields.is_empty())
                        });
                    }
                }
                self.settings.answered(through);
                // Folder choices the folder writer has not finished stay as this window chose them.
                let owed = self
                    .panes
                    .iter()
                    .flatten()
                    .next()
                    .map_or_else(Vec::new, |host| host.nav.writer.borrow_mut().owed());
                let saving = self.settings.failed.iter().chain(&self.settings.pending);
                let patches = owed.iter().chain(saving.map(|(_, patch)| patch));
                store::keep_own(&mut document, &self.launch.settings.data, patches);
                self.apply_settings(document, window, cx);
                self.finish_network_favourite(through, Ok(""), cx);
                if self
                    .settings
                    .favourite_focus
                    .is_some_and(|(id, _)| id <= through)
                {
                    let (_, index) = self.settings.favourite_focus.take().unwrap();
                    self.focus_settings_favourite(index, cx);
                }
            }
        }
    }

    fn settings_save_error(&mut self, cx: &mut Context<Self>) {
        self.settings_error("settings.error.not_saved", cx);
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.panel.save_error = Some(Text::Key("settings.error.not_saved"));
                view.panel.rebuild();
                cx.notify();
            });
        }
        if let Some(host) = &mut self.panes[self.focus_side] {
            host.status = self.locale.catalog.tr("settings.error.not_saved").into();
        }
        cx.notify();
    }

    pub(crate) fn apply_settings(
        &mut self,
        data: Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old = &self.launch.settings.data;
        let preset_changed = old.get("keys") != data.get("keys");
        let plugins_changed = ["plugins", "shelf"]
            .iter()
            .any(|key| old.get(key) != data.get(key));
        let relist = [
            "hidden",
            "foldersFirst",
            "hiddenLast",
            "groupByKind",
            "rememberSort",
            "sort",
            "folderSorts",
        ]
        .iter()
        .any(|key| old.get(key) != data.get(key));
        let hidden_changed = old.get("hidden") != data.get("hidden");
        let newest = self.launch.settings.newest.clone();
        let mut settings = Settings::from_json(data.clone());
        settings.newest = newest.clone();
        self.launch.settings = settings;
        if let Some(host) = &self.quick_look {
            let languages = data.get("langMarks").and_then(Json::as_bool) != Some(false);
            host.view
                .update(cx, |view, cx| view.set_languages(languages, cx));
        }
        self.rail_state(&data, cx);
        if let Some(host) = self.panes.iter().flatten().next() {
            *host.nav.folders.borrow_mut() = Folders::from_settings(&data, newest);
        }
        self.font_key = None;
        self.sync_font_settings(cx);
        let typeface = self.settings_typeface(cx);
        // Settings.js: a new Columns view limit re-lays an open Columns pane at once, as a resize does.
        let limit = Some(self.launch.settings.columns as f64);
        for host in self.panes.iter_mut().flatten() {
            configure_pane(host, &data, typeface.clone());
            if let Some(columns) = host.columns.as_mut() {
                columns.set_limit(limit);
            }
        }
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.typeface = typeface.clone();
                view.panel.replace_store(Box::new(SessionStore {
                    document: data.clone(),
                    patches: self.settings.patches.clone(),
                }));
                cx.notify();
            });
        }
        if let Some(view) = &self.settings.sheet {
            view.update(cx, |view, cx| {
                view.typeface = typeface;
                view.sheet
                    .set_preset(pages::word(&data, &["keys"], "default"));
                cx.notify();
            });
        }
        if plugins_changed {
            self.plugins_settings_changed(cx);
        }
        if plugins_changed || preset_changed {
            self.user_keys_changed(window, cx);
        }
        self.sync_trash_sheet(cx);
        if relist {
            self.settings.relist = [true, true];
            if hidden_changed {
                self.settings.relist_fresh = [true, true];
            }
        }
        self.settle_settings(window, cx);
        cx.notify();
    }

    pub(crate) fn restyle_settings(&self, face: &crate::list::Typeface, cx: &mut Context<Self>) {
        let base = self.settings_base(cx);
        if let Some(view) = &self.settings.panel {
            view.update(cx, |view, cx| {
                view.typeface = face.clone();
                view.panel.facts.base_size = base;
                view.panel.rebuild();
                cx.notify();
            });
        }
        if let Some(view) = &self.settings.sheet {
            view.update(cx, |view, cx| {
                view.typeface = face.clone();
                cx.notify();
            });
        }
    }

    pub(crate) fn configure_settings_pane(
        &self,
        host: &mut crate::window::pane::PaneHost,
        cx: &gpui::App,
    ) {
        configure_pane(host, &self.launch.settings.data, self.settings_typeface(cx));
    }

    pub(crate) fn settings_folder_arrived(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.view == View::Dual {
            return;
        }
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        if self.settings.listed_paths[self.focus_side] == host.path {
            return;
        }
        self.settings.listed_paths[self.focus_side] = host.path.clone();
        if !pages::is_true(&self.launch.settings.data, &["rememberViewPerFolder"]) {
            return;
        }
        let fallback = view_name(view_kind(pages::word(
            &self.launch.settings.data,
            &["view"],
            "list",
        )));
        let name = host.nav.folders.borrow().view_for(&host.path, fallback);
        self.view = view_kind(name);
        cx.notify();
    }
}

fn configure_pane(
    host: &mut crate::window::pane::PaneHost,
    data: &Json,
    typeface: crate::list::Typeface,
) {
    let mut list = host.list.borrow_mut();
    list.view
        .set_lang_marks(data.get("langMarks").and_then(Json::as_bool) != Some(false));
    list.density = crate::list::Density::parse(pages::word(data, &["density"], "compact"));
    list.set_typeface(typeface);
    host.pane.set_row_height(list.file_metrics().row);
}

fn view_kind(name: &str) -> View {
    match name {
        "columns" => View::Columns,
        "grid" => View::Grid,
        "dual" => View::Dual,
        _ => View::List,
    }
}
