//! A native action prompt keeps its original backend snapshot until submit validates it again.
use crate::{g2::context_menus::join::Captured, state::Shell};
use flea::{jsondoc::Json, ui_logic_plugin_settings::contributions::ActionInput};
use gpui::{Context, Window};

pub(super) struct Pending {
    capture: Captured,
    action: String,
    descriptor: ActionInput,
    paths: Vec<String>,
    value: Option<String>,
}

pub(super) enum Input {
    Ready(Option<Json>),
    Waiting,
    Refused,
}

impl Shell {
    pub(super) fn action_prompt(
        &mut self,
        capture: &Captured,
        action: &str,
        descriptor: Option<ActionInput>,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Input {
        if let Some(pending) = &self.menu.plugin_runtime.input {
            if pending.capture.id != capture.id || pending.action != action {
                return Input::Refused;
            }
            let Some(value) = pending.value.clone() else {
                return Input::Waiting;
            };
            if descriptor.as_ref() != Some(&pending.descriptor) || pending.paths != paths {
                return Input::Refused;
            }
            let key = pending.descriptor.key.clone();
            self.menu.plugin_runtime.input = None;
            self.menu.plugin_runtime.input_task = None;
            return Input::Ready(Some(Json::Obj(vec![(key, Json::Str(value))])));
        }
        let Some(descriptor) = descriptor else {
            return Input::Ready(None);
        };
        self.menu.menu.borrow_mut().close();
        self.menu.presentation.cancel();
        self.pause_plugin_menu();
        self.menu.activating = false;
        let Some(receiver) = self.plugin_text_input(&descriptor, window, cx) else {
            return Input::Refused;
        };
        self.menu.plugin_runtime.input = Some(Pending {
            capture: capture.clone(),
            action: action.into(),
            descriptor,
            paths: paths.to_vec(),
            value: None,
        });
        let id = capture.id;
        self.menu.plugin_runtime.input_task = Some(cx.spawn_in(window, async move |this, cx| {
            let value = receiver.recv().await.ok().flatten();
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.action_input_finished(id, value, window, cx)
            });
        }));
        Input::Waiting
    }

    fn action_input_finished(
        &mut self,
        id: u64,
        value: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self
            .menu
            .plugin_runtime
            .input
            .as_ref()
            .filter(|p| p.capture.id == id)
        else {
            return;
        };
        if self
            .menu
            .captured
            .as_ref()
            .is_none_or(|capture| capture.id != id || capture.side != pending.capture.side)
        {
            return;
        }
        let Some(value) = value else {
            self.retire_menu(window, cx);
            return;
        };
        if value.trim().is_empty()
            || value.chars().any(char::is_control)
            || value.chars().count() > pending.descriptor.max_chars
        {
            self.menu_refuse("menu.refusal.unavailable", cx);
            self.retire_menu(window, cx);
            return;
        }
        self.menu.pending = Some((pending.action.clone(), String::new()));
        self.menu.plugin_runtime.input.as_mut().unwrap().value = Some(value);
        self.menu.activating = false;
        self.activate_menu(window, cx);
    }
}
