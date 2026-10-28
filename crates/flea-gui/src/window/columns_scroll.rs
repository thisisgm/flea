//! Each directory column keeps a quiet overlay lane, with the shared viewport knob and hold/fade policy.
use crate::{
    columns::{geometry::Slot, TrackRows},
    input::{scroll::Bar, Frame, PaneInput},
    state::Shell,
};
use gpui::{
    canvas, fill, point, prelude::*, px, size, Bounds, Context, DispatchPhase, Entity,
    HitboxBehavior, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Window,
};
use std::time::Duration;

#[path = "columns_scroll_input.rs"]
mod input;

#[derive(Default)]
pub(super) struct ColumnBar {
    pub input: PaneInput,
    path: String,
}

pub(super) fn surface(
    shell: Entity<Shell>,
    side: usize,
    slot: Slot,
    palette: crate::list::Palette,
) -> impl IntoElement {
    let prepare = shell.downgrade();
    let paint = prepare.clone();
    canvas(
        move |bounds, window, cx| {
            let _ = prepare.update(cx, |shell, cx| {
                shell.column_bar_frame(side, slot, bounds, window, cx)
            });
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        },
        move |bounds, hit, window, cx| {
            let _ = paint.update(cx, |shell, cx| {
                let corner = shell.corner;
                let Some(bar) = shell.panes[side]
                    .as_mut()
                    .and_then(|h| h.columns.as_mut())
                    .map(|c| &mut c.bars[slot.index()])
                else {
                    return;
                };
                let Some(frame) = bar.input.frame else { return };
                let now = cx.background_executor().now();
                bar.input.in_lane =
                    hit.is_hovered(window) && frame.in_lane(window.mouse_position());
                let opacity = bar.input.opacity(now, cx.reduce_motion());
                if bar.input.animating(now, cx.reduce_motion()) {
                    window.request_animation_frame();
                }
                let geometry = Bar::new(frame);
                let wide = bar.input.in_lane || bar.input.grab.is_some();
                let ratio = frame.lane / 14.;
                let width = ((if wide { 10. } else { 6. }) * ratio).round();
                let lane = Bounds::new(
                    point(bounds.right() - px(frame.lane), bounds.top()),
                    size(px(frame.lane), bounds.size.height),
                );
                if wide && opacity > 0. && frame.max_scroll() > 0.5 {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.right() - px(frame.lane), bounds.top()),
                            size(px(frame.lane), bounds.size.height),
                        ),
                        palette.foreground.opacity(0.05 * opacity),
                    ));
                }
                let knob = Bounds::new(
                    point(
                        bounds.right() - px((2. * ratio).round() + width),
                        bounds.top() + px(geometry.offset),
                    ),
                    size(px(width), px(geometry.handle)),
                );
                bar.input.drawn = Some(crate::input::Painted {
                    frame,
                    lane,
                    knob,
                    visible: frame.max_scroll() > 0.5,
                    shown: bar.input.shown(now),
                    fill: palette.foreground.opacity(crate::list::TRACK),
                });
                if opacity <= 0. || frame.max_scroll() <= 0.5 {
                    return;
                }
                let alpha = if bar.input.grab.is_some() {
                    0.7
                } else if wide {
                    0.6
                } else {
                    0.5
                };
                let mut quad = fill(knob, palette.foreground.opacity(alpha * opacity));
                quad.corner_radii =
                    gpui::Corners::all(px(corner.min(width.min(geometry.handle) / 2.).max(0.)));
                window.paint_quad(quad);
            });
            let down = paint.clone();
            let hovered = hit.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hovered.is_hovered(window) {
                    let _ = down.update(cx, |shell, cx| {
                        shell.column_bar_down(side, slot, event, window, cx)
                    });
                }
            });
            let moved = paint.clone();
            let hovered = hit.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = moved.update(cx, |shell, cx| {
                        shell.column_bar_move(
                            side,
                            slot,
                            event,
                            hovered.is_hovered(window),
                            window,
                            cx,
                        )
                    });
                }
            });
            let up = paint.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    let up = up.clone();
                    window.defer(cx, move |window, cx| {
                        let _ =
                            up.update(cx, |shell, cx| shell.column_bar_up(side, slot, window, cx));
                    });
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

impl Shell {
    fn column_bar_frame(
        &mut self,
        side: usize,
        slot: Slot,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(columns) = host.columns.as_mut() else {
            return;
        };
        let Some(paint) = columns.painters.get(slot) else {
            return;
        };
        let paint = paint.borrow();
        let Some(drawn) = &paint.drawn else { return };
        let total = match &drawn.view().rows {
            TrackRows::Listing => host.filter.shown_total(host.pane.held.total),
            TrackRows::Peek(rows) => rows.len(),
            _ => 0,
        };
        let lane = crate::columns::geometry::LANE;
        let frame = Frame {
            bounds,
            row: crate::list::pane::ROW_HEIGHT,
            lane,
            content: total as f32 * crate::list::pane::ROW_HEIGHT,
            scroll: drawn.scroll,
            listing: host.pane.held.listing,
            total,
            columns: 1,
            cell_width: f32::from(bounds.size.width) - lane,
        };
        let bar = &mut columns.bars[slot.index()];
        let old = bar.input.frame;
        let unavailable = drawn.view().state != crate::columns::TrackState::Rows;
        let settling = unavailable
            || bar.path != drawn.view().path
            || old.is_none_or(|old| {
                old.listing != frame.listing || old.total != frame.total || old.bounds != bounds
            });
        let release = settling && bar.input.grab.is_some();
        if settling {
            bar.input = PaneInput::default();
        }
        let moved = !settling && old.is_some_and(|old| old.scroll != frame.scroll);
        bar.path.clone_from(&drawn.view().path);
        bar.input.frame = (!unavailable).then_some(frame);
        drop(paint);
        if release {
            self.hold_pointer(side, false, window, cx);
        }
        if moved {
            self.column_bar_moved(side, slot, cx);
        }
    }

    fn column_bar_moved(&mut self, side: usize, slot: Slot, cx: &mut Context<Self>) {
        let Some(bar) = self.panes[side]
            .as_mut()
            .and_then(|h| h.columns.as_mut())
            .map(|c| &mut c.bars[slot.index()])
        else {
            return;
        };
        bar.input.moved(cx.background_executor().now());
        bar.input.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1000))
                .await;
            let _ = this.update(cx, |shell, cx| {
                if let Some(columns) = shell.panes[side].as_mut().and_then(|h| h.columns.as_mut()) {
                    columns.bars[slot.index()].input.timer = None;
                }
                cx.notify();
            });
        }));
    }
}
