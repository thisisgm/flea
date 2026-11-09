//! Production Shell mount, backend fanout and view visibility.
use super::*;
use crate::{backend::Event, state::Shell, window::startup::View};
use gpui::{AppContext, Window};

pub(crate) fn settings(data: &Json) -> crate::columns::preview_host::PreviewSettings {
    let preview = data.get("preview").unwrap_or(&Json::Null);
    crate::columns::preview_host::PreviewSettings {
        column: preview.get("column").and_then(Json::as_bool) != Some(false),
        automatic: preview.get("loadOn").and_then(Json::as_str) != Some("manual"),
        manual_hold: false,
    }
}
pub(crate) fn share(data: &Json) -> f32 {
    data.get("preview")
        .and_then(|v| v.get("width"))
        .and_then(Json::as_f64)
        .unwrap_or(0.) as f32
}

/// preview.tracks: each Columns track's weight left of the third, empty for equal tracks.
pub(crate) fn tracks(data: &Json) -> Vec<f32> {
    data.get("preview")
        .and_then(|v| v.get("tracks"))
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Json::as_f64)
                .map(|w| w as f32)
                .collect()
        })
        .unwrap_or_default()
}

impl Shell {
    /// The retained renderer shared with the pane and future expanded-preview host.
    pub fn selection_preview(&self, side: usize) -> Option<Entity<SelectionPreview>> {
        self.panes.get(side)?.as_ref()?.preview.clone()
    }

    pub(crate) fn sync_selection_previews(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let geometry = self.geometry(window);
        let setting = settings(&self.launch.settings.data);
        let preview = self
            .launch
            .settings
            .data
            .get("preview")
            .unwrap_or(&Json::Null);
        for side in 0..2 {
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let visible = setting.column
                && side == 0
                && self.view == View::Columns
                && self.slots.covers[side].is_none()
                && !host.stopped
                && host.backend.is_some();
            if host.preview.is_none() && visible {
                let entity = cx.new(|cx| {
                    SelectionPreview::new(
                        host.preview_port.clone(),
                        self.locale.catalog.clone(),
                        host.list.borrow().typeface.clone(),
                        cx,
                    )
                });
                entity.update(cx, |view, cx| view.observe_frames(window, cx));
                host.preview_watch = Some(cx.observe(&entity, |_, _, cx| cx.notify()));
                host.preview = Some(entity);
            }
            let Some(entity) = host.preview.clone() else {
                continue;
            };
            let subject = Subject::for_pane(host);
            let share = share(&self.launch.settings.data);
            let width = crate::columns::geometry::Layout::weighted(
                geometry.panes[side].width,
                Some(self.launch.settings.columns as f64),
                share.into(),
                None,
                &tracks(&self.launch.settings.data),
            )
            .third_width;
            // The search strip over Columns, as columns_search_strip reads it, from the host already borrowed.
            let search = if self.view == View::Columns && host.search.owner.active() {
                crate::list::Metrics::for_typeface(&host.list.borrow().typeface).header
            } else {
                0.
            };
            let strip = self.slots.strip_height_for(side) + search;
            let height = geometry.content_height_for(side, self.view, strip);
            let folder_wait = self.view == View::Columns
                && entity.read(cx).drawn.is_some()
                && host.columns.as_ref().is_some_and(|columns| {
                    columns.area.host.preview_visible(columns.area.settings)
                });
            let busy = host.pane.busy();
            let mode = self.view;
            let path = host.pane.path().to_owned();
            entity.update(cx, |view, cx| {
                if !visible {
                    if view.visible {
                        view.hide(window, cx);
                    }
                    return;
                }
                if view.mode != mode && view.visible {
                    view.hide(window, cx);
                }
                view.visible = true;
                view.mode = mode;
                view.width = width;
                view.height = height;
                view.face = host.list.borrow().typeface.clone();
                view.catalog = self.locale.catalog.clone();
                view.corner = self.corner;
                view.automatic = setting.automatic;
                view.full_facts = self
                    .launch
                    .settings
                    .data
                    .get("previewFacts")
                    .and_then(Json::as_str)
                    == Some("full");
                if busy {
                    view.follow(None, false, false, window, cx);
                    return;
                }
                if subject.is_none() && !folder_wait {
                    view.hide(window, cx);
                    return;
                }
                if view
                    .class
                    .as_ref()
                    .is_none_or(|(directory, _)| directory != &path)
                {
                    view.class = Some((path.clone(), "?".into()));
                    view.outbox.borrow_mut().push(r#"{"c":"fsinfo"}"#.into());
                }
                let class = view.class.as_ref().map_or("?", |(_, class)| class.as_str());
                if class == "?" && setting.automatic {
                    return;
                }
                let class_on = match class {
                    "network" => preview.get("thumbNetwork").and_then(Json::as_bool) == Some(true),
                    "phone" => preview.get("thumbPhone").and_then(Json::as_bool) == Some(true),
                    "usb" => preview.get("thumbUsb").and_then(Json::as_bool) != Some(false),
                    _ => true,
                };
                let manual = !setting.automatic
                    || (!class_on
                        && preview.get("thumbnails").and_then(Json::as_str) != Some("off"));
                view.follow(subject, manual, false, window, cx);
            });
            let published = readers::Reader {
                view: Some(entity.read(cx)),
                window,
                active: visible,
                cx,
            }
            .value("previewColumnState", &[])
            .unwrap_or_default();
            entity
                .read(cx)
                .port
                .0
                .borrow_mut()
                .readers
                .insert("previewColumnState".into(), published);
            entity.read(cx).port.0.borrow_mut().wanted = false;
            for line in std::mem::take(&mut *entity.read(cx).outbox.borrow_mut()) {
                if let Some(backend) = host.backend.as_ref().filter(|_| !host.stopped) {
                    if let Err(error) = backend.line(line) {
                        host.stopped = true;
                        host.record_error(error, &self.locale);
                        break;
                    }
                }
            }
        }
    }

    pub(crate) fn selection_preview_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(entity) = self.panes[side]
            .as_ref()
            .and_then(|host| host.preview.clone())
        else {
            return false;
        };
        match event {
            Event::Failed(_) | Event::Drained => {
                entity.update(cx, |view, cx| view.hide(window, cx))
            }
            Event::FilesystemDetails { facts, class } => entity.update(cx, |view, cx| {
                if view
                    .class
                    .as_ref()
                    .is_some_and(|(path, _)| path == &facts.path)
                {
                    view.class = Some((facts.path.clone(), class.clone()));
                    cx.notify();
                }
            }),
            Event::ListDetails(crate::list::sizes::Reply::Filesystem { path, class }) => entity
                .update(cx, |view, cx| {
                    if view
                        .class
                        .as_ref()
                        .is_some_and(|(directory, _)| directory == path)
                    {
                        view.class = Some((path.clone(), class.clone()));
                        cx.notify();
                    }
                }),
            Event::Producer(line) => {
                let Ok(document) = flea::jsondoc::parse(line) else {
                    return false;
                };
                return entity.update(cx, |view, cx| view.on_line(&document, line, window, cx));
            }
            _ => {}
        }
        false
    }

    pub(crate) fn stop_selection_previews(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick_look.as_ref().is_some_and(|h| h.custom.window.is_some()) {
            self.quick_look_tab_switch(window, cx);
        }
        for host in self.panes.iter().flatten() {
            if let Some(preview) = &host.preview {
                preview.update(cx, |preview, cx| preview.hide(window, cx));
            }
        }
    }
}
