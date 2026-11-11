//! The footer's "z undoes" target under the pointer runs the undo the z key runs.
use super::status::Footer;
use crate::{g2::status_feedback::line::MessageLine, state::Shell};
use gpui::{Context, Entity};

impl Shell {
    /// The footer's own line, its undo target wired to the shared undo, which refuses wherever an outside undo is.
    pub(super) fn footer_undo_line(
        &mut self,
        footer: &Entity<Footer>,
        cx: &mut Context<Self>,
    ) -> Entity<MessageLine> {
        let line = footer.read_with(cx, |footer, _| footer.line.clone());
        let shell = cx.entity().downgrade();
        line.update(cx, |line, _| {
            line.on_undo(std::rc::Rc::new(move |window, cx| {
                let _ = shell.update(cx, |shell, cx| shell.run_builtin_action("undo", window, cx));
            }))
        });
        line
    }
}

#[cfg(test)]
#[path = "status_undo_click_tests.rs"]
mod tests;
