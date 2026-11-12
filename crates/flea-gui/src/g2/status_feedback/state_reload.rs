//! The waiting pane's plain framed Reload, using the existing DialogButton recipe.
use super::{state::StateMessage, Ink};
use crate::g2::file_ops::glyph::Chrome;
use flea::ui_logic_file_ops::card;
use gpui::{
    div, prelude::*, px, Context, Div, EventEmitter, FocusHandle, MouseButton, SharedString,
    Stateful, Window,
};
use std::time::Instant;

// The board's sizes at its 14px body text, scaled with the live body size.
const BOARD_BODY: f32 = 14.;
const LABEL_INSET: f32 = 10.;
const ABOVE_FRAME: f32 = 9.;

/// The framed control's width for a label of `label` px at `body` px text.
pub(super) fn width(label: f32, body: f32) -> f32 {
    label + 2. * LABEL_INSET * body / BOARD_BODY
}

pub struct Reload;
impl EventEmitter<Reload> for StateMessage {}

pub(super) struct Button {
    pub focus: FocusHandle,
    hovered: bool,
    pressed: bool,
    motion: Option<(bool, Instant)>,
}

impl Button {
    pub(super) fn reset(&mut self) {
        self.hovered = false;
        self.pressed = false;
        self.motion = None;
    }

    fn press(&mut self, pressed: bool, now: Instant) {
        if self.pressed != pressed {
            self.pressed = pressed;
            self.motion = Some((pressed, now));
        }
    }
}

impl StateMessage {
    pub fn set_corner(&mut self, corner: f32, cx: &mut Context<Self>) {
        if self.corner != corner {
            self.corner = corner;
            cx.notify();
        }
    }

    pub fn reload_focus(&mut self, cx: &mut Context<Self>) -> FocusHandle {
        self.reload_state(cx).focus.clone()
    }

    fn reload_state(&mut self, cx: &mut Context<Self>) -> &mut Button {
        self.reload.get_or_insert_with(|| Button {
            focus: cx.focus_handle(),
            hovered: false,
            pressed: false,
            motion: None,
        })
    }

    pub(super) fn reload_button(
        &mut self,
        label: SharedString,
        ink: Ink,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let button = self.reload_state(cx);
        let focus = button.focus.clone();
        let pressed = button.pressed && button.hovered;
        let hovered = button.hovered;
        let motion = button.motion;
        let body = f32::from(self.typeface.body);
        let scale = body / BOARD_BODY;
        let width = width(
            super::caption::measure(
                label.clone(),
                self.typeface.font.clone(),
                self.typeface.body,
                px(0.),
                window,
            ),
            body,
        );
        let height = card::BUTTON_HEIGHT * scale;
        let frame = Chrome {
            frame: if focus.is_focused(window) {
                ink.foreground
            } else {
                ink.muted_frame
            },
            frame_width: card::BORDER,
            radius: self.corner,
            motion,
            wash: (pressed || hovered).then(|| {
                ink.foreground.opacity(if pressed {
                    card::PRESS_WASH
                } else {
                    card::HOVER_WASH
                })
            }),
            overlay: None,
        };
        div()
            .id("fb1-reload")
            .debug_selector(|| "fb1-reload".into())
            .track_focus(&focus)
            .relative()
            .flex_none()
            .mt(px(ABOVE_FRAME * scale))
            .w(px(width))
            .h(px(height))
            .px(px(LABEL_INSET * scale))
            .flex()
            .items_center()
            .cursor_pointer()
            .font_family(self.typeface.font.family.clone())
            .text_size(self.typeface.body)
            .text_color(ink.foreground)
            .child(frame)
            .child(super::caption::Tracked {
                text: label,
                font: self.typeface.font.clone(),
                size: self.typeface.body,
                // A line as tall as the frame, so label and frame round from one edge in every view.
                line: px(height),
                tracking: px(0.),
                color: ink.foreground,
            })
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if let Some(button) = &mut view.reload {
                    button.hovered = *hovered;
                    if !hovered {
                        button.press(false, cx.background_executor().now());
                    }
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| {
                    let button = view.reload_state(cx);
                    window.focus(&button.focus, cx);
                    button.press(true, cx.background_executor().now());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    if let Some(button) = &mut view.reload {
                        button.press(false, cx.background_executor().now());
                    }
                    cx.notify();
                }),
            )
            .on_click(cx.listener(|view, _, _, cx| view.activate_reload(cx)))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.modifiers == gpui::Modifiers::none()
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    if !event.is_held {
                        view.activate_reload(cx);
                    }
                    cx.stop_propagation();
                }
            }))
    }

    fn activate_reload(&mut self, cx: &mut Context<Self>) {
        if self.facts().is_some_and(|facts| facts.reload.is_some()) {
            if let Some(button) = &mut self.reload {
                button.pressed = false;
            }
            cx.emit(Reload);
        }
    }
}
