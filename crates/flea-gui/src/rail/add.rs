//! The NETWORK heading plus: its press opens Add a Network Share through the rows model.
use super::*;

impl RailElement {
    /// The plus answers a press on a box at least the hit minimum, centred on its ink, and the row under it never sees that press.
    pub(super) fn listen_add(&self, add: Rect, window: &mut Window) {
        let hit = add_hit(add);
        let model = Rc::clone(&self.model);
        let producer = self.producer.clone();
        window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _, cx| {
            let at = crate::rail::geometry::Point {
                x: f32::from(event.position.x),
                y: f32::from(event.position.y),
            };
            if phase != gpui::DispatchPhase::Bubble
                || event.button != gpui::MouseButton::Left
                || !hit.contains(at)
            {
                return;
            }
            model.borrow_mut().add_presses += 1;
            producer.update(cx, |_, cx| cx.notify());
            cx.stop_propagation();
        });
    }
}

/// The plus's press box: at least the hit minimum, centred on its painted ink.
pub fn add_hit(add: Rect) -> Rect {
    let side = add.width.max(HIT_MIN);
    Rect::new(
        add.x + (add.width - side) / 2.,
        add.y + (add.height - side) / 2.,
        side,
        side,
    )
}

/// Theme.hitMin, the smallest box a pointer target answers on.
pub(crate) const HIT_MIN: f32 = 24.;
