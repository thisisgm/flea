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
}

pub fn surface(shell: Entity<Shell>, side: usize) -> impl IntoElement {
    surface_with(shell, side, move |shell, bounds| {
        let host = shell.panes[side].as_mut()?;
        host.input.hit_boxes = None;
        let list = host.list.borrow();
        Some((
            Frame::list(bounds, &list.drawn.metrics, &host.pane),
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
    surface_mode(shell, side, false, read)
}

pub(super) fn surface_mode(
    shell: Entity<Shell>,
    side: usize,
    band_only: bool,
    read: impl Fn(&mut Shell, Bounds<gpui::Pixels>) -> Option<(Frame, crate::list::Palette)> + 'static,
) -> impl IntoElement {
    let shell = shell.downgrade();
    let paint = shell.clone();
    canvas(
        move |bounds, window, cx| {
            let drawn = shell
                .update(cx, |shell, _| {
                    let (frame, palette) = read(shell, bounds)?;
                    let host = shell.panes[side].as_mut()?;
                    if !band_only {
                        host.pane.scroll_extent = Some(frame.content);
                        host.input.frame = Some(frame);
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
                if !band_only && opacity > 0. && frame.max_scroll() > 0.5 {
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
                // Ipc.qml:90 reports Style.selectedFill; Row.qml:127 is the wash the list paints for this state.
                host.input.drawn = Some(Painted {
                    frame,
                    lane,
                    knob,
                    visible: frame.max_scroll() > 0.5,
                    shown,
                    fill: palette.foreground.opacity(crate::list::TRACK),
                });
            });
            let (down, hit) = (paint.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hit.is_hovered(window) {
                    let _ = down.update(cx, |shell, cx| {
                        if band_only {
                            if shell.input_blocked(cx) {
                                return;
                            }
                            let Some(host) = shell.panes[side].as_mut() else {
                                return;
                            };
                            if event.button != gpui::MouseButton::Left
                                || host.stopped
                                || host.pane.busy()
                                || host.input.hit_in(frame, event.position).is_some()
                                || frame.in_lane(event.position)
                                || cx
                                    .try_global::<crate::g2::dg1::carry::Carrying>()
                                    .is_some_and(|carrying| carrying.carry.active())
                            {
                                return;
                            }
                            host.input.frame = Some(frame);
                            host.pane.scroll_extent = Some(frame.content);
                            host.pane.scroll_to(frame.scroll);
                            if let Some(grid) = host.grid.as_mut() {
                                grid.view.land_glide();
                            }
                        }
                        shell.hold_pointer(side, true, window, cx);
                        shell.pointer_down(side, event, window, cx)
                    });
                }
            });
            let (moved, hit) = (paint.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = moved.update(cx, |shell, cx| {
                        if band_only
                            && !shell.panes[side]
                                .as_ref()
                                .is_some_and(|host| host.input.band.is_some())
                        {
                            return;
                        }
                        if event.pressed_button.is_none() {
                            shell.cancel_pointer(side, window, cx);
                        }
                        shell.pointer_move(side, event, hit.is_hovered(window), window, cx)
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
                        if band_only
                            && !shell.panes[side]
                                .as_ref()
                                .is_some_and(|host| host.input.band.is_some())
                        {
                            return;
                        }
                        shell.pointer_up(side, event, window, cx);
                        if band_only {
                            if let Some(host) = shell.panes[side].as_mut() {
                                host.input.frame = None;
                            }
                        }
                        shell.hold_pointer(side, false, window, cx);
                    });
                }
            });
            let wheel = paint.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    if band_only {
                        return;
                    }
                    let _ =
                        wheel.update(cx, |shell, cx| shell.pointer_wheel(side, event, window, cx));
                }
            });
        },
    )
    .absolute()
    .left(px(0.))
    .top(px(0.))
    .size_full()
}
