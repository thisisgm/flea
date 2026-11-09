//! The column's reserved lane stays empty until movement, hover or a drag reveals its knob.
use crate::{
    columns::{geometry::Slot, TrackRows, TrackView},
    input::{scroll::Bar, Frame, Painted},
    state::Shell,
};
use gpui::{
    canvas, fill, point, prelude::*, px, size, Bounds, DispatchPhase, Entity, HitboxBehavior,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent,
};
use std::{rc::Rc, time::Duration};

pub(super) fn surface(
    shell: Entity<Shell>,
    side: usize,
    slot: Slot,
    tracks: Rc<[TrackView]>,
) -> impl IntoElement {
    let weak = shell.downgrade();
    let paint = weak.clone();
    canvas(
        move |bounds, window, cx| {
            let ready = weak
                .update(cx, |shell, cx| {
                    let host = shell.panes[side].as_mut()?;
                    let active_row = host.columns_row_height();
                    let columns = host.columns.as_mut()?;
                    if !Rc::ptr_eq(columns.area.tracks(), &tracks) {
                        return None;
                    }
                    let view = tracks.iter().find(|view| view.slot == slot)?;
                    let total = match &view.rows {
                        TrackRows::Listing => host
                            .filter
                            .shown
                            .as_ref()
                            .map_or(host.pane.held.total, |rows| rows.len()),
                        TrackRows::Peek(rows) => rows.len(),
                        _ => return None,
                    };
                    // Only the active track takes the refused rename's band, as its rows do.
                    let grow = if slot == Slot::Active {
                        shell.grows[side]
                    } else {
                        crate::list::pane::Grow::default()
                    };
                    let row = if slot == Slot::Active {
                        active_row
                    } else {
                        columns.area.row()
                    };
                    let scroll = if slot == Slot::Active {
                        host.pane.scroll_y
                    } else {
                        columns.area.scroll(slot)
                    };
                    // ColumnPane.qml gives every column its footer band, so a long one scrolled to its end keeps a blank press target.
                    let content = if slot == Slot::Active {
                        crate::columns::geometry::active_content(
                            grow,
                            total,
                            row,
                            &host.list.borrow().typeface,
                        )
                    } else {
                        grow.content(total, row) + crate::columns::geometry::FOOTER
                    };
                    if slot == Slot::Active {
                        host.pane.scroll_extent = Some(content);
                    }
                    let lane = crate::columns::geometry::lane(&host.list.borrow().typeface);
                    let state = &mut columns.scroll[slot.index()];
                    let was_grabbed = state.input.grab.is_some();
                    let snapshot = super::scroll::snapshot(
                        &view.rows,
                        host.filter.shown.as_ref().map(|rows| rows.as_ptr()),
                    );
                    state.prepare(
                        &view.path,
                        Frame {
                            bounds,
                            row,
                            lane,
                            content,
                            scroll,
                            listing: if slot == Slot::Active {
                                host.pane.held.listing
                            } else {
                                0
                            },
                            total,
                            columns: 1,
                            cell_width: f32::from(bounds.size.width) - lane,
                            cell_origin: (0., 0.),
                            grow,
                        },
                        snapshot,
                    );
                    let released = was_grabbed && state.input.grab.is_none();
                    if released {
                        shell.hold_pointer(side, false, window, cx)
                    }
                    shell.arm_columns_bar_timer(side, slot, window, cx);
                    Some(())
                })
                .ok()
                .flatten()
                .is_some();
            (window.insert_hitbox(bounds, HitboxBehavior::Normal), ready)
        },
        move |bounds, (hit, ready), window, cx| {
            if !ready {
                return;
            }
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            let foreground = theme.foreground;
            let _ = paint.update(cx, |shell, cx| {
                let corner = shell.corner;
                let state = &mut shell.panes[side].as_mut()?.columns.as_mut()?.scroll[slot.index()];
                let frame = state.input.frame?;
                state.input.in_lane =
                    hit.is_hovered(window) && frame.in_lane(window.mouse_position());
                let bar = Bar::new(frame);
                let wide = state.input.in_lane || state.input.grab.is_some();
                let ratio = frame.lane / 14.;
                let width = ((if wide { 10. } else { 6. }) * ratio).round();
                let lane = Bounds::new(
                    point(bounds.right() - px(frame.lane), bounds.top()),
                    size(px(frame.lane), bounds.size.height),
                );
                let knob = Bounds::new(
                    point(
                        bounds.right() - px((2. * ratio).round() + width),
                        bounds.top() + px(bar.offset),
                    ),
                    size(px(width), px(bar.handle)),
                );
                let now = cx.background_executor().now();
                let opacity = state.input.opacity(now, cx.reduce_motion());
                if opacity > 0. && frame.max_scroll() > 0.5 {
                    if wide {
                        window.paint_quad(fill(lane, foreground.opacity(0.05 * opacity)))
                    }
                    let alpha = if state.input.grab.is_some() {
                        0.7
                    } else if wide {
                        0.6
                    } else {
                        0.5
                    };
                    let mut quad = fill(knob, foreground.opacity(alpha * opacity));
                    quad.corner_radii =
                        gpui::Corners::all(px(corner.min(width.min(bar.handle) / 2.).max(0.)));
                    window.paint_quad(quad);
                }
                state.input.drawn = Some(Painted {
                    frame,
                    lane,
                    knob,
                    visible: frame.max_scroll() > 0.5,
                    shown: state.input.shown(now),
                    fill: foreground.opacity(crate::list::TRACK),
                    selected_fill: foreground.opacity(crate::list::GREY_CURSOR),
                });
                if state.input.moving() || (opacity > 0. && opacity < 1.) {
                    window.request_animation_frame()
                }
                Some(())
            });
            let (down, inside) = (paint.clone(), hit.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && inside.is_hovered(window) {
                    let _ = down.update(cx, |shell, cx| {
                        shell.columns_bar_down(side, slot, event, window, cx)
                    });
                }
            });
            let (moving, inside) = (paint.clone(), hit.clone());
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = moving.update(cx, |shell, cx| {
                        shell.columns_bar_move(
                            side,
                            slot,
                            event,
                            inside.is_hovered(window),
                            window,
                            cx,
                        )
                    });
                }
            });
            let up = paint.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = up.update(cx, |shell, cx| {
                        shell.columns_bar_up(side, slot, event, window, cx)
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
    fn arm_columns_bar_timer(
        &mut self,
        side: usize,
        slot: Slot,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = self.panes[side]
            .as_mut()
            .and_then(|h| h.columns.as_mut())
            .map(|c| &mut c.scroll[slot.index()])
        else {
            return;
        };
        let Some(at) = state.input.moved_at else {
            return;
        };
        let elapsed = cx.background_executor().now().saturating_duration_since(at);
        if elapsed >= Duration::from_millis(1000) || state.input.timer.is_some() {
            return;
        }
        let delay = Duration::from_millis(1000) - elapsed;
        state.input.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |shell, _, cx| {
                if let Some(columns) = shell.panes[side].as_mut().and_then(|h| h.columns.as_mut()) {
                    columns.scroll[slot.index()].input.timer = None;
                }
                cx.notify();
            });
        }));
    }
}
