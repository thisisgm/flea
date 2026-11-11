//! Quick Look's ground (#331): one blocking hitbox over the window and a listener that ends every pointer event there.
use crate::leaf::{Leaf, Painted};
use flea::ui_logic_preview_core::ground::{Button, Pointer};
use gpui::{
    fill, relative, size, App, Bounds, DispatchPhase, Hitbox, HitboxBehavior, Hsla, IntoElement,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, Point,
    ScrollDelta, ScrollWheelEvent, Style, Window,
};
use std::rc::Rc;

/// One pointer event the shield ended: what it was, where, whether it fell on the card, and a wheel's travel.
#[derive(Clone, Copy, Debug)]
pub struct Hand {
    pub pointer: Pointer,
    pub at: Point<Pixels>,
    pub inside: bool,
    pub wheel: Option<ScrollDelta>,
    /// A move made with the left button down; a held knob or pan ends on a move without it.
    pub held: bool,
}

/// Called with each event the shield has already consumed.
pub type OnPointer = Rc<dyn Fn(Hand, &mut Window, &mut App)>;

/// Painted first inside the overlay, so the card's own controls answer before it and everything behind it never does.
pub struct Shield {
    card: Bounds<Pixels>,
    ground: Hsla,
    on_pointer: OnPointer,
    dragging: bool,
}

impl Shield {
    /// card is in window coordinates; ground is the dim drawn under the card.
    pub fn new(card: Bounds<Pixels>, ground: Hsla, on_pointer: OnPointer) -> Self {
        Self {
            card,
            ground,
            on_pointer,
            dragging: false,
        }
    }

    /// A held knob or pan keeps the pointer grab, so its moves and release count even outside the window.
    pub fn dragging(mut self, dragging: bool) -> Self {
        self.dragging = dragging;
        self
    }
}

fn button(button: MouseButton) -> Button {
    match button {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
        MouseButton::Navigate(_) => Button::Other,
    }
}

impl IntoElement for Shield {
    type Element = Painted<Self>;
    fn into_element(self) -> Self::Element {
        Painted(self)
    }
}

impl Leaf for Shield {
    type Prepaint = Hitbox;

    fn layout(&mut self, window: &mut Window, cx: &mut App) -> LayoutId {
        let mut style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        style.position = gpui::Position::Absolute;
        window.request_layout(style, None, cx)
    }

    fn prepaint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) -> Hitbox {
        // BlockMouse: every hitbox behind reads unhovered, so no hover, click, tooltip or wheel handler behind it fires.
        window.insert_hitbox(bounds, HitboxBehavior::BlockMouse)
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, _: &mut Hitbox, window: &mut Window, _: &mut App) {
        window.paint_quad(fill(bounds, self.ground));
        let (card, dragging) = (self.card, self.dragging);
        // Bubble runs front to back, so a listener here ends the event after the card's controls and before anything behind.
        macro_rules! end {
            ($event:ty, $grabbed:expr, $pointer:expr) => {{
                let on_pointer = Rc::clone(&self.on_pointer);
                window.on_mouse_event(move |event: &$event, phase, window, cx| {
                    let grabbed = dragging && $grabbed;
                    if phase != DispatchPhase::Bubble
                        || !(grabbed || bounds.contains(&event.position))
                    {
                        return;
                    }
                    cx.stop_propagation();
                    let read: fn(&$event) -> (Pointer, Option<ScrollDelta>, bool) = $pointer;
                    let (pointer, wheel, held) = read(event);
                    let hand = Hand {
                        pointer,
                        at: event.position,
                        inside: card.contains(&event.position),
                        wheel,
                        held,
                    };
                    on_pointer(hand, window, cx);
                });
            }};
        }
        end!(MouseDownEvent, false, |e| (
            Pointer::Press {
                button: button(e.button),
                clicks: e.click_count
            },
            None,
            false
        ));
        end!(MouseUpEvent, true, |e| (
            Pointer::Release {
                button: button(e.button)
            },
            None,
            false
        ));
        end!(MouseMoveEvent, true, |e| (
            Pointer::Move,
            None,
            e.pressed_button == Some(MouseButton::Left)
        ));
        end!(ScrollWheelEvent, false, |e| (
            Pointer::Wheel,
            Some(e.delta),
            false
        ));
        end!(PinchEvent, false, |_| (Pointer::Pinch, None, false));
    }
}
