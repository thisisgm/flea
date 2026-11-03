//! One viewport overlay paints the scroller and band and installs native hit-tested input.
use super::{scroll::Bar, Frame};
use crate::state::Shell;
use gpui::{
    canvas, fill, point, prelude::*, px, size, Bounds, DispatchPhase, Entity, HitboxBehavior,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent,
};

#[derive(Clone, Copy)]
pub struct Painted {
    pub frame: Frame,
    pub lane: Bounds<gpui::Pixels>,
    pub knob: Bounds<gpui::Pixels>,
    pub visible: bool,
    pub shown: bool,
    pub fill: gpui::Hsla,
    /// The grey selected fill, the token the other pane's cursor and the rail's lit section paint; selectedFill names it.
    pub selected_fill: gpui::Hsla,
}

pub fn surface(shell: Entity<Shell>, side: usize) -> impl IntoElement {
    surface_with(shell, side, move |shell, bounds| {
        let grow = shell.grows[side];
        let host = shell.panes[side].as_mut()?;
        host.input.hit_boxes = None;
        let list = host.list.borrow();
        Some((
            Frame::list(bounds, &list.drawn.metrics, &host.pane, grow),
            list.drawn.palette?,
        ))
    })
}

/// After PaneInput::prepare, publish the mounted view geometry and palette from its own prepaint.
pub fn surface_with(
    shell: Entity<Shell>,
    side: usize,
    read: impl Fn(&mut Shell, Bounds<gpui::Pixels>) -> Option<(Frame, crate::list::Palette)> + 'static,
) -> impl IntoElement {
    viewport(shell, side, false, read)
}

/// Grid borrows the same scroller paint and deadlines while retaining its own scroll position.
pub(crate) fn grid_surface(
    shell: Entity<Shell>,
    side: usize,
    read: impl Fn(&mut Shell, Bounds<gpui::Pixels>) -> Option<(Frame, crate::list::Palette)> + 'static,
) -> impl IntoElement {
    viewport(shell, side, true, read)
}

fn viewport(
    shell: Entity<Shell>,
    side: usize,
    grid: bool,
    read: impl Fn(&mut Shell, Bounds<gpui::Pixels>) -> Option<(Frame, crate::list::Palette)> + 'static,
) -> impl IntoElement {
    let shell = shell.downgrade();
    let paint = shell.clone();
    canvas(
        move |bounds, window, cx| {
            let drawn = shell
                .update(cx, |shell, cx| {
                    let facts = read(shell, bounds);
                    let host = shell.panes[side].as_mut()?;
                    let Some((frame, palette)) = facts else {
                        if grid {
                            host.input.cancel(&mut host.selected, &mut host.pane.cursor);
                            host.input.frame = None;
                            host.input.drawn = None;
                            shell.hold_pointer(side, false, window, cx);
                        }
                        return None;
                    };
                    if grid {
                        host.input
                            .prepare(frame, &mut host.selected, &mut host.pane.cursor);
                        if host.input.frame.is_some_and(|old| {
                            old.listing == frame.listing
                                && old.total == frame.total
                                && old.scroll != frame.scroll
                        }) {
                            host.input.moved(cx.background_executor().now());
                        }
                    } else {
                        host.pane.scroll_extent = Some(frame.content);
                    }
                    host.input.frame = Some(frame);
                    if grid {
                        shell.arm_input_tick(side, window, cx);
                    } else {
                        shell.advance_row_drag(side, frame, window, cx);
                    }
                    Some((frame, palette))
                })
                .ok()
                .flatten();
            (window.insert_hitbox(bounds, HitboxBehavior::Normal), drawn)
        },
        move |bounds, (hitbox, drawn), window, cx| {
            let Some((frame, palette)) = drawn else {
                return;
            };
            let _ = paint.update(cx, |shell, cx| {
                let host = shell.panes[side].as_mut().expect("mounted pane");
                host.input.in_lane =
                    hitbox.is_hovered(window) && frame.in_lane(window.mouse_position());
                let bar = Bar::new(frame);
                let ratio = frame.lane / 14.;
                let wide = host.input.in_lane || host.input.grab.is_some();
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
                let shown = host.input.shown(cx.background_executor().now());
                let opacity = host
                    .input
                    .opacity(cx.background_executor().now(), cx.reduce_motion());
                // Columns paints each track's bar from its own scroll state.
                let columns = !grid && shell.view == crate::window::startup::View::Columns;
                if opacity > 0. && frame.max_scroll() > 0.5 && !columns {
                    if wide {
                        window.paint_quad(fill(lane, palette.foreground.opacity(0.05 * opacity)));
                    }
                    let alpha = if host.input.grab.is_some() {
                        0.7
                    } else if wide {
                        0.6
                    } else {
                        0.5
                    };
                    let mut quad = fill(knob, palette.foreground.opacity(alpha * opacity));
                    quad.corner_radii = gpui::Corners::all(px(shell
                        .corner
                        .min(width.min(bar.handle) / 2.)
                        .max(0.)));
                    window.paint_quad(quad);
                }
                if let Some(band) = &host.input.band.as_ref().filter(|band| band.active) {
                    let (x1, y1) = band.anchor;
                    let (x2, y2) = band.end;
                    let rect = Bounds::new(
                        point(
                            bounds.left() + px(x1.min(x2)),
                            bounds.top() + px(y1.min(y2) - frame.scroll),
                        ),
                        size(px((x1 - x2).abs()), px((y1 - y2).abs())),
                    );
                    window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                        window.paint_quad(gpui::quad(
                            rect,
                            gpui::Corners::default(),
                            palette.accent.opacity(palette.marked.a),
                            gpui::Edges::all(px(1.)),
                            palette.accent,
                            gpui::BorderStyle::Solid,
                        ));
                    });
                }
                host.input.drawn = Some(Painted {
                    frame,
                    lane,
                    knob,
                    visible: frame.max_scroll() > 0.5,
                    shown,
                    fill: palette.foreground.opacity(crate::list::TRACK),
                    selected_fill: palette.foreground.opacity(crate::list::GREY_CURSOR),
                });
            });
            let (down, hit) = (paint.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hit.is_hovered(window) {
                    let _ = down.update(cx, |shell, cx| {
                        if grid && frame.in_lane(event.position) {
                            shell.grid_bar_down(side, event, window, cx);
                        } else if !grid
                            && shell.view == crate::window::startup::View::Columns
                            && (event.button == gpui::MouseButton::Right || shell.input_blocked(cx))
                        {
                            shell.columns_pointer(side, event, window, cx);
                        } else if !grid || shell.band_press(side, frame, event, cx) {
                            shell.hold_pointer(side, true, window, cx);
                            shell.pointer_down(side, event, window, cx);
                        }
                    });
                }
            });
            let (moved, hit) = (paint.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = moved.update(cx, |shell, cx| {
                        if event.pressed_button.is_none() {
                            shell.cancel_pointer(side, window, cx);
                        }
                        // A Grid band press hands its moves to the shared band; the lane keeps the bar drag.
                        let band = shell.panes[side]
                            .as_ref()
                            .is_some_and(|host| host.input.band.is_some());
                        if grid && !band {
                            shell.grid_bar_move(side, event, hit.is_hovered(window), window, cx);
                        } else {
                            shell.pointer_move(side, event, hit.is_hovered(window), window, cx);
                        }
                    });
                }
            });
            let up = paint.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    let up = up.clone();
                    // Run after the normal release handler; a consuming overlay still releases the hold.
                    window.defer(cx, move |window, cx| {
                        let _ = up.update(cx, |shell, cx| shell.cancel_pointer(side, window, cx));
                    });
                } else if phase == DispatchPhase::Bubble {
                    let _ = up.update(cx, |shell, cx| {
                        if grid {
                            shell.grid_bar_up(side, event, window, cx);
                        }
                        shell.pointer_up(side, event, window, cx);
                        shell.hold_pointer(side, false, window, cx);
                    });
                }
            });
            let wheel = paint.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if !grid && phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    // Columns takes the wheel once, at its area, for whichever track is under it.
                    let _ = wheel.update(cx, |shell, cx| {
                        if shell.view != crate::window::startup::View::Columns {
                            shell.pointer_wheel(side, event, window, cx);
                        }
                    });
                }
            });
        },
    )
    .absolute()
    .left(px(0.))
    .top(px(0.))
    .size_full()
}
