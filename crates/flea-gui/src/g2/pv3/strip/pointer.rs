//! The strip's pointer: press, drag, release and wheel, routed to the part under them.
use super::{part_at, span_bounds, Part, StripElement, StripInput};
use crate::g2::pv3::ink::size;
use flea::ui_logic_pv3::strip::{self as geometry, Layout};
use gpui::{
    px, Bounds, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    ScrollWheelEvent, Window,
};

impl StripElement {
    pub(super) fn listen(
        &self,
        bounds: Bounds<Pixels>,
        layout: Layout,
        content: Bounds<Pixels>,
        window: &mut Window,
    ) {
        let local = move |x: Pixels| f32::from(x - content.origin.x);
        let (cache, input) = (self.cache.clone(), self.on_input.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !bounds.contains(&event.position)
            {
                return;
            }
            // Transport clicks handle their input without entering the preview's keyboard context.
            window.prevent_default();
            let x = local(event.position.x);
            let part = part_at(&layout, x);
            cache.borrow_mut().pressed = part;
            input(StripInput::Touched, window, cx);
            match part {
                Some(Part::Seek) => input(
                    StripInput::ScrubPress(geometry::fraction_at(x - layout.seek.x, layout.seek.w)),
                    window,
                    cx,
                ),
                Some(Part::Volume) => {
                    let track = layout.volume.unwrap_or(layout.seek);
                    input(
                        StripInput::LevelPress(geometry::level_at(x - track.x, track.w)),
                        window,
                        cx,
                    )
                }
                _ => {}
            }
        });
        let (cache, input) = (self.cache.clone(), self.on_input.clone());
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            let pressed = cache.borrow().pressed;
            let x = local(event.position.x);
            match pressed {
                Some(Part::Seek) => input(
                    StripInput::ScrubMove(geometry::fraction_at(x - layout.seek.x, layout.seek.w)),
                    window,
                    cx,
                ),
                Some(Part::Volume) if event.pressed_button == Some(MouseButton::Left) => {
                    let track = layout.volume.unwrap_or(layout.seek);
                    input(
                        StripInput::LevelMove(geometry::level_at(x - track.x, track.w)),
                        window,
                        cx,
                    )
                }
                _ if bounds.contains(&event.position) => input(StripInput::Touched, window, cx),
                _ => {}
            }
        });
        let (cache, input) = (self.cache.clone(), self.on_input.clone());
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                return;
            }
            let Some(pressed) = cache.borrow_mut().pressed.take() else {
                return;
            };
            let inside = bounds.contains(&event.position);
            let over = part_at(&layout, local(event.position.x)).filter(|_| inside);
            input(StripInput::Touched, window, cx);
            match pressed {
                Part::Seek => input(StripInput::ScrubRelease, window, cx),
                Part::Volume => input(StripInput::LevelRelease, window, cx),
                // A tap counts only when released inside the part it pressed.
                Part::Play if over == Some(Part::Play) => input(StripInput::Toggle, window, cx),
                Part::Mute if over == Some(Part::Mute) => input(StripInput::Mute, window, cx),
                _ => {}
            }
        });
        let input = self.on_input.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || !span_bounds(layout.seek, content).contains(&event.position)
            {
                return;
            }
            let up = f32::from(event.delta.pixel_delta(px(size::STRIP)).y) > 0.;
            input(StripInput::Touched, window, cx);
            input(StripInput::Wheel { up }, window, cx);
        });
    }
}
