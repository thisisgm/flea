//! The native toolbar owns its painted controls and hosts the existing path surface.
use super::{
    mark::{divider, mark, mark_ink, press_scale, Press},
    view::PathView,
};
use crate::{list::Palette, palette::Theme, window::startup::View};
use gpui::{
    canvas, div, prelude::*, px, Bounds, Context, Entity, EventEmitter, FocusHandle, KeyDownEvent,
    MouseButton, Pixels, Point, Window,
};
use std::{cell::RefCell, rc::Rc, time::Instant};

pub const GLYPHS: [&str; 10] = [
    "arrow-left",
    "arrow-up",
    "search",
    "filter",
    "sort",
    "list",
    "columns",
    "grid",
    "dual",
    "sliders",
];
pub const ACTIONS: [&str; 10] = [
    "back",
    "up",
    "search",
    "filter",
    "sortNext",
    "viewList",
    "viewColumns",
    "viewGrid",
    "viewDual",
    "settings",
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Control {
    pub bounds: Option<Bounds<Pixels>>,
    pub visible: bool,
    pub enabled: bool,
    pub focused: bool,
    pub hovered: bool,
    pointer_over: bool,
    pub pressed: bool,
    /// The initiating left press owns the tap only until release or permanent cancellation.
    pub pointer_down: bool,
    pub press_origin: Option<Point<Pixels>>,
    pub press: Option<Press>,
}

impl Control {
    pub fn set_pressed(&mut self, down: bool, now: Instant, reduced: bool) {
        if !down {
            self.pointer_down = false;
            self.press_origin = None;
        }
        if self.pressed != down {
            let from = press_scale(self.press, now, reduced).0;
            self.press = Some(Press {
                down,
                since: now,
                from,
            });
            self.pressed = down;
        }
    }
}

#[derive(Default)]
pub struct Frame {
    pub bounds: Option<Bounds<Pixels>>,
    pub controls: [Control; 10],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inputs {
    pub view: View,
    pub back: bool,
    pub up: bool,
    pub filter: bool,
    pub sort: bool,
    pub live: bool,
    pub corner: f32,
}

pub struct ChromeView {
    pub path: Entity<PathView>,
    pub inputs: Inputs,
    pub frame: Rc<RefCell<Frame>>,
    pub focus: [FocusHandle; 10],
}

impl EventEmitter<&'static str> for ChromeView {}

impl ChromeView {
    pub fn new(path: Entity<PathView>, inputs: Inputs, cx: &mut Context<Self>) -> Self {
        Self {
            path,
            inputs,
            frame: Default::default(),
            focus: std::array::from_fn(|_| cx.focus_handle()),
        }
    }

    fn button(
        &self,
        index: usize,
        enabled: bool,
        resting: gpui::Hsla,
        foreground: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let frame = self.frame.clone();
        let glyph = GLYPHS[index];
        {
            let mut frame = frame.borrow_mut();
            frame.controls[index].visible = true;
            frame.controls[index].enabled = enabled;
        }
        let focused = self.frame.borrow().controls[index].focused && enabled;
        let ink = mark_ink(enabled, focused, resting, foreground);
        let hoverable = self.inputs.live;
        let live = enabled && hoverable;
        div()
            .id(("chrome-button", index))
            .w(px(24.))
            .h(px(27.))
            .flex_none()
            .cursor_pointer()
            .relative()
            .flex()
            .justify_center()
            .track_focus(&self.focus[index])
            .on_key_down(cx.listener(move |_, event: &KeyDownEvent, _, cx| {
                if live
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                    && event.keystroke.modifiers == gpui::Modifiers::none()
                {
                    cx.emit(ACTIONS[index]);
                    cx.stop_propagation();
                }
            }))
            .hover_listener_mode(gpui::HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                let mut frame = view.frame.borrow_mut();
                let control = &mut frame.controls[index];
                control.pointer_over = *hovered;
                control.hovered = control.pointer_over && view.inputs.live;
                let down = control.pointer_down && control.hovered && control.enabled;
                if control.pressed != down {
                    control.set_pressed(down, cx.background_executor().now(), cx.reduce_motion());
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                    if live && view.inputs.live {
                        let mut frame = view.frame.borrow_mut();
                        let control = &mut frame.controls[index];
                        control.pointer_down = true;
                        control.press_origin = Some(event.position);
                        control.set_pressed(
                            true,
                            cx.background_executor().now(),
                            cx.reduce_motion(),
                        );
                        drop(frame);
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |view, event: &gpui::MouseUpEvent, _, cx| {
                    let mut frame = view.frame.borrow_mut();
                    let control = &mut frame.controls[index];
                    let activate = live
                        && view.inputs.live
                        && control.pointer_down
                        && super::tap::within_threshold(control.press_origin, event.position)
                        && control.pressed
                        && control
                            .bounds
                            .is_some_and(|bounds| bounds.contains(&event.position));
                    control.pointer_down = false;
                    control.set_pressed(false, cx.background_executor().now(), cx.reduce_motion());
                    drop(frame);
                    cx.notify();
                    if activate {
                        cx.emit(ACTIONS[index]);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |view, _, _, cx| {
                    let mut frame = view.frame.borrow_mut();
                    let control = &mut frame.controls[index];
                    if control.pointer_down {
                        control.pointer_down = false;
                        control.set_pressed(
                            false,
                            cx.background_executor().now(),
                            cx.reduce_motion(),
                        );
                        cx.notify();
                    }
                }),
            )
            .child(div().h(px(26.)).flex().items_center().child(mark(
                glyph,
                ink,
                self.frame.clone(),
                index,
            )))
            .child(
                canvas(
                    move |bounds, _, _| frame.borrow_mut().controls[index].bounds = Some(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(focused, |button| {
                button.child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .top(px(3.))
                        .size(px(20.))
                        .rounded(px(self.inputs.corner / 2.))
                        .border_1()
                        .border_color(foreground),
                )
            })
    }
}

impl Render for ChromeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        for control in &mut self.frame.borrow_mut().controls {
            if control.visible {
                control.bounds = None;
            }
            control.visible = false;
            control.enabled = false;
        }
        for (index, focus) in self.focus.iter().enumerate() {
            self.frame.borrow_mut().controls[index].focused = focus.is_focused(window);
        }
        let ink = |enabled| {
            if enabled {
                palette.foreground
            } else {
                palette.metadata_frame.opacity(0.55)
            }
        };
        let mut right = div().h_full().flex().items_center().gap(px(9.)).flex_none();
        if self.inputs.view == View::Grid {
            right = right
                .child(self.button(
                    3,
                    self.inputs.filter,
                    ink(self.inputs.filter),
                    palette.foreground,
                    cx,
                ))
                .child(self.button(
                    4,
                    self.inputs.sort,
                    ink(self.inputs.sort),
                    palette.foreground,
                    cx,
                ));
        } else {
            right = right.child(self.button(2, true, palette.foreground, palette.foreground, cx));
        }
        right = right.child(divider(palette.foreground));
        for (index, mode) in [
            (5, View::List),
            (6, View::Columns),
            (7, View::Grid),
            (8, View::Dual),
        ] {
            right = right.child(self.button(
                index,
                true,
                if self.inputs.view == mode {
                    palette.foreground
                } else {
                    palette.metadata_frame
                },
                palette.foreground,
                cx,
            ));
        }
        right = right.child(divider(palette.foreground)).child(self.button(
            9,
            true,
            palette.foreground,
            palette.foreground,
            cx,
        ));
        let frame = self.frame.clone();
        let pointer_frame = self.frame.clone();
        let toolbar = div().size_full().relative().bg(theme.chrome()).child(
            div()
                .size_full()
                .flex()
                .gap(px(9.))
                .px(px(14.))
                .child(
                    div()
                        .flex()
                        .gap(px(9.))
                        .flex_none()
                        .child(self.button(
                            0,
                            self.inputs.back,
                            ink(self.inputs.back),
                            palette.foreground,
                            cx,
                        ))
                        .child(self.button(
                            1,
                            self.inputs.up,
                            ink(self.inputs.up),
                            palette.foreground,
                            cx,
                        )),
                )
                .child(div().h(px(26.)).flex_1().min_w_0().child(self.path.clone()))
                .child(right),
        );
        for control in &mut self.frame.borrow_mut().controls {
            control.hovered = control.pointer_over && control.visible && self.inputs.live;
            if !control.visible || !control.enabled || !self.inputs.live {
                control.pointer_down = false;
                control.set_pressed(false, cx.background_executor().now(), cx.reduce_motion());
            }
        }
        toolbar
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .w_full()
                    .h(px(1.))
                    .bg(palette.foreground.opacity(0.12)),
            )
            .child(
                canvas(
                    move |bounds, _, _| frame.borrow_mut().bounds = Some(bounds),
                    move |_, _, window, _| super::tap::observe(pointer_frame.clone(), window),
                )
                .absolute()
                .size_full(),
            )
    }
}

pub struct PanePath {
    pub path: Entity<PathView>,
    pub side: usize,
}

impl Render for PanePath {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let probe = self.path.read_with(cx, |view, _| view.probe.clone());
        super::probe::wrap(
            div()
                .size_full()
                .child(super::render::pane_strip(&self.path, self.side, cx)),
            self.path.clone(),
            1 + self.side,
            probe,
        )
    }
}
