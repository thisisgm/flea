//! The step's current mouse gesture, released once and cancelled independently of its paint state.
use super::draw_flash::{CancelTap, FlashTaps};
use gpui::{
    prelude::*, App, Bounds, Context, Div, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Stateful, Subscription, Window,
};
use std::{cell::RefCell, rc::Rc};

type OwnerSlot = Rc<RefCell<Option<Entity<Gesture>>>>;

struct Gesture {
    down: Option<MouseDownEvent>,
    released: Option<MouseDownEvent>,
    cancel: Option<CancelTap>,
    _activation: Subscription,
    _escape: Subscription,
}

impl Gesture {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |gesture, window, cx| {
            if !window.is_window_active() {
                gesture.cancel(window, cx);
            }
        });
        let handle = window.window_handle();
        let weak = cx.weak_entity();
        let escape = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() == handle && event.keystroke.key == "escape" {
                let _ = weak.update(cx, |gesture: &mut Self, cx| gesture.cancel(window, cx));
            }
        });
        Self {
            down: None,
            released: None,
            cancel: None,
            _activation: activation,
            _escape: escape,
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut App) {
        let down = self.down.take();
        let released = self.released.take();
        let held = down.is_some() || released.is_some();
        if held {
            if let Some(cancel) = &self.cancel {
                cancel(window, cx);
            }
        }
    }
}

pub(super) struct PressButton {
    button: Stateful<Div>,
    owner: OwnerSlot,
    taps: FlashTaps,
    available: bool,
    pressed: bool,
    animating: bool,
}

fn owner(slot: &OwnerSlot) -> Entity<Gesture> {
    slot.borrow()
        .as_ref()
        .expect("the press owner is installed before input")
        .clone()
}

impl PressButton {
    pub(super) fn new(
        mut button: Stateful<Div>,
        taps: &FlashTaps,
        available: bool,
        pressed: bool,
        animating: bool,
    ) -> Self {
        let slot = OwnerSlot::default();
        if let Some(up) = &taps.up {
            let up = up.clone();
            let slot = slot.clone();
            button = button.on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                owner(&slot).update(cx, |gesture, cx| gesture.cancel(window, cx));
                up(event, window, cx);
            });
        }
        if available {
            if let (Some(down), Some(fire)) = (&taps.down, &taps.step) {
                let down = down.clone();
                let fire = fire.clone();
                let down_slot = slot.clone();
                let up_slot = slot.clone();
                button = button
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        if window.captured_hitbox().is_none() {
                            owner(&down_slot)
                                .update(cx, |gesture, _| gesture.down = Some(event.clone()));
                            down(event, window, cx);
                        }
                    })
                    .on_mouse_up(MouseButton::Left, move |event, window, cx| {
                        let began_here = owner(&up_slot)
                            .update(cx, |gesture, _| gesture.released.take().is_some());
                        if began_here {
                            fire(event, window, cx);
                        }
                    });
            }
        }
        Self {
            button,
            owner: slot,
            taps: taps.clone(),
            available,
            pressed,
            animating,
        }
    }
}

impl IntoElement for PressButton {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for PressButton {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = <Stateful<Div> as Element>::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Element::id(&self.button)
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.button.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = window.with_element_state(
            id.expect("the action has an id"),
            |state: Option<Entity<Gesture>>, window| {
                let state = state.unwrap_or_else(|| cx.new(|cx| Gesture::new(window, cx)));
                (state.clone(), state)
            },
        );
        *self.owner.borrow_mut() = Some(state.clone());
        state.update(cx, |gesture, cx| {
            gesture.cancel = self.taps.cancel.clone();
            if !self.available || !self.pressed || window.captured_hitbox().is_some() {
                gesture.cancel(window, cx);
            }
        });
        self.button
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = owner(&self.owner);
        let down_state = state.clone();
        window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
            if phase.capture() {
                down_state.update(cx, |gesture, cx| gesture.cancel(window, cx));
            }
        });
        let up_state = state.clone();
        let up = self.taps.up.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase.capture() {
                up_state.update(cx, |gesture, cx| {
                    if window.captured_hitbox().is_some() {
                        gesture.cancel(window, cx);
                    }
                    let down = gesture.down.take();
                    gesture.released = down.filter(|down| {
                        down.button == MouseButton::Left && event.button == MouseButton::Left
                    });
                    if let Some(up) = &up {
                        up(event, window, cx);
                    }
                });
            }
        });
        let move_state = state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase.capture()
                && (event.pressed_button != Some(MouseButton::Left)
                    || window.captured_hitbox().is_some())
            {
                move_state.update(cx, |gesture, cx| gesture.cancel(window, cx));
            }
        });
        let exit_state = state.clone();
        window.on_mouse_event(move |event: &MouseExitEvent, phase, window, cx| {
            if phase.capture() && event.pressed_button != Some(MouseButton::Left) {
                exit_state.update(cx, |gesture, cx| gesture.cancel(window, cx));
            }
        });
        self.button
            .paint(id, inspector, bounds, layout, prepaint, window, cx);
        if self.animating {
            window.request_animation_frame();
        }
    }
}
