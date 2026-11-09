//! The IPC reader for the collision card; every other dialog's reader belongs to its own unit.
use super::model::Operations;
use super::{host::Overlay, measured, model::Dialog, press::Press};
use crate::visual::{number, object};
use crate::{feedback::spinner::Mode, ipc::Adapter};
use flea::ipc::wire::Reply;
use flea::json::escape;
use flea::{jsondoc::Json, ui_logic_file_ops::collide};

pub struct OperationsAdapter<'a> {
    pub model: &'a Operations,
}

impl Adapter for OperationsAdapter<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        match function {
            "collideState" => Some(Reply::value(self.model.collide_state())),
            _ => None,
        }
    }
}

/// Only the Omi reader is installed in the window's loading diagnostics.
pub struct OmiAdapter<'a> {
    pub overlay: Option<&'a Overlay>,
}
impl Adapter for OmiAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "omiState" {
            return None;
        }
        let Some(overlay) = self.overlay else {
            return Some(Reply::value("[]"));
        };
        let jobs = &overlay.progress.jobs;
        let rows: Vec<_> = overlay
            .model
            .blocks
            .iter()
            .map(|(row, block)| {
                let mode = Mode::for_job(&row.kind);
                let shown = jobs.visible()
                    && !jobs.folded
                    && block.state != flea::ui_logic_file_ops::jobs::State::Failed;
                format!(
                    "{{\"id\":{},\"shown\":{},\"mode\":\"{}\",\"label\":\"{}\"}}",
                    row.id,
                    shown,
                    mode.id(),
                    escape(mode.label())
                )
            })
            .collect();
        Some(Reply::value(format!("[{}]", rows.join(","))))
    }
}

pub(crate) struct LiveOperationsAdapter<'a> {
    pub overlay: Option<&'a Overlay>,
}

impl Adapter for LiveOperationsAdapter<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        if function != "collideState" {
            return None;
        }
        let Some(overlay) = self.overlay else {
            return Some(Reply::value(r#"{"opened":false}"#));
        };
        let Some(Dialog::Collide {
            title,
            names,
            more,
            focus,
            ..
        }) = overlay.drawn_dialog()
        else {
            return Some(Reply::value(r#"{"opened":false}"#));
        };
        let frame = &overlay.frame;
        let buttons = collide::BUTTONS.map(|choice| {
            let part = frame.part(&format!("button-{}", Press::Collide(choice).code()));
            let visible = part.as_ref().is_some_and(|part| {
                part.bounds.size.width > gpui::px(0.) && part.bounds.size.height > gpui::px(0.)
            });
            object([
                ("name", Json::Str(choice.wire().into())),
                (
                    "label",
                    Json::Str(part.as_ref().map_or("", |part| part.text.as_str()).into()),
                ),
                ("visible", Json::Bool(visible)),
                ("enabled", Json::Bool(visible)),
                ("focused", Json::Bool(visible && choice == *focus)),
                (
                    "hovered",
                    Json::Bool(visible && overlay.pointer.hovered == Some(Press::Collide(choice))),
                ),
                (
                    "pressed",
                    Json::Bool(visible && overlay.pointer.pressed == Some(Press::Collide(choice))),
                ),
                ("rect", Json::Str(measured::rect(part.as_ref()))),
                ("centre", Json::Str(measured::centre(part.as_ref()))),
            ])
        });
        let title_part = frame.part("title");
        let explain = frame.part("explain");
        let body = frame.part("body");
        let row = frame.part("buttons");
        let fits = body.as_ref().zip(row.as_ref()).is_some_and(|(body, row)| {
            row.bounds.size.width <= body.bounds.size.width
                && collide::BUTTONS
                    .iter()
                    .filter_map(|choice| {
                        frame.part(&format!("button-{}", Press::Collide(*choice).code()))
                    })
                    .all(|part| measured::fits(&part, row))
        });
        let lines = explain.as_ref().map_or(0, |part| {
            (f32::from(part.bounds.size.height) / f32::from(part.line_height)).round() as usize
        });
        let value = object([
            ("opened", Json::Bool(true)),
            ("title", Json::Str(title.clone())),
            (
                "titleTruncated",
                Json::Bool(
                    title_part
                        .as_ref()
                        .is_some_and(|part| part.text_width > part.bounds.size.width),
                ),
            ),
            (
                "names",
                Json::Arr(
                    names
                        .iter()
                        .map(|name| Json::Str(name.name.clone()))
                        .collect(),
                ),
            ),
            ("more", Json::Str(more.clone())),
            ("focus", Json::Str(focus.wire().into())),
            (
                "explain",
                Json::Str(explain.map_or_else(String::new, |part| part.text)),
            ),
            ("explainLines", number(lines)),
            ("buttonsFit", Json::Bool(fits)),
            (
                "rect",
                Json::Str(measured::rect(frame.part("card").as_ref())),
            ),
            ("buttons", Json::Arr(buttons.into_iter().collect())),
        ]);
        Some(Reply::value(flea::jsondoc::render(&value).trim_end()))
    }
}
