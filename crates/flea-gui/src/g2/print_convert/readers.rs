//! The Convert reader projects the draft and the controls the current card actually painted.
use super::{
    controls::Control,
    host::OutputView,
    model::{Card, Focus},
};
use flea::{jsondoc::Json, ui_logic_print_convert::output::Kind};

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn text(value: impl Into<String>) -> Json {
    Json::Str(value.into())
}
fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}

impl OutputView {
    fn read_control(&self, control: Control, name: &str, enabled: bool, focused: bool) -> Json {
        let probe = &self.observed.probe;
        let id = format!("control:{}", control.code());
        let clip = match control {
            Control::Cancel | Control::Submit => &id,
            _ => "body",
        };
        let centre = probe.centre(&id, clip);
        let visible = !centre.is_empty();
        object([
            ("name", text(name)),
            ("visible", Json::Bool(visible)),
            ("enabled", Json::Bool(enabled)),
            ("focused", Json::Bool(focused)),
            ("hovered", Json::Bool(self.hovered == Some(control))),
            ("pressed", Json::Bool(self.pressed == Some(control))),
            ("rect", text(probe.rect(&id))),
            ("centre", text(centre)),
        ])
    }

    pub(crate) fn convert_state(&self) -> String {
        let model = &self.model;
        let Some(Card::Convert(convert)) = &model.card else {
            return r#"{"opened":false}"#.into();
        };
        let formats = convert
            .kind
            .formats()
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let control = Control::Choice(Focus::Formats, index);
                let mut row = self.read_control(
                    control,
                    name,
                    model.editable(),
                    model.focus == Some(Focus::Formats) && convert.cursor == index,
                );
                let Json::Obj(fields) = &mut row else {
                    unreachable!()
                };
                fields.extend([
                    ("current".into(), Json::Bool(convert.cursor == index)),
                    ("selected".into(), Json::Bool(convert.format == *name)),
                    (
                        "labelColor".into(),
                        text(crate::ipc::rows::colour(self.palette.foreground)),
                    ),
                    (
                        "markColor".into(),
                        text(crate::ipc::rows::colour(if convert.format == *name {
                            self.palette.foreground
                        } else {
                            self.palette.muted_frame
                        })),
                    ),
                    (
                        "pointerProbe".into(),
                        text(self.pointer_position.map_or_else(String::new, |at| {
                            format!("{} {}", f32::from(at.x).round(), f32::from(at.y).round())
                        })),
                    ),
                ]);
                row
            })
            .collect();
        let mut controls = vec![
            self.read_control(
                Control::Cancel,
                "Cancel",
                model.can_cancel(),
                model.focus == Some(Focus::Cancel),
            ),
            self.read_control(
                Control::Submit,
                "Convert",
                model.can_submit(),
                model.focus == Some(Focus::Submit),
            ),
        ];
        if convert.kind == Kind::Image {
            let combine = convert.format == "pdf"
                && model
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.files.len() > 1);
            controls.push(
                self.read_control(
                    Control::Check(Focus::Check),
                    if combine {
                        "Combine into one PDF"
                    } else {
                        "Remove metadata"
                    },
                    model.editable()
                        && convert.check_enabled(
                            model
                                .selection
                                .as_ref()
                                .map_or(0, |selection| selection.files.len()),
                        ),
                    model.focus == Some(Focus::Check),
                ),
            );
        }
        let probe = &self.observed.probe;
        let rect = |bounds: Option<gpui::Bounds<gpui::Pixels>>| {
            bounds.map_or_else(String::new, |bounds| {
                crate::ipc::rows::rect_of(
                    bounds.origin.x.into(),
                    bounds.origin.y.into(),
                    bounds.size.width.into(),
                    bounds.size.height.into(),
                )
            })
        };
        let output_lines = probe.bounds("output").map_or(0, |bounds| {
            (f32::from(bounds.size.height) / super::view::OUTPUT_LINE_HEIGHT).round() as usize
        });
        let state = object([
            ("opened", Json::Bool(true)),
            ("title", text(model.title(&self.catalog))),
            ("busy", Json::Bool(model.busy)),
            ("unavailable", Json::Bool(model.unavailable)),
            ("checking", Json::Bool(convert.pending.is_some())),
            ("canConvert", Json::Bool(model.can_submit())),
            ("format", text(convert.format.clone())),
            ("strip", Json::Bool(convert.strip)),
            ("collision", Json::Bool(convert.collision)),
            ("error", text(model.error.clone())),
            ("requestId", number(model.request_id())),
            (
                "source",
                object([
                    (
                        "menuId",
                        number(
                            model
                                .selection
                                .as_ref()
                                .map_or(0, |selection| selection.menu),
                        ),
                    ),
                    (
                        "path",
                        text(
                            model
                                .selection
                                .as_ref()
                                .and_then(|selection| selection.files.first())
                                .map_or("", |file| file.path.as_str()),
                        ),
                    ),
                ]),
            ),
            ("rect", text(rect(self.observed.card.get()))),
            ("bodyRect", text(probe.rect("body"))),
            ("outputRect", text(probe.rect("output"))),
            ("outputLines", number(output_lines)),
            (
                "outputText",
                text(format!(
                    "{}{}",
                    self.catalog.tr("dialogs.output.output"),
                    super::view::convert_path(self, convert)
                )),
            ),
            (
                "scrollY",
                number(-f32::from(self.body_scroll.handle.offset().y)),
            ),
            ("formats", Json::Arr(formats)),
            ("controls", Json::Arr(controls)),
        ]);
        flea::jsondoc::render(&state).trim_end().into()
    }
}
