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
        jsondoc::parse(&self.picker.drawn.controls_json())
            .ok()
            .and_then(|value| value.as_array().map(<[_]>::to_vec))
            .unwrap_or_default()
    }

    fn controls_value(&self) -> Vec<Json> {
        let mut controls = self.buttons();
        let listing = self.picker.browse.listing.read(self.cx);
        let state = listing.drawn();
        let header = &state.header.drawn;
        if header.frames > 0 {
            let bounds = header.bounds;
            for (key, label) in [
                ("name", &state.view.words.name),
                ("mode", &state.view.words.mode),
                ("size", &state.view.words.size),
                ("mtime", &state.view.words.modified),
                ("kind", &state.view.words.kind),
            ] {
                if let Some(cell) = crate::ipc::rows::header_cell(
                    &header.layout,
                    &state.drawn.metrics,
                    bounds.size.width.into(),
                    key,
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
            members.push(("view".into(), Json::Str("list".into())));
        }
        let text = jsondoc::render(&value);
        let text = text.trim();
        text[1..text.len() - 1].into()
    }
}
