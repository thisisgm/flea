//! The picker's bare chrome marks surround its shared path strip.
use super::body::PATH_HEIGHT;
use super::view::{GAP, SIDE};
use super::Picker;
use crate::{g2::picker_listing::chrome::PathBar, list::Palette};
use gpui::{div, prelude::*, Context, Entity, Render, WeakEntity, Window};
use std::{cell::RefCell, rc::Rc};

// picker-path: each bare chrome mark takes a 24 px slot the strip's height.
const MARK_WIDTH: f32 = 24.;

#[cfg(test)]
thread_local! {
    /// The marks the last render drew, for a test that reads each mark's painted ink.
    pub(crate) static DRAWN: std::cell::Cell<Option<Rc<RefCell<crate::path::chrome::Frame>>>> = const { std::cell::Cell::new(None) };
}

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
        let size = super::size::Size::of(window, cx);
        let frame = Rc::new(RefCell::new(crate::path::chrome::Frame::default()));
        #[cfg(test)]
        DRAWN.set(Some(frame.clone()));
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
            let ink = palette.mark(enabled && (index < 2 || selected));
            let handle = picker.clone();
            let mut control = owner
                .chrome
                .probe
                .attach(div().relative(), id)
                .id(id)
                .debug_selector(move || id.into())
                .w(size.px(MARK_WIDTH))
                .h(size.px(PATH_HEIGHT))
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
            let pressed = picker.clone();
            control = control.on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                let now = cx.background_executor().now();
                pressed.update(cx, |picker, _| picker.chrome.pressed_at = Some(now));
            });
            // Enter and Space act on their press in control_key; gpui's keyboard click on the release would step a second time (EY-355).
            controls.push(control.on_click(move |event, window, cx| {
                let held = handle.read(cx).chrome.pressed_at;
                let long = matches!(event, gpui::ClickEvent::Mouse(_))
                    && !crate::path::tap::short_press(held, cx.background_executor().now());
                if !enabled || long || event.is_keyboard() {
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
            .gap(size.px(GAP))
            .px(size.px(SIDE))
            .border_b_1()
            .border_color(palette.foreground.opacity(0.12))
            .children(controls.next())
            .children(controls.next())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .debug_selector(|| "picker-path-slot".into())
                    // Capture runs before the strip's breadcrumb press, so one click opens the entry instead of navigating.
                    .capture_any_mouse_down(move |event, window, cx| {
                        picker.update(cx, |picker, cx| picker.path_press(event, window, cx));
                    })
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.path.clone()),
            )
            .children(controls)
    }
}
