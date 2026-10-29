//! The picker's bare chrome marks surround its shared path strip.
use super::Picker;
use crate::{g2::pk2::chrome::PathBar, list::Palette};
use gpui::{div, prelude::*, px, Context, Entity, Render, WeakEntity, Window};
use std::{cell::RefCell, rc::Rc};

pub struct ChromeView {
    pub picker: WeakEntity<Picker>,
    pub path: Entity<PathBar>,
}

impl Render for ChromeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let Some(picker) = self.picker.upgrade() else {
            return div();
        };
        let owner = picker.read(cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let frame = Rc::new(RefCell::new(crate::path::chrome::Frame::default()));
        let mode = owner.browse.listing.read(cx).view_mode();
        let buttons = [
            (
                "picker-back",
                "arrow-left",
                !owner.chrome.history.is_empty(),
                Some(owner.chrome.back.clone()),
            ),
            (
                "picker-up",
                "arrow-up",
                owner.can_go_up(),
                Some(owner.chrome.up.clone()),
            ),
            ("picker-list", "list", true, None),
            ("picker-grid-view", "grid", true, None),
        ];
        let mut controls = Vec::new();
        for (index, (id, glyph, enabled, focus)) in buttons.into_iter().enumerate() {
            let selected = (index == 2 && mode == "list") || (index == 3 && mode == "grid");
            let ink = if index < 2 || selected {
                palette.foreground
            } else {
                palette.metadata
            };
            let handle = picker.clone();
            let mut control = owner
                .chrome
                .probe
                .attach(div().relative(), id)
                .id(id)
                .debug_selector(move || id.into())
                .w(px(24.))
                .h(px(27.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .when(!enabled, |view| view.opacity(0.55))
                .child(crate::path::mark::mark(glyph, ink, frame.clone(), index));
            if let Some(focus) = focus {
                control = control
                    .track_focus(&focus)
                    .when(focus.is_focused(window), |view| {
                        view.border_1().border_color(palette.foreground)
                    });
            }
            controls.push(control.on_click(move |_, window, cx| {
                if !enabled {
                    return;
                }
                handle.update(cx, |picker, cx| {
                    window.focus(&picker.focus, cx);
                    match index {
                        0 => picker.go_back(cx),
                        1 => picker.go_up(cx),
                        2 => picker.user_set_view("list", cx),
                        _ => picker.user_set_view("grid", cx),
                    }
                });
            }));
        }
        let mut controls = controls.into_iter();
        div()
            .size_full()
            .flex()
            .items_center()
            .gap(px(9.))
            .px(px(14.))
            .border_b_1()
            .border_color(palette.foreground.opacity(0.12))
            .children(controls.next())
            .children(controls.next())
            .child(div().flex_1().min_w_0().child(self.path.clone()))
            .children(controls)
    }
}
