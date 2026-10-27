//! Conversion readers use the mounted output card and the focused backend's capabilities.
use super::Adapter;
use crate::{g2::op4::model::Card, state::Shell};
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct OutputAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
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
