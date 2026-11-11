//! Buttons use the shared card recipe, including the default wash beneath pointer feedback.
use super::{
    dialog::Focus,
    drawing::{BoxPaint, Drawn, Rect},
    view::NetworkView,
};
use flea::ui_logic_file_ops::card::{self, ButtonState};
use gpui::Window;

impl Drawn {
    pub(super) fn button(
        &mut self,
        focus: Focus,
        rect: Rect,
        text: &str,
        view: &NetworkView,
        window: &Window,
    ) {
        let p = view.palette;
        let disabled = view.model.saving || (focus != Focus::Cancel && view.model.connecting);
        let look = card::look(ButtonState {
            primary: focus == Focus::Connect,
            focused: view.model.focus == focus,
            disabled,
            hovered: view.hovered == Some(focus),
            pressed: view.pressed == Some(focus) && view.hovered == Some(focus),
            rest_is_foreground: p.accent_frame == p.foreground,
            ..ButtonState::default()
        });
        self.hits.push((focus, rect));
        let scale = if look.pressed { card::PRESS_SCALE } else { 1. };
        let rect = Rect {
            x: rect.x + rect.w * (1. - scale) / 2.,
            y: rect.y + rect.h * (1. - scale) / 2.,
            w: rect.w * scale,
            h: rect.h * scale,
        };

        self.boxes.push(BoxPaint {
            field: None,
            rect,
            fill: p
                .ink(look.wash)
                .map_or(p.surface, |ink| ink.opacity(look.wash_alpha)),
            border: p
                .ink(look.frame)
                .unwrap_or(p.muted_frame)
                .opacity(look.alpha),
            border_width: look.frame_width,
            radius: p.radius,
            clip: None,
        });
        if let Some(ink) = p.ink(look.overlay) {
            let inset = look.frame_width;
            self.boxes.push(BoxPaint {
                field: None,
                rect: Rect {
                    x: rect.x + inset,
                    y: rect.y + inset,
                    w: (rect.w - 2. * inset).max(0.),
                    h: (rect.h - 2. * inset).max(0.),
                },
                fill: ink.opacity(look.overlay_alpha),
                border: ink.opacity(0.),
                border_width: 0.,
                radius: (p.radius - inset).max(0.),
                clip: None,
            });
        }
        self.label(
            text,
            Rect {
                x: rect.x + 10. * scale,
                w: rect.w - 20. * scale,
                ..rect
            },
            p.foreground.opacity(look.alpha),
            14. * scale,
            false,
            window,
        );
    }
}
