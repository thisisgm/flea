//! Conversion settings and geometry in the legacy IPC shapes, read from the mounted output owner.
use super::{rows::{centre_of, rect_of}, Adapter};
use crate::{g2::op4::host::OutputView, state::Shell};
use flea::ipc::wire::Reply;
use gpui::App;

pub(crate) struct OutputAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
}

impl Adapter for OutputAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if !matches!(function, "convertCardRect" | "convertTitleCentre" | "convertFormat" | "convertStrip") {
            return None;
        }
        let view: Option<&OutputView> = self.shell.output.view.as_ref().map(|view| view.read(self.cx));
        let settings = view.and_then(|view| view.model.conversion_settings());
        let value = match function {
            "convertFormat" => settings.map_or("", |(format, _)| format).to_owned(),
            "convertStrip" => settings.is_some_and(|(_, strip)| strip).to_string(),
            "convertCardRect" | "convertTitleCentre" => {
                view.filter(|_| settings.is_some()).and_then(|view| {
                    if function == "convertCardRect" {
                        view.observed.card.get().map(rect_of)
                    } else {
                        view.observed.title.get().map(centre_of)
                    }
                }).unwrap_or_default()
            }
            _ => unreachable!(),
        };
        Some(Reply::value(value))
    }
}
