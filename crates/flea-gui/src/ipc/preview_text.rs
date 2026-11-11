//! Text preview readers project the mounted code viewport, including its held swap and focus.
use super::Adapter;
use crate::{
    g2::{doc_preview::code::CodeView, quick_look_overlay::doc::View},
    state::Shell,
    window::selection_preview::{docs::Doc, Mounted},
};
use flea::{ipc::wire::Reply, json::escape};
use gpui::{App, Window};

pub(crate) struct PreviewText<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

fn state(code: &CodeView, visible: bool, focused: bool, ready: bool) -> String {
    let row = code.metrics.row;
    let before = (0..code.top).map(|n| code.rows_of(n)).sum::<usize>() as f32;
    let total = (0..code.lines.total)
        .map(|n| code.rows_of(n))
        .sum::<usize>() as f32;
    let height = (code.frame.1 - code.metrics.top).max(0.);
    let maximum = (total * row - height).max(0.);
    format!(
        r#"{{"path":"{}","visible":{visible},"ready":{},"focused":{focused},"wrap":{},"y":{},"minY":0,"maxY":{maximum},"height":{height},"total":{}}}"#,
        escape(&code.lines.path),
        ready && code.first_screen(),
        code.wrap,
        before * row + code.offset,
        code.lines.total
    )
}

impl Adapter for PreviewText<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function != "previewTextState" {
            return None;
        }
        let value = if arguments.first().is_some_and(|column| column == "true") {
            self.shell
                .selection_preview(self.shell.focus_side)
                .and_then(|entity| {
                    let view = entity.read(self.cx);
                    let frame = view.drawn.as_ref()?;
                    let Mounted::Doc(Doc::Code(code)) = &frame.mounted else {
                        return None;
                    };
                    let held = view.port.0.borrow().holding;
                    Some(state(
                        code.read(self.cx),
                        view.visible,
                        // Preview.qml activeFocus: the column or a view inside it holds the keyboard.
                        view.focus.contains_focused(self.window, self.cx),
                        !held && view.pending.is_none(),
                    ))
                })
        } else {
            self.shell.quick_look.as_ref().and_then(|host| {
                let look = host.view.read(self.cx);
                let doc = look.subject()?.body.doc()?;
                let code = match &doc.view {
                    View::Code(code) => code.read(self.cx),
                    _ => return None,
                };
                Some(state(
                    code,
                    look.is_open(),
                    self.shell.focus.is_focused(self.window),
                    look.is_revealed(),
                ))
            })
        };
        Some(Reply::value(value.unwrap_or_else(|| "{}".into())))
    }
}
