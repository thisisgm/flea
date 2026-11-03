//! Conversion readers use the mounted output card and the focused backend's capabilities.
use super::{
    rows::{centre_of, rect_of},
    Adapter,
};
use crate::{g2::op4::model::Card, state::Shell};
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct OutputAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
}

impl OutputAdapter<'_> {
    /// GPUI-only (v0.3.12 has no Combine card): the open card's settings, the focused field and image, in drawing order.
    fn combine_state(&self) -> String {
        use flea::jsondoc::Json;
        let Some(model) = self
            .shell
            .output
            .view
            .as_ref()
            .map(|view| &view.read(self.cx).model)
        else {
            return r#"{"opened":false}"#.into();
        };
        let Some(Card::Combine(combine)) = &model.card else {
            return r#"{"opened":false}"#.into();
        };
        let word = |debug: String| Json::Str(debug.to_lowercase());
        let settings = &combine.settings;
        let images = model.selection.as_ref().map_or_else(Vec::new, |selection| {
            selection
                .files
                .iter()
                .map(|file| {
                    let name = std::path::Path::new(&file.path)
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                    Json::Str(name)
                })
                .collect()
        });
        flea::jsondoc::render(&Json::Obj(vec![
            ("opened".into(), Json::Bool(true)),
            ("layout".into(), word(format!("{:?}", settings.layout))),
            ("spacing".into(), word(format!("{:?}", settings.spacing))),
            (
                "background".into(),
                word(format!("{:?}", settings.background)),
            ),
            ("columns".into(), Json::Num(settings.columns.to_string())),
            ("format".into(), Json::Str(combine.format.clone())),
            ("name".into(), Json::Str(combine.name.clone())),
            (
                "focus".into(),
                model
                    .focus
                    .map_or(Json::Null, |focus| word(format!("{focus:?}"))),
            ),
            (
                "focusedImage".into(),
                Json::Num(combine.focused.map_or(-1, |at| at as i64).to_string()),
            ),
            ("images".into(), Json::Arr(images)),
            ("collision".into(), Json::Bool(combine.collision)),
            ("busy".into(), Json::Bool(model.busy)),
        ]))
    }
}

impl Adapter for OutputAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function == "canConvert" {
            return Some(Reply::value(
                self.shell.output.convert[self.shell.focus_side]
                    .unwrap_or(false)
                    .to_string(),
            ));
        }
        if function == "combineState" {
            return Some(Reply::value(self.combine_state()));
        }
        if matches!(function, "convertCardRect" | "convertTitleCentre") {
            // Ipc.qml retains the built Convert dialog's last geometry after close or replacement.
            let view = self
                .shell
                .output
                .view
                .as_ref()
                .map(|view| view.read(self.cx));
            let observed = view.and_then(|view| {
                if function == "convertCardRect" {
                    view.observed.convert_card.get().map(|b| {
                        rect_of(
                            f32::from(b.origin.x),
                            f32::from(b.origin.y),
                            f32::from(b.size.width),
                            f32::from(b.size.height),
                        )
                    })
                } else {
                    view.observed.convert_title.get().map(|b| {
                        centre_of(
                            f32::from(b.origin.x),
                            f32::from(b.origin.y),
                            f32::from(b.size.width),
                            f32::from(b.size.height),
                        )
                    })
                }
            });
            return Some(Reply::value(observed.unwrap_or_default()));
        }
        if !matches!(
            function,
            "convertOpen" | "convertFormat" | "convertStrip" | "convertState"
        ) {
            return None;
        }
        let model = self
            .shell
            .output
            .view
            .as_ref()
            .map(|view| &view.read(self.cx).model);
        let convert = model.and_then(|model| match &model.card {
            Some(Card::Convert(convert)) => Some(convert),
            _ => None,
        });
        let value = match function {
            "convertOpen" => convert.is_some().to_string(),
            "convertFormat" | "convertStrip" => {
                let Some(convert) = convert else {
                    if model.is_some() {
                        return Some(Reply::error(3, format!("FLEA_IPC_ERROR flea.{function} unavailable field: retained conversion draft (closing or replacing the output card discards it)")));
                    }
                    return Some(Reply::value(if function == "convertFormat" {
                        ""
                    } else {
                        "false"
                    }));
                };
                if function == "convertFormat" {
                    convert.format.clone()
                } else {
                    convert.strip.to_string()
                }
            }
            "convertState" => {
                if model.is_some() {
                    return Some(Reply::error(3, "FLEA_IPC_ERROR flea.convertState unavailable fields: retained conversion draft, outputLines, outputRect, bodyRect, rect, formats control feedback (the output card does not record the oracle's complete painted state)"));
                }
                "{\"opened\":false}".into()
            }
            _ => unreachable!(),
        };
        Some(Reply::value(value))
    }
}
