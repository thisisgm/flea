//! Make executable uses the Permissions card's journaled batch and keeps replies on their pane.
use super::number;
use crate::{
    g2::context_menus::{delivery::string, join::Captured},
    state::Shell,
    window::navigate::Step,
};
use flea::{
    jsondoc::Json,
    ui_logic_context_menus::rows,
    ui_logic_open_with_properties::permissions::{self, Batch},
    ui_logic_status_feedback::status::Hint,
};
use gpui::{Context, Window};

#[derive(Default)]
pub(super) struct Execution {
    sequence: [u64; 2],
    pending: [Option<u64>; 2],
}

impl Shell {
    pub(crate) fn make_executable(
        &mut self,
        capture: &Captured,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = capture.side;
        let context = &self.menu.context;
        let allowed = capture.ready
            && !capture.background
            && capture.rows.as_slice() == [capture.cursor]
            && paths.len() == 1
            && capture.identity == self.menu_identity()
            && rows::can_make_executable(
                context.row_mode,
                capture.rows.len(),
                context.has_shebang,
                context.cursor_is_target,
            );
        if !allowed {
            let sentence = self.locale.catalog.tr("menu.make_executable.refused");
            self.make_executable_notice(side, &sentence, false, Hint::None, cx);
            self.retire_menu(window, cx);
            return;
        }
        let batch = Batch {
            paths: paths.to_vec(),
            modes: vec!["u+x".into()],
            ..Batch::default()
        };
        let execution = &mut self.menu.shebang.execution;
        execution.sequence[side] += 1;
        let id = permissions::MAKE_EXEC_ID + execution.sequence[side];
        execution.pending[side] = Some(id);
        self.menu_send(side, permissions::batch_request(id, &batch), cx);
        self.retire_menu(window, cx);
    }

    pub(super) fn make_executable_reply(
        &mut self,
        side: usize,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = number(reply, "id");
        if self.menu.shebang.execution.pending[side] != Some(id)
            || string(reply, "op") != "applyMany"
        {
            return;
        }
        self.menu.shebang.execution.pending[side] = None;
        let ok = reply.get("ok").and_then(Json::as_bool) == Some(true);
        let error = string(reply, "error");
        let sentence = if ok {
            self.locale.catalog.tr("menu.make_executable.made")
        } else if error.is_empty() {
            self.locale.catalog.tr("menu.make_executable.failed")
        } else {
            error.to_owned()
        };
        self.make_executable_notice(
            side,
            &sentence,
            !ok,
            if ok { Hint::Undo } else { Hint::None },
            cx,
        );
        if ok {
            self.menu.shebang.has = false;
            self.menu.context.has_shebang = false;
            let _ = self.step(side, Step::Reload, window, cx);
        }
    }

    fn make_executable_notice(
        &mut self,
        side: usize,
        sentence: &str,
        error: bool,
        hint: Hint,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = &mut self.panes[side] {
            host.status = sentence.to_owned().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(sentence, error, "", hint, now);
        });
        cx.notify();
    }
}
