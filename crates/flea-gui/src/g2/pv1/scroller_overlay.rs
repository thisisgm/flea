//! The document views share the viewport lane geometry and pointer policy with Quick Look.
use super::{index, Axis, Scroll, View, AXES};
use flea::ui_logic_pv1::scroll;
use gpui::{
    canvas, fill, point, prelude::*, px, size, Bounds, Context, DispatchPhase, Entity,
    HitboxBehavior, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct Overlay {
    scroll: Scroll,
    moved: [Option<Instant>; 2],
    timers: [Option<gpui::Task<()>>; 2],
}

impl Overlay {
    pub(crate) fn vertical_lane(&self) -> Option<super::capture::Lane> {
        self.scroll.vertical_lane(false)
    }
}

#[cfg(test)]
impl Overlay {
    // The vertical bar shows: the view moved within the last HOLD_MS.
    pub(crate) fn vertical_shown(&self) -> bool {
        self.scroll.moving[0]
    }
}

pub(crate) trait Owner: Sized + 'static {
    fn overlay(&mut self) -> &mut Overlay;
    fn viewport(&self, bounds: Bounds<Pixels>) -> Option<(View, (f64, f64))>;
    fn position(&mut self, position: (f64, f64), cx: &mut Context<Self>);
}

fn reveal<T: Owner>(owner: &mut T, axis: Axis, cx: &mut Context<T>) {
    let i = index(axis);
    let bar = owner.overlay();
    bar.moved[i] = Some(cx.background_executor().now());
    bar.scroll.moving[i] = true;
    bar.timers[i] = Some(cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(scroll::HOLD_MS))
            .await;
        let _ = this.update(cx, |owner, cx| {
            owner.overlay().scroll.moving[i] = false;
            owner.overlay().timers[i] = None;
            cx.notify();
        });
    }));
}

fn prepare<T: Owner>(owner: &mut T, bounds: Bounds<Pixels>, cx: &mut Context<T>) {
    let Some((view, position)) = owner.viewport(bounds) else {
        *owner.overlay() = Overlay::default();
        return;
    };
    let bar = owner.overlay();
    let old = bar.scroll;
    let settling = old
        .view
        .is_none_or(|old| old.bounds != view.bounds || old.sideways != view.sideways);
    if settling {
        *bar = Overlay::default();
    }
    bar.scroll.x = position.0;
    bar.scroll.y = position.1;
    bar.scroll.fit(view);
    let changed = AXES.map(|axis| !settling && old.at(axis) != bar.scroll.at(axis));
    for (axis, changed) in AXES.into_iter().zip(changed) {
        if changed {
            reveal(owner, axis, cx);
        }
    }
}

fn paint(
    bar: &Overlay,
    ink: Hsla,
    corner: f32,
    now: Instant,
    reduced: bool,
    window: &mut gpui::Window,
) {
    let Some(view) = bar.scroll.view else { return };
    for axis in AXES {
        if !view.scrolls(axis) {
            continue;
        }
        let i = index(axis);
        let pressed = bar.scroll.drag.is_some_and(|(a, _)| a == axis);
        let wide = pressed || bar.scroll.lane == Some(axis);
        let opacity = if wide || bar.scroll.moving[i] {
            1.
        } else if reduced {
            0.
        } else {
            bar.moved[i].map_or(0., |at| {
                scroll::fade(now.saturating_duration_since(at).as_millis() as f32)
            })
        };
        if !wide && !bar.scroll.moving[i] && opacity > 0. {
            window.request_animation_frame();
        }
        if opacity <= 0. {
            continue;
        }
        let lane = view.lane(axis);
        let origin = view.bounds.origin + lane.origin;
        let (content, viewport) = view.extent(axis);
        let track = view.track(axis);
        let length = scroll::handle_length(track, content, viewport);
        let offset = scroll::handle_offset(bar.scroll.at(axis), content, viewport, track);
        let width = if wide {
            scroll::KNOB_WIDE_PX
        } else {
            scroll::KNOB_REST_PX
        } as f32;
        let inset = scroll::KNOB_INSET_PX as f32;
        if wide {
            window.paint_quad(fill(
                Bounds::new(origin, lane.size),
                ink.opacity(scroll::TRACK_ALPHA * opacity),
            ));
        }
        let (at, extent) = match axis {
            Axis::Vertical => (
                origin + point(lane.size.width - px(inset + width), px(offset as f32)),
                size(px(width), px(length as f32)),
            ),
            Axis::Horizontal => (
                origin + point(px(offset as f32), lane.size.height - px(inset + width)),
                size(px(length as f32), px(width)),
            ),
        };
        let alpha = if pressed {
            scroll::KNOB_PRESSED_ALPHA
        } else if wide {
            scroll::KNOB_HOVER_ALPHA
        } else {
            scroll::KNOB_ALPHA
        };
        let mut quad = fill(Bounds::new(at, extent), ink.opacity(alpha * opacity));
        quad.corner_radii =
            gpui::Corners::all(px(corner.min(width.min(length as f32) / 2.).max(0.)));
        window.paint_quad(quad);
    }
}

pub(crate) fn surface<T: Owner>(entity: Entity<T>, ink: Hsla, corner: f32) -> impl IntoElement {
    let laid = entity.downgrade();
    let painted = laid.clone();
    canvas(
        move |bounds, window, cx| {
            let _ = laid.update(cx, |owner, cx| prepare(owner, bounds, cx));
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        },
        move |_, hit, window, cx| {
            let _ = painted.update(cx, |owner, cx| {
                let bar = owner.overlay();
                if let Some(view) = bar.scroll.view {
                    let at = window.mouse_position();
                    bar.scroll.lane = AXES.into_iter().find(|&axis| {
                        view.scrolls(axis) && view.lane(axis).contains(&view.local(at))
                    });
                }
                paint(
                    bar,
                    ink,
                    corner,
                    cx.background_executor().now(),
                    cx.reduce_motion(),
                    window,
                );
            });
            let down = painted.clone();
            let hovered = hit.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !hovered.is_hovered(window)
                {
                    return;
                }
                let _ = down.update(cx, |owner, cx| {
                    if let Some(axis) = owner.overlay().scroll.press(event.position) {
                        let bar = owner.overlay();
                        let position = (bar.scroll.x, bar.scroll.y);
                        owner.position(position, cx);
                        reveal(owner, axis, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                });
            });
            let moved = painted.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let _ = moved.update(cx, |owner, cx| {
                    let bar = owner.overlay();
                    let (changed, dragged) = bar.scroll.hover(
                        event.position,
                        event.pressed_button == Some(MouseButton::Left),
                    );
                    let position = (bar.scroll.x, bar.scroll.y);
                    if let Some(axis) = dragged {
                        owner.position(position, cx);
                        reveal(owner, axis, cx);
                        cx.stop_propagation();
                    }
                    if changed {
                        cx.notify();
                    }
                });
            });
            let released = painted.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                    return;
                }
                let released = released.clone();
                window.defer(cx, move |_, cx| {
                    let _ = released.update(cx, |owner, cx| {
                        if owner.overlay().scroll.release() {
                            cx.notify();
                        }
                    });
                });
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
