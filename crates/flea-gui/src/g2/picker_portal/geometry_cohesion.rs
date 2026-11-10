//! Cohesion reads the chooser's painted controls and its actual keyboard owner.
use super::Live;
use flea::jsondoc::Json;
use gpui::{Bounds, Pixels};

fn project(mut source: Json, kind: &str) -> Option<Json> {
    let values = source.get("bounds")?.as_array()?;
    let values: Vec<f32> = values
        .iter()
        .filter_map(Json::as_f64)
        .map(|n| n as f32)
        .collect();
    let [x, y, w, h] = values.as_slice() else {
        return None;
    };
    let visible = source
        .get("visible")
        .and_then(Json::as_bool)
        .unwrap_or(true)
        && *w > 0.
        && *h > 0.;
    if let Json::Obj(fields) = &mut source {
        fields.retain(|(key, _)| !matches!(key.as_str(), "rect" | "centre" | "visible"));
        fields.extend([
            (
                "rect".into(),
                Json::Str(crate::ipc::rows::rect_of(*x, *y, *w, *h)),
            ),
            (
                "centre".into(),
                Json::Str(crate::ipc::rows::centre_of(*x, *y, *w, *h)),
            ),
            ("visible".into(), Json::Bool(visible)),
        ]);
    }
    crate::ipc::cohesion_controls::control(&source, "", kind)
}

impl Live<'_> {
    fn observed(
        &self,
        id: &str,
        name: &str,
        bounds: Bounds<Pixels>,
        kind: &str,
        enabled: bool,
    ) -> Option<Json> {
        let mut source = super::control(
            name,
            bounds,
            self.picker.chrome.focused == Some(id),
            enabled,
        );
        if let (Some((hovered, pressed)), Json::Obj(fields)) =
            (self.picker.chrome.probe.live_pointer(id), &mut source)
        {
            fields.extend([
                ("hovered".into(), Json::Bool(hovered)),
                ("pressed".into(), Json::Bool(pressed)),
            ]);
        }
        project(source, kind)
    }

    pub(super) fn cohesion(&self) -> String {
        let mut controls: Vec<_> = self
            .buttons()
            .into_iter()
            .enumerate()
            .filter_map(|(index, mut row)| {
                if let Json::Obj(fields) = &mut row {
                    fields.retain(|(key, _)| !matches!(key.as_str(), "primary" | "destructive"));
                    fields.extend([
                        (
                            "primary".into(),
                            Json::Bool(index == 1 && self.picker.answer_is_default()),
                        ),
                        (
                            "destructive".into(),
                            Json::Bool(index == 1 && self.picker.accept_state().collision_review()),
                        ),
                    ]);
                }
                project(row, "DialogButton")
            })
            .collect();
        for (id, key, enabled) in [
            (
                "picker-back",
                "keys.action.historyBack",
                !self.picker.chrome.history.is_empty(),
            ),
            ("picker-up", "keys.action.parent", self.picker.can_go_up()),
            ("picker-list", "keys.action.viewList", true),
            ("picker-grid-view", "keys.action.viewGrid", true),
            ("picker-new-folder", "picker.browser.new_folder", true),
        ] {
            if let Some(bounds) = self.picker.chrome.probe.bounds(id) {
                controls.extend(self.observed(
                    id,
                    &self.picker.locale.catalog.tr(key),
                    bounds,
                    "ChromeButton",
                    enabled,
                ));
            }
        }
        if let Some(bounds) = self.picker.chrome.probe.bounds("picker-filter") {
            let enabled =
                !self.picker.backend_unavailable() && !self.picker.accept_state().submitting();
            controls.extend(self.observed(
                "picker-filter",
                &self.picker.filter_label(),
                bounds,
                "PickerFilter",
                enabled,
            ));
        }
        let idle = !self.picker.accept_state().submitting() && !self.picker.backend_unavailable();
        for (at, entry) in self.picker.rail.entries.iter().enumerate() {
            let id = format!("rail:{at}");
            let probe = &self.picker.chrome.probe;
            if let Some(bounds) = probe
                .bounds(&id)
                .zip(probe.bounds(&format!("{id}:mask")))
                .map(|(bounds, mask)| bounds.intersect(&mask))
            {
                let name = self
                    .picker
                    .rail
                    .labels
                    .get(at)
                    .map_or("", |text| text.as_ref());
                let mut row = super::control(
                    name,
                    bounds,
                    self.picker.chrome.focused == Some("picker-rail")
                        && self.picker.rail.cursor == at,
                    entry.error.is_none() && idle,
                );
                if let Json::Obj(fields) = &mut row {
                    fields.push(("pressLive".into(), Json::Bool(false)));
                }
                controls.extend(project(row, "SidebarRow"));
            }
        }
        let mut state = crate::ipc::cohesion_controls::state(controls);
        let listing = self.picker.browse.listing.read(self.cx);
        let focus = if listing.keyboard_focused() {
            let cursor = listing.listing().cursor();
            let bounds = if listing.view_mode() == "grid" {
                let grid = listing.grid_state();
                let grid = grid.borrow();
                grid.drawn_row(cursor).map(|row| {
                    Bounds::new(
                        grid.bounds.origin
                            + gpui::point(gpui::px(row.cell.x), gpui::px(row.cell.y)),
                        gpui::size(gpui::px(row.cell.width), gpui::px(row.cell.height)),
                    )
                })
            } else {
                let list = listing.drawn();
                use crate::ipc::Adapter;
                let text = crate::ipc::rows::RowsAdapter {
                    list: &list,
                    held: &listing.listing().held(),
                    states: &Default::default(),
                }
                .call("rowRect", &[cursor.to_string()])
                .map(|reply| reply.stdout)
                .unwrap_or_default();
                // Sample input: rowRect's reply "232 96 640 28".
                let values: Vec<f32> = text
                    .split_whitespace()
                    .filter_map(|n| n.parse().ok())
                    .collect();
                match values.as_slice() {
                    [x, y, w, h] => Some(Bounds::new(
                        gpui::point(gpui::px(*x), gpui::px(*y)),
                        gpui::size(gpui::px(*w), gpui::px(*h)),
                    )),
                    _ => None,
                }
            };
            bounds.and_then(|bounds| project(super::control("", bounds, true, true), "Row"))
        } else {
            None
        };
        if let (Some(focus), Json::Obj(fields)) = (focus, &mut state) {
            if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key == "focus") {
                *value = focus;
            }
        }
        flea::jsondoc::render(&state).trim_end().into()
    }
}
