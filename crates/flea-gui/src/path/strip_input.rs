//! Pointer input for chrome and pane breadcrumbs.
use super::{
    paint::{CrumbElement, Place},
    strip::{Hit, Tap},
    tap::{within_chrome_threshold, within_threshold},
    view::{Out, PathView},
};
use crate::window::input::modifiers;
use gpui::{
    canvas, div, prelude::*, Context, DispatchPhase, Entity, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
};

pub(super) struct Press {
    slot: usize,
    origin: Point<Pixels>,
    revision: u64,
}

impl Press {
    /// The chrome shares the window move's threshold; pane strips keep the ordinary tap slop.
    fn holds(&self, position: Point<Pixels>) -> bool {
        if self.slot == 0 {
            within_chrome_threshold(self.origin, position)
        } else {
            within_threshold(Some(self.origin), position)
        }
    }
}

pub(super) fn strip(
    view: Entity<PathView>,
    place: Place,
    cx: &mut Context<PathView>,
) -> impl IntoElement {
    let slot = place.slot();
    let origin = move |view: &PathView, x: Pixels| {
        let bounds = view.paint.borrow().strips[slot];
        let pad = super::paint::strip_pad(place != Place::Chrome, &view.paint.borrow().scale());
        bounds.map(|bounds| f32::from(x - bounds.origin.x) - pad)
    };
    let watch = view.clone();
    div()
        .id(("path-strip", slot))
        .size_full()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                let Some(x) = origin(view, event.position.x) else {
                    return;
                };
                view.press_strip(slot, x, event, cx);
                cx.stop_propagation();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            // The release propagates so an ancestor's crumb drop still lands.
            cx.listener(move |view, event: &MouseUpEvent, window, cx| {
                view.release_strip(slot, event.position, window, cx);
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |view, _, _, _| {
                if view
                    .strip_press
                    .as_ref()
                    .is_some_and(|press| press.slot == slot)
                {
                    view.strip_press = None;
                }
            }),
        )
        .on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
            let x = origin(view, event.position.x);
            view.hover_strip(slot, x, cx);
        }))
        .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
            if !hovered {
                view.hover_strip(slot, None, cx);
            }
        }))
        .child(CrumbElement::new(view, place))
        .child(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let down = watch.clone();
                    window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            down.update(cx, |view, _| view.strip_press = None);
                        }
                    });
                    let drag = watch.clone();
                    // Observe outside the strip too, so returning after a drag cannot revive its tap.
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            drag.update(cx, |view, _| view.drag_strip(event));
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        )
}

impl PathView {
    /// Parents open on press; inert ground waits for a release the window move has not taken.
    fn press_strip(&mut self, slot: usize, x: f32, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.strip_press = None;
        if self.inputs.blocked {
            return;
        }
        let side = slot.saturating_sub(1).min(1);
        if slot > 0 {
            self.inputs.side = side;
            cx.emit(Out::Focus(side));
        }
        let hit = self.strips[slot].hit(x);
        match self.strips[slot].press(hit, event.click_count, modifiers(event.modifiers)) {
            Tap::Open(path) => cx.emit(Out::Open {
                side: if slot == 0 { self.inputs.side } else { side },
                path,
            }),
            Tap::Edit => {
                self.strip_press = Some(Press {
                    slot,
                    origin: event.position,
                    revision: self.strips[slot].revision(),
                });
            }
            Tap::Nothing => {}
        }
    }

    fn drag_strip(&mut self, event: &MouseMoveEvent) {
        if self.strip_press.as_ref().is_some_and(|press| {
            event.pressed_button != Some(MouseButton::Left)
                || !press.holds(event.position)
                || !self.paint.borrow().strips[press.slot]
                    .is_some_and(|bounds| bounds.contains(&event.position))
        }) {
            self.strip_press = None;
        }
    }

    fn release_strip(
        &mut self,
        slot: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(press) = self.strip_press.take() else {
            return;
        };
        if !self.inputs.blocked
            && press.slot == slot
            && press.revision == self.strips[slot].revision()
            && press.holds(position)
        {
            self.open(window, cx);
            self.tapped = self.entry.editing;
        }
    }

    /// What a press at position asks of strip slot, for a host that captures the press before the strip sees it.
    pub fn tap_at(&mut self, slot: usize, event: &MouseDownEvent) -> Tap {
        let Some(bounds) = self.paint.borrow().strips[slot] else {
            return Tap::Nothing;
        };
        if !bounds.contains(&event.position) {
            return Tap::Nothing;
        }
        let pad = super::paint::strip_pad(slot > 0, &self.paint.borrow().scale());
        let hit = self.strips[slot].hit(f32::from(event.position.x - bounds.origin.x) - pad);
        self.strips[slot].press(hit, event.click_count, modifiers(event.modifiers))
    }

    pub fn hover_strip(&mut self, slot: usize, x: Option<f32>, cx: &mut Context<Self>) {
        let hit: Option<Hit> = x.map(|x| self.strips[slot].hit(x));
        if self.strips[slot].hover(hit) {
            cx.notify();
        }
    }
}
