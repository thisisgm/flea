//! The mounted preview hairline lends pointer input to the area's retained resize owner.
use crate::{
    columns::{edge::EdgeElement, preview_host::Edge},
    state::Shell,
};
use flea::jsondoc::Json;
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, Context, DispatchPhase, HitboxBehavior,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window,
};

pub(super) fn surface(shell: &Shell, side: usize, cx: &Context<Shell>) -> Option<AnyElement> {
    let columns = shell.panes[side].as_ref()?.columns.as_ref()?;
    if !columns.area.host.third_shown(columns.area.settings) {
        return None;
    }
    let (left, width) = Edge::zone(&columns.area.layout);
    let entity = cx.entity().downgrade();
    let paint = columns.area.edge_paint();
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(EdgeElement::new(paint))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let zone = gpui::Bounds::new(
                            bounds.origin + point(px(left), px(0.)),
                            size(px(width), bounds.size.height),
                        );
                        (window.insert_hitbox(zone, HitboxBehavior::Normal), zone)
                    },
                    move |bounds, (hit, zone), window, _| {
                        window.set_cursor_style(gpui::CursorStyle::ResizeLeftRight, &hit);
                        let down = entity.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture
                                || event.button != MouseButton::Left
                                || !hit.is_hovered(window)
                            {
                                return;
                            }
                            let _ = down.update(cx, |shell, cx| {
                                if shell.input_blocked(cx) {
                                    return;
                                }
                                let Some(columns) = shell.panes[side]
                                    .as_mut()
                                    .and_then(|host| host.columns.as_mut())
                                else {
                                    return;
                                };
                                if event.click_count == 2 {
                                    columns.area.edge_restore();
                                } else {
                                    columns
                                        .area
                                        .edge_press(f32::from(event.position.x - bounds.left()));
                                }
                                shell.persist_columns_edge(side, window, cx);
                                cx.stop_propagation();
                                cx.notify();
                            });
                        });
                        let moved = entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let _ = moved.update(cx, |shell, cx| {
                                let blocked = shell.input_blocked(cx);
                                let Some(columns) = shell.panes[side]
                                    .as_mut()
                                    .and_then(|host| host.columns.as_mut())
                                else {
                                    return;
                                };
                                let area = &mut columns.area;
                                let old = (area.host.edge.hovered, area.host.edge.last);
                                area.host.edge.hovered = !blocked && zone.contains(&event.position);
                                if area.host.edge.dragging {
                                    if blocked || event.pressed_button != Some(MouseButton::Left) {
                                        area.edge_release();
                                    } else {
                                        area.edge_move(f32::from(event.position.x - bounds.left()));
                                    }
                                    cx.stop_propagation();
                                }
                                if old != (area.host.edge.hovered, area.host.edge.last) {
                                    cx.notify();
                                }
                                shell.persist_columns_edge(side, window, cx);
                            });
                        });
                        let up = entity.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture || event.button != MouseButton::Left
                            {
                                return;
                            }
                            let _ = up.update(cx, |shell, cx| {
                                let Some(columns) = shell.panes[side]
                                    .as_mut()
                                    .and_then(|host| host.columns.as_mut())
                                else {
                                    return;
                                };
                                if columns.area.host.edge.dragging {
                                    columns.area.edge_release();
                                    shell.persist_columns_edge(side, window, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element(),
    )
}

impl Shell {
    pub(super) fn persist_columns_edge(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stored = self.panes[side]
            .as_mut()
            .and_then(|host| host.columns.as_mut())
            .and_then(|columns| std::mem::take(&mut columns.area.stored).last().copied());
        let Some(width) = stored else {
            return;
        };
        if crate::window::selection_preview::host::share(&self.launch.settings.data) == width as f32
        {
            return;
        }
        let preview = Json::Obj(vec![("width".into(), Json::Num(width.to_string()))]);
        self.change_settings(Json::Obj(vec![("preview".into(), preview)]), window, cx);
    }
}
