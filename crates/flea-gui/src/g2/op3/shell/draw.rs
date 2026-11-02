//! The stand-in frame: the card, its title, its controls and its buttons, drawn to the boards.
use super::*;
use crate::g2::fb1::caption::{tracking_for, Tracked};
use gpui::{
    div, prelude::*, px, relative, AnyElement, App, Div, FontWeight, Pixels, Stateful, Window,
};
use std::rc::Rc;

/// Draws a card centred with no scrim, until op1's shell replaces it.
pub struct StandIn;

impl Draw<'_> {
    pub(super) fn wired(
        &self,
        control: Control,
        element: gpui::Stateful<Div>,
    ) -> gpui::Stateful<Div> {
        let id = match control {
            Control::Field => "field".into(),
            Control::Check(at) => format!("check:{at}"),
            Control::Button(at) => format!("button:{at}"),
        };
        let element = match self.probe {
            Some(probe) => probe.track(element, id),
            None => element,
        };
        match self.wire {
            Some(wire) => wire(control, element),
            None => element,
        }
    }

    pub(super) fn px(&self, board: f32) -> Pixels {
        px(board * self.s)
    }

    pub(super) fn text(&self, size: f32) -> Div {
        div().text_size(self.px(size)).whitespace_nowrap()
    }

    pub(super) fn hairline(&self) -> Div {
        div().h(px(1.)).flex_none().bg(self.t.rule)
    }

    pub(super) fn check(&self, check: Check) -> AnyElement {
        let t = self.t;
        let on = check.value != Tri::Off;
        let frame = if on || check.focused {
            t.fg
        } else {
            t.muted_frame
        };
        let mut box_ = div()
            .size(self.px(CHECK))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .border_2()
            .border_color(frame)
            .rounded(px(self.corner / 2.))
            .when(on, |d| d.bg(t.fg));
        box_ = match check.value {
            Tri::On => box_.child(glyph(Glyph::Check, 12. * self.s, t.surface)),
            Tri::Mixed => box_.child(div().w(self.px(8.)).h(self.px(2.)).bg(t.surface)),
            Tri::Off => box_,
        };
        // A focused filled box takes a ring 2 outside it, since its own frame is already foreground.
        if check.focused && on {
            let ring = div()
                .absolute()
                .top(px(-2.))
                .left(px(-2.))
                .right(px(-2.))
                .bottom(px(-2.))
                .border_1()
                .border_color(t.fg)
                .rounded(px(self.corner / 2.));
            return div()
                .relative()
                .flex_none()
                .child(box_)
                .child(ring)
                .into_any_element();
        }
        box_.into_any_element()
    }

    pub(super) fn caption(&self, text: &str, line: f32, color: Hsla) -> Tracked {
        Tracked {
            text: flea::ui_logic_st1::caps::heading(text, self.language).into(),
            font: self.typeface.font.clone(),
            size: self.px(CAPTION),
            line: self.px(line),
            tracking: self.px(tracking_for(self.language)),
            color,
        }
    }

    pub(super) fn label(&self, text: &str) -> Div {
        self.text(CAPTION)
            .line_height(self.px(14.))
            .mb(self.px(5.))
            .text_color(self.t.muted)
            .child(self.caption(text, 14., self.t.muted))
    }

    pub(super) fn field(&self, field: &Field) -> Div {
        let t = self.t;
        let caret = || {
            div()
                .relative()
                .w(px(0.))
                .h(self.px(16.))
                .flex_none()
                .child(
                    div()
                        .debug_selector(|| "op3-caret".into())
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(1.))
                        .h_full()
                        .bg(t.fg),
                )
        };
        let frame = if field.focused {
            t.accent_frame
        } else {
            t.muted_frame
        };
        if let Some(probe) = self.probe {
            let role = if frame == t.error {
                "error"
            } else if frame == t.accent {
                "accent"
            } else if frame == t.muted_frame {
                "mutedFrame"
            } else if frame == t.muted {
                "muted"
            } else {
                "other"
            };
            probe.set_role("field", role);
        }
        let mut line = div()
            .h(self.px(BUTTON_HEIGHT))
            .flex()
            .items_center()
            .gap(self.px(8.))
            .px(self.px(BUTTON_PAD))
            .bg(t.ground)
            .border_1()
            .border_color(frame)
            .rounded(px(self.corner))
            .text_color(t.fg)
            .whitespace_nowrap();
        if let Some(mark) = field.glyph {
            line = line.child(glyph(mark, 14. * self.s, t.muted));
        }
        let slot = || {
            div().relative().flex_1().min_w_0().h_full().when(
                field.value.is_empty() && !field.placeholder.is_empty(),
                |slot| {
                    slot.child(
                        div()
                            .absolute()
                            .left(px(1.))
                            .top_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .text_color(t.muted)
                            .child(field.placeholder.clone()),
                    )
                },
            )
        };
        // A Shell card names its input; a live card's own entity mounts its focused field.
        let mounted = match (&field.input, self.mount) {
            (Some(input), _) => Some(
                crate::g2::op1::field::mount(input.clone(), slot(), self.typeface)
                    .into_any_element(),
            ),
            (None, Some(mount)) if field.focused => Some(mount(slot())),
            _ => None,
        };
        if let Some(mounted) = mounted {
            line = line.child(mounted);
            return match &field.label {
                Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
                None => line,
            };
        }
        let mut text = div()
            .flex()
            .items_center()
            .min_w_0()
            .pr(px(1.))
            .overflow_hidden();
        if field.value.is_empty() {
            if field.focused {
                text = text.child(caret());
            }
            if !field.placeholder.is_empty() {
                text = text.child(
                    div()
                        .ml(px(1.))
                        .text_color(t.muted)
                        .child(field.placeholder.clone()),
                );
            }
        } else {
            text = text.child(field.value.clone());
            if field.focused {
                text = text.child(caret());
            }
        }
        let line = line.child(crate::locale_geometry::unmirrored(text));
        match &field.label {
            Some(name) => div().flex().flex_col().child(self.label(name)).child(line),
            None => line,
        }
    }

    pub(super) fn button(&self, button: &Button) -> Div {
        let t = self.t;
        let primary = button.primary && button.enabled;
        let frame = match (button.focused, primary) {
            (true, _) => t.fg,
            (false, true) => t.accent_frame,
            (false, false) => t.muted_frame,
        };
        self.text(BOARD_BODY)
            .h(self.px(BUTTON_HEIGHT))
            .px(self.px(BUTTON_PAD))
            .flex()
            .items_center()
            .border_1()
            .border_color(frame)
            .rounded(px(self.corner))
            .text_color(t.fg)
            .when(primary, |d| d.bg(t.accent.opacity(WASH_DEFAULT)))
            .when(!button.enabled, |d| d.opacity(DISABLED))
            .child(button.label.clone())
    }
}

impl DialogShell for StandIn {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement {
        present(card, palette, typeface, corner, room, None, None)
    }
}

impl StandIn {
    pub fn present_pressable(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        press: impl Fn(&usize, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.present_pressable_observed(card, palette, typeface, corner, room, (press, None))
    }
    pub(crate) fn present_observed(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        probe: &crate::ipc::card_geometry::CardGeometry,
    ) -> AnyElement {
        present(card, palette, typeface, corner, room, None, Some(probe))
    }
    /// The same card with live buttons: a click on one reports its index, as Enter on it would press it.
    pub(crate) fn present_pressable_observed(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        controls: (
            impl Fn(&usize, &mut Window, &mut App) + 'static,
            Option<&crate::ipc::card_geometry::CardGeometry>,
        ),
    ) -> AnyElement {
        let (press, probe) = controls;
        let press = Rc::new(press);
        let enabled: Vec<bool> = card.buttons.iter().map(|button| button.enabled).collect();
        // A disabled button draws as before but takes no click.
        let wire = move |control, element: Stateful<Div>| match control {
            Control::Button(at) if enabled.get(at).copied().unwrap_or(false) => {
                let press = press.clone();
                element.on_click(move |_, window, cx| press(&at, window, cx))
            }
            _ => element,
        };
        let live = Live {
            wire: &wire,
            surface: None,
            mount: None,
        };
        present(card, palette, typeface, corner, room, Some(&live), probe)
    }
}

impl DialogShell for Interactive<'_> {
    fn present(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
    ) -> AnyElement {
        let live = Live {
            wire: self.wire,
            surface: Some(self.surface),
            mount: self.mount,
        };
        present(card, palette, typeface, corner, room, Some(&live), None)
    }
}

impl Interactive<'_> {
    pub(crate) fn present_observed(
        &self,
        card: &Card,
        palette: &Palette,
        typeface: &Typeface,
        corner: f32,
        room: f32,
        probe: &crate::ipc::card_geometry::CardGeometry,
    ) -> AnyElement {
        let live = Live {
            wire: self.wire,
            surface: Some(self.surface),
            mount: self.mount,
        };
        present(
            card,
            palette,
            typeface,
            corner,
            room,
            Some(&live),
            Some(probe),
        )
    }
}

/// The one live path every card takes: its wired controls, and the window's surface and mounted field where it has them.
struct Live<'a> {
    wire: &'a Wire,
    surface: Option<Hsla>,
    mount: Option<&'a Mount>,
}

mod frame;
use frame::present;

#[path = "pressable.rs"]
mod pressable;
pub use pressable::Probed;
