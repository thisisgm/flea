use crate::{g2::op3::shell, state::Shell};
use flea::ui_logic_st2::contributions::ActionInput;
use gpui::{
    div, AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Styled, Window,
};

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;

pub(crate) struct Open {
    pub descriptor: ActionInput,
    pub value: String,
    pub focus: usize,
    sender: Option<async_channel::Sender<Option<String>>>,
}

impl Open {
    fn valid(&self) -> bool {
        !self.value.trim().is_empty()
            && self.value.chars().count() <= self.descriptor.max_chars
            && !self.value.chars().any(char::is_control)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(None);
        }
    }
}

impl Shell {
    pub(crate) fn cancel_plugin_input(&mut self, cx: &mut Context<Self>) {
        if self.plugins.input.take().is_some() {
            self.rename.ime.close();
            cx.notify();
        }
    }
    pub(crate) fn plugin_text_input(
        &mut self,
        descriptor: &ActionInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<async_channel::Receiver<Option<String>>> {
        if self.plugins.input.is_some() || self.card_holds_keyboard(cx) {
            return None;
        }
        let (sender, receiver) = async_channel::bounded(1);
        self.plugins.input = Some(Open {
            descriptor: descriptor.clone(),
            value: String::new(),
            focus: 0,
            sender: Some(sender),
        });
        window.focus(&self.focus, cx);
        self.sync_dialog_editor();
        cx.notify();
        Some(receiver)
    }

    pub(crate) fn finish_plugin_input(
        &mut self,
        submit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if submit && !self.plugins.input.as_ref().is_some_and(Open::valid) {
            return;
        }
        if let Some(mut open) = self.plugins.input.take() {
            if let Some(sender) = open.sender.take() {
                let _ = sender.try_send(submit.then(|| open.value.clone()));
            }
            self.rename.ime.close();
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub(crate) fn plugin_input_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.plugins.input.is_none() {
            return false;
        }
        if self.dialog_key(event, cx) {
            return true;
        }
        let key = event.keystroke.key.as_str();
        let focus = self.plugins.input.as_ref().unwrap().focus;
        match key {
            "escape" => self.finish_plugin_input(false, window, cx),
            "enter" | "return" | "kp_enter" => self.finish_plugin_input(focus != 1, window, cx),
            "tab" | "backtab" => {
                let backwards = key == "backtab" || event.keystroke.modifiers.shift;
                self.plugins.input.as_mut().unwrap().focus =
                    (focus + if backwards { 2 } else { 1 }) % 3;
                self.sync_dialog_editor();
                cx.notify();
            }
            _ => {}
        }
        cx.stop_propagation();
        true
    }

    pub(crate) fn plugin_input_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.plugins.input.as_ref()?;
        self.sync_dialog_editor();
        let open = self.plugins.input.as_ref()?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        self.rename.ime.palette = Some(crate::g2::op1::palette::CardPalette::from_theme(
            &window.text_style(),
            theme,
            self.corner,
        ));
        self.rename.ime.error_ink = false;
        let card = shell::Card {
            language: self.locale.catalog.language().into(),
            title: open.descriptor.title.clone(),
            esc: self.locale.catalog.tr("dialogs.output.esc"),
            width: 440.,
            bleed: false,
            body: vec![shell::Block::Field(shell::Field {
                input: (open.focus == 0).then(|| cx.entity()),
                label: Some(open.descriptor.label.clone()),
                value: open.value.clone(),
                placeholder: open.descriptor.placeholder.clone(),
                glyph: None,
                focused: open.focus == 0,
            })],
            buttons: vec![
                shell::Button {
                    label: self.locale.catalog.tr("dialogs.output.cancel"),
                    primary: false,
                    focused: open.focus == 1,
                    enabled: true,
                },
                shell::Button {
                    label: open.descriptor.submit.clone(),
                    primary: true,
                    focused: open.focus == 2,
                    enabled: open.valid(),
                },
            ],
        };
        let entity = cx.entity().downgrade();
        let wire = move |control, row: gpui::Stateful<gpui::Div>| {
            let entity = entity.clone();
            row.on_mouse_down(
                MouseButton::Left,
                move |_: &gpui::MouseDownEvent, window, cx| {
                    let _ = entity.update(cx, |shell, cx| match control {
                        shell::Control::Field => {
                            if let Some(open) = &mut shell.plugins.input {
                                open.focus = 0;
                            }
                            shell.sync_dialog_editor();
                            cx.notify();
                        }
                        shell::Control::Button(index) => {
                            shell.finish_plugin_input(index == 1, window, cx)
                        }
                        _ => {}
                    });
                    cx.stop_propagation();
                },
            )
        };
        let drawn = shell::DialogShell::present(
            &shell::Interactive {
                wire: &wire,
                surface: theme.surface,
                mount: None,
            },
            &card,
            &palette,
            &self.active_typeface(),
            self.corner,
            f32::from(window.viewport_size().width),
        );
        Some(
            div()
                .id("plugin-action-input")
                .debug_selector(|| "plugin-action-input".into())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .bg(palette
                    .background
                    .opacity(flea::ui_logic_op1::card::scrim_alpha(0)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|shell, _: &gpui::MouseDownEvent, window, cx| {
                        shell.finish_plugin_input(false, window, cx)
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(drawn),
                )
                .into_any_element(),
        )
    }
}
