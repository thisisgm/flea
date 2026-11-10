//! The native toolbar owns its painted controls and hosts the existing path surface.
use super::{
    mark::{divider, press_scale, resting_ink, Press, MARK},
    view::PathView,
};
use crate::{
    list::{Metrics, Palette, Typeface},
    window::startup::View,
};
use gpui::{
    canvas, div, prelude::*, px, Bounds, Context, Entity, EventEmitter, FocusHandle, Pixels, Point,
    Window,
};
use std::{cell::RefCell, rc::Rc, time::Instant};

mod address;
mod button;

/// The copy mark after the path is the last control, so every earlier index keeps its place.
pub const COPY: usize = 10;

pub const GLYPHS: [&str; 11] = [
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
    "copy",
];
pub const ACTIONS: [&str; 11] = [
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
    "copyAddress",
];
/// The order the keyboard walks the controls, as they stand left to right: the copy mark follows the path.
pub const ORDER: [usize; 11] = [0, 1, COPY, 2, 3, 4, 5, 6, 7, 8, 9];

#[derive(Clone, Copy, Debug, Default)]
pub struct Control {
    pub bounds: Option<Bounds<Pixels>>,
    #[cfg(test)]
    pub mark_bounds: Option<Bounds<Pixels>>,
    #[cfg(test)]
    pub mark_ink: Option<gpui::Hsla>,
    /// The sprite's drawn width in device pixels: its snapped bounds times its transform's scale.
    #[cfg(test)]
    pub mark_drawn: Option<f32>,
    pub visible: bool,
    /// Built this frame and so on the keyboard's walk; the copy mark stays reachable while it draws nothing.
    pub reachable: bool,
    pub enabled: bool,
    pub focused: bool,
    pub hovered: bool,
    pointer_over: bool,
    pub pressed: bool,
    /// The initiating left press owns the tap only until release or permanent cancellation.
    pub pointer_down: bool,
    pub press_origin: Option<Point<Pixels>>,
    pub press_at: Option<Instant>,
    pub press: Option<Press>,
}

impl Control {
    pub fn set_pressed(&mut self, down: bool, now: Instant, reduced: bool) {
        if !down {
            self.pointer_down = false;
            self.press_origin = None;
            self.press_at = None;
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

/// Theme.hitMin: a chrome button's hit box is at least this wide.
const HIT_MIN: f32 = 24.;

/// ChromeBar.qml and ChromeButton.qml at a text size: the band, the mark, its slot and focus ring, gaps and padding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub band: f32,
    pub mark: f32,
    pub slot: f32,
    pub ring: f32,
    pub gap: f32,
    pub pad: f32,
}

impl Sizes {
    pub fn at(face: &Typeface) -> Self {
        let metrics = Metrics::for_typeface(face);
        let mark = (MARK * f32::from(face.body) / 14.).round();
        let slot = HIT_MIN.max(mark);
        // Theme.chromeControlInset; the ring keeps the mark's parity so the mark sits centred in it.
        let inset = (f32::from(face.caption) / 4.)
            .round()
            .max(2. * metrics.hairline);
        let room = slot.min(metrics.header - metrics.hairline - 2. * inset);
        Self {
            band: metrics.header,
            mark,
            slot,
            ring: room - (room - mark).rem_euclid(2.),
            gap: metrics.gap,
            pad: metrics.pad,
        }
    }

    /// Keep the mark's parity so centring never puts an odd-sized mark half a pixel high.
    pub fn inner(&self) -> f32 {
        self.band - (self.band - self.mark).rem_euclid(2.)
    }
}

#[test]
fn visual_geometry_toolbar_marks_keep_the_board_origin_at_14_and_20() {
    for (base, band, mark, top) in [(14, 27., 16., 5.), (20, 37., 23., 7.)] {
        let sizes = Sizes::at(&Typeface::omarchy_default().at_base(base));
        assert_eq!((sizes.band, sizes.mark), (band, mark));
        assert_eq!((sizes.inner() - sizes.mark) / 2., top);
    }
}

#[derive(Default)]
pub struct Frame {
    pub bounds: Option<Bounds<Pixels>>,
    pub controls: [Control; 11],
    /// The pointer is over the path's own slot, which is what shows the copy mark.
    pub address_hovered: bool,
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
    pub focus: [FocusHandle; 11],
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
}

impl Render for ChromeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::window::glass::producers::chrome_theme(window, cx);
        let palette = Palette::inherited(&window.text_style(), theme);
        for control in &mut self.frame.borrow_mut().controls {
            if control.visible {
                control.bounds = None;
            }
            control.visible = false;
            control.reachable = false;
            control.enabled = false;
        }
        for (index, focus) in self.focus.iter().enumerate() {
            self.frame.borrow_mut().controls[index].focused = focus.is_focused(window);
        }
        let sizes = Sizes::at(&Typeface::for_window(window, cx));
        let ink = |enabled| resting_ink(enabled, true, &palette);
        let mut right = div()
            .h_full()
            .flex()
            .items_center()
            .gap(px(sizes.gap))
            .flex_none();
        if self.inputs.view == View::Grid {
            right = right
                .child(self.button(
                    sizes,
                    3,
                    self.inputs.filter,
                    ink(self.inputs.filter),
                    palette.foreground,
                    cx,
                ))
                .child(self.button(
                    sizes,
                    4,
                    self.inputs.sort,
                    ink(self.inputs.sort),
                    palette.foreground,
                    cx,
                ));
        } else {
            right = right.child(self.button(
                sizes,
                2,
                true,
                palette.foreground,
                palette.foreground,
                cx,
            ));
        }
        right = right.child(divider(palette.foreground, sizes.mark));
        for (index, mode) in [
            (5, View::List),
            (6, View::Columns),
            (7, View::Grid),
            (8, View::Dual),
        ] {
            right = right.child(self.button(
                sizes,
                index,
                true,
                resting_ink(true, self.inputs.view == mode, &palette),
                palette.foreground,
                cx,
            ));
        }
        right = right
            .child(divider(palette.foreground, sizes.mark))
            .child(self.button(sizes, 9, true, palette.foreground, palette.foreground, cx));
        let frame = self.frame.clone();
        let pointer_frame = self.frame.clone();
        let toolbar = div()
            .size_full()
            .relative()
            .child(crate::window::glass::producers::backdrop(
                crate::window::glass::producers::Producer::Chrome,
                theme.chrome(),
            ))
            .child(
                div()
                    .size_full()
                    .flex()
                    .gap(px(sizes.gap))
                    .px(px(sizes.pad))
                    .child(
                        div()
                            .flex()
                            .gap(px(sizes.gap))
                            .flex_none()
                            .child(self.button(
                                sizes,
                                0,
                                self.inputs.back,
                                ink(self.inputs.back),
                                palette.foreground,
                                cx,
                            ))
                            .child(self.button(
                                sizes,
                                1,
                                self.inputs.up,
                                ink(self.inputs.up),
                                palette.foreground,
                                cx,
                            )),
                    )
                    .child(self.address(sizes, &palette, cx))
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
