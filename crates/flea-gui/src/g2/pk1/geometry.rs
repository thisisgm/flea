//! Picker geometry reads the controls and listing that painted the requested frame.
use super::{ipc::PickerGeometry, Picker};
use crate::{
    ipc::{rows::RowsAdapter, Adapter},
    list::RowStates,
};
use flea::jsondoc::{self, Json};
use gpui::{App, Bounds, Pixels};

pub(super) struct Live<'a> {
    pub picker: &'a Picker,
    pub cx: &'a App,
}

fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}

fn rect(bounds: Bounds<Pixels>) -> [f32; 4] {
    [
        bounds.origin.x.into(),
        bounds.origin.y.into(),
        bounds.size.width.into(),
        bounds.size.height.into(),
    ]
}

fn control(name: &str, bounds: Bounds<Pixels>, focused: bool, enabled: bool) -> Json {
    let [x, y, width, height] = rect(bounds);
    crate::visual::object([
        ("name", Json::Str(name.into())),
        ("visible", Json::Bool(width > 0. && height > 0.)),
        ("enabled", Json::Bool(enabled)),
        ("focused", Json::Bool(focused)),
        (
            "centre",
            Json::Str(crate::ipc::rows::centre_of(x, y, width, height)),
        ),
        (
            "bounds",
            Json::Arr([x, y, width, height].map(number).to_vec()),
        ),
    ])
}

impl Live<'_> {
    fn buttons(&self) -> Vec<Json> {
        let mut buttons: Vec<Json> = jsondoc::parse(&self.picker.drawn.controls_json())
            .ok()
            .and_then(|value| value.as_array().map(<[_]>::to_vec))
            .unwrap_or_default();
        for (index, button) in buttons.iter_mut().enumerate() {
            if let Json::Obj(members) = button {
                members.retain(|(key, _)| key != "focused");
                let id = if index == 0 {
                    "picker-cancel"
                } else {
                    "picker-answer"
                };
                members.push((
                    "focused".into(),
                    Json::Bool(self.picker.chrome.focused == Some(id)),
                ));
            }
        }
        buttons
    }

    fn controls_value(&self) -> Vec<Json> {
        let mut controls = self.buttons();
        for (id, key, enabled) in [
            (
                "picker-back",
                "keys.action.historyBack",
                !self.picker.chrome.history.is_empty(),
            ),
            ("picker-up", "keys.action.parent", self.picker.can_go_up()),
            ("picker-list", "keys.action.viewList", true),
            ("picker-grid-view", "keys.action.viewGrid", true),
            ("picker-filter", "picker.browser.filter", true),
            ("picker-new-folder", "picker.browser.new_folder", true),
        ] {
            if let Some(bounds) = self.picker.chrome.probe.bounds(id) {
                controls.push(control(
                    &self.picker.locale.catalog.tr(key),
                    bounds,
                    self.picker.chrome.focused == Some(id),
                    enabled,
                ));
            }
        }
        let listing = self.picker.browse.listing.read(self.cx);
        let state = listing.drawn();
        let header = &state.header.drawn;
        if header.frames > 0 && listing.view_mode() == "list" {
            let bounds = header.bounds;
            for (key, label) in [
                ("name", &state.view.words.name),
                ("mode", &state.view.words.mode),
                ("size", &state.view.words.size),
                ("date", &state.view.words.modified),
                ("kind", &state.view.words.kind),
            ] {
                if let Some(cell) = crate::ipc::rows::header_cell(
                    &header.layout,
                    &state.drawn.metrics,
                    bounds.size.width.into(),
                    key,
                    false,
                )
                .filter(|cell| cell.width > 0. && !label.is_empty())
                {
                    let bounds = Bounds::new(
                        gpui::point(bounds.origin.x + gpui::px(cell.x), bounds.origin.y),
                        gpui::size(gpui::px(cell.width), bounds.size.height),
                    );
                    controls.push(control(
                        label,
                        bounds,
                        false,
                        !listing.listing().is_pending(),
                    ));
                }
            }
        }
        if let Some(field) = &self.picker.browse.filename {
            let field = field.read(self.cx);
            if let Some(bounds) = field.probe.bounds("field") {
                controls.push(control(
                    &self.picker.locale.catalog.tr("picker.browser.filename"),
                    bounds,
                    field.focused,
                    true,
                ));
            }
        }
        // PickerPlaces.qml controls(): every rail row last, refused while it errs, submits or has no backend.
        let rail = &self.picker.rail;
        let idle = !self.picker.accept_state().submitting() && !self.picker.backend_unavailable();
        let rail_focused = self.picker.chrome.focused == Some("picker-rail");
        for (index, entry) in rail.entries.iter().enumerate() {
            let id = format!("rail:{index}");
            let Some(bounds) = self
                .picker
                .chrome
                .probe
                .bounds(&id)
                .zip(self.picker.chrome.probe.bounds(&format!("{id}:mask")))
                .map(|(bounds, mask)| bounds.intersect(&mask))
                .filter(|bounds| {
                    bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.)
                })
            else {
                continue;
            };
            controls.push(control(
                rail.labels.get(index).map_or("", |label| label.as_ref()),
                bounds,
                rail_focused && rail.cursor == index,
                entry.error.is_none() && idle,
            ));
        }
        controls
    }
}

impl PickerGeometry for Live<'_> {
    fn controls(&self) -> String {
        jsondoc::render(&Json::Arr(self.controls_value()))
            .trim_end()
            .into()
    }

    fn row_centre(&self, index: usize) -> String {
        let listing = self.picker.browse.listing.read(self.cx);
        if listing.view_mode() == "grid" {
            let state = listing.grid_state();
            let state = state.borrow();
            let Some(tile) = state.drawn_row(index) else {
                return String::new();
            };
            return crate::ipc::rows::centre_of(
                f32::from(state.bounds.origin.x) + tile.cell.x,
                f32::from(state.bounds.origin.y) + tile.cell.y,
                tile.cell.width,
                tile.cell.height,
            );
        }
        let state = listing.drawn();
        let held = listing.listing().held();
        let reader = RowsAdapter {
            list: &state,
            held: &held,
            states: &RowStates::default(),
        };
        reader
            .call("rowCentre", &[index.to_string()])
            .map_or_else(String::new, |reply| reply.stdout.trim_end().into())
    }

    fn cohesion_controls(&self) -> String {
        let controls = self
            .buttons()
            .into_iter()
            .enumerate()
            .filter_map(|(index, button)| {
                let bounds = button.get("bounds")?.as_array()?;
                Some(crate::visual::object([
                    ("kind", Json::Str("DialogButton".into())),
                    ("name", button.get("name")?.clone()),
                    ("x", bounds[0].clone()),
                    ("y", bounds[1].clone()),
                    ("w", bounds[2].clone()),
                    ("h", bounds[3].clone()),
                    ("focused", button.get("focused")?.clone()),
                    ("hovered", button.get("hovered")?.clone()),
                    ("pressed", button.get("pressed")?.clone()),
                    ("hoverable", Json::Bool(true)),
                    ("pressable", Json::Bool(true)),
                    ("enabled", button.get("enabled")?.clone()),
                    ("value", Json::Str(String::new())),
                    (
                        "primary",
                        Json::Bool(index == 1 && self.picker.answer_is_default()),
                    ),
                    (
                        "destructive",
                        Json::Bool(index == 1 && self.picker.accept_state().collision_review()),
                    ),
                    ("member", Json::Bool(false)),
                    ("via", Json::Str(String::new())),
                ]))
            })
            .collect();
        jsondoc::render(&crate::visual::object([
            ("focus", Json::Null),
            ("controls", Json::Arr(controls)),
        ]))
        .trim_end()
        .into()
    }

    fn path_bar_open(&self) -> bool {
        self.picker
            .browse
            .path_view(self.cx)
            .read(self.cx)
            .editing()
    }

    fn save_field_centre(&self) -> String {
        self.picker
            .browse
            .filename
            .as_ref()
            .map_or_else(String::new, |field| {
                let field = field.read(self.cx);
                field
                    .probe
                    .bounds("field")
                    .map_or_else(String::new, |bounds| {
                        let [x, y, width, height] = rect(bounds);
                        crate::ipc::rows::centre_of(x, y, width, height)
                    })
            })
    }

    fn snapshot_members(&self) -> String {
        let text = format!("{{{}}}", self.picker.drawn.snapshot_members());
        let mut value = jsondoc::parse(&text).unwrap_or_else(|_| Json::Obj(Vec::new()));
        if let Json::Obj(members) = &mut value {
            members.retain(|(key, _)| key != "controls");
            members.push(("controls".into(), Json::Arr(self.controls_value())));
            members.push(("pathBarOpen".into(), Json::Bool(self.path_bar_open())));
            // PickerWindow.qml snapshot.jump: the shared path view's dropdown, its rows, drawn rows, scroll, cursor and paths.
            let path = self.picker.browse.path_view(self.cx).read(self.cx);
            let number = |n: f64| Json::Num(n.to_string());
            members.push((
                "jump".into(),
                Json::Obj(vec![
                    ("open".into(), Json::Bool(path.jump.shown())),
                    ("rows".into(), number(path.jump.entries.len() as f64)),
                    (
                        "visibleRows".into(),
                        number(path.paint.borrow().drawn as f64),
                    ),
                    ("contentY".into(), number(f64::from(path.scroll))),
                    (
                        "cursor".into(),
                        number(path.jump.cursor.map_or(-1., |at| at as f64)),
                    ),
                    (
                        "paths".into(),
                        Json::Arr(
                            path.jump
                                .entries
                                .iter()
                                .map(|entry| Json::Str(entry.path.clone()))
                                .collect(),
                        ),
                    ),
                ]),
            ));
            let order = self.picker.browse.listing.read(self.cx).listing().order();
            members.push(("sortBy".into(), Json::Str(order.key.into())));
            members.push(("sortDesc".into(), Json::Bool(order.desc)));
            members.push((
                "view".into(),
                Json::Str(self.picker.browse.listing.read(self.cx).view_mode().into()),
            ));
        }
        let text = jsondoc::render(&value);
        let text = text.trim();
        text[1..text.len() - 1].into()
    }
}
