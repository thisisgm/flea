//! The chrome is the window's title bar: a left press that travels past four pixels moves the window.
use super::composition::Rect;
use crate::state::Shell;
use gpui::{
    canvas, prelude::*, App, Context, DispatchPhase, Entity, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
};

/// The title bar's drag threshold, read on each axis from the press (ui/WindowDrag.qml).
const THRESHOLD: f32 = 4.;

/// One press on the chrome: where it began, and whether the window move already took it.
#[derive(Default)]
pub struct Gesture {
    origin: Option<Point<Pixels>>,
    moved: bool,
}

/// Registers the chrome's press listeners for this frame; gpui takes them only while painting.
pub fn observer(shell: Entity<Shell>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let (down, drag, up) = (shell.clone(), shell.clone(), shell.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    down.update(cx, |shell, cx| shell.chrome_move_down(event, window, cx));
                }
            });
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    drag.update(cx, |shell, cx| shell.chrome_move_drag(event, window, cx));
                }
            });
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    up.update(cx, |shell, _| shell.chrome_move = Gesture::default());
                }
            });
        },
    )
    .absolute()
    .size_full()
}

impl Shell {
    /// A left press on the chrome may move the window only while the path editor is closed and nothing covers the chrome.
    fn chrome_move_down(&mut self, event: &MouseDownEvent, window: &Window, cx: &App) {
        self.chrome_move = Gesture::default();
        if event.button != MouseButton::Left
            || !inside(self.geometry(window).chrome, event.position)
        {
            return;
        }
        let Some(view) = self.chrome.view.as_ref() else {
            return;
        };
        let view = view.read(cx);
        if view.inputs.live && !view.path.read(cx).editing() {
            self.chrome_move.origin = Some(event.position);
        }
    }

    /// Past the threshold the window move takes the press, so every chrome control drops its tap.
    fn chrome_move_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.chrome_move = Gesture::default();
            return;
        }
        let Some(origin) = self.chrome_move.origin else {
            return;
        };
        if self.chrome_move.moved || within_threshold(origin, event.position) {
            return;
        }
        self.chrome_move.moved = true;
        if let Some(view) = self.chrome.view.clone() {
            let frame = view.read(cx).frame.clone();
            let now = cx.background_executor().now();
            let reduced = cx.reduce_motion();
            for control in &mut frame.borrow_mut().controls {
                if control.pointer_down {
                    control.set_pressed(false, now, reduced);
                }
            }
            view.update(cx, |_, cx| cx.notify());
        }
        request_move(window, cx);
    }
}

fn inside(rect: Rect, at: Point<Pixels>) -> bool {
    let (x, y) = (f32::from(at.x), f32::from(at.y));
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

fn within_threshold(origin: Point<Pixels>, position: Point<Pixels>) -> bool {
    f32::from(position.x - origin.x).abs() <= THRESHOLD
        && f32::from(position.y - origin.y).abs() <= THRESHOLD
}

#[cfg(not(test))]
fn request_move(window: &Window, _: &App) {
    window.start_window_move();
}

// The test platform leaves the compositor handoff unimplemented, so tests count the request instead.
#[cfg(test)]
fn request_move(_: &Window, cx: &App) {
    super::chrome_move_tests::record(cx);
}
