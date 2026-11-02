//! The mounted Columns hairlines, every column edge and the preview edge, lend pointer input to the area's resize owner.
use crate::{
    columns::{edge::EdgeElement, geometry::Line, preview_host::Edge},
    state::Shell,
};
use flea::jsondoc::Json;
use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, Context, DispatchPhase, HitboxBehavior,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window,
};

pub(super) fn surface(shell: &Shell, side: usize, cx: &Context<Shell>) -> Option<AnyElement> {
    let columns = shell.panes[side].as_ref()?.columns.as_ref()?;
    let third = columns.area.host.third_shown(columns.area.settings);
    let layout = &columns.area.layout;
    let zones: Vec<(Line, f32, f32)> = layout
        .lines()
        .filter(|&line| third || line != Line::Preview)
        .filter_map(|line| Edge::zone_at(layout, line).map(|(left, width)| (line, left, width)))
        .collect();
    if zones.is_empty() {
        return None;
    }
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
                        zones
                            .iter()
                            .map(|&(line, left, width)| {
                                let zone = gpui::Bounds::new(
                                    bounds.origin + point(px(left), px(0.)),
                                    size(px(width), bounds.size.height),
                                );
                                (
                                    line,
                                    window.insert_hitbox(zone, HitboxBehavior::Normal),
                                    zone,
                                )
                            })
                            .collect::<Vec<_>>()
                    },
                    move |bounds, hits, window, _| {
                        for (_, hit, _) in &hits {
                            window.set_cursor_style(gpui::CursorStyle::ResizeLeftRight, hit);
                        }
                        let hits = std::rc::Rc::new(hits);
                        let down = entity.clone();
                        let pressed = hits.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture || event.button != MouseButton::Left
                            {
                                return;
                            }
                            let Some(&(line, _, _)) =
                                pressed.iter().find(|(_, hit, _)| hit.is_hovered(window))
                            else {
                                return;
                            };
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
                                columns.area.host.edge.at = line;
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
                                let old = (
                                    area.host.edge.hovered,
                                    area.host.edge.at,
                                    area.host.edge.last,
                                );
                                let hot = hits
                                    .iter()
                                    .find(|(_, _, zone)| zone.contains(&event.position))
                                    .map(|&(line, _, _)| line)
                                    .filter(|_| !blocked);
                                area.host.edge.hovered = hot.is_some();
                                if let Some(line) = hot.filter(|_| !area.host.edge.dragging) {
                                    area.host.edge.at = line;
                                }
                                if area.host.edge.dragging {
                                    if blocked || event.pressed_button != Some(MouseButton::Left) {
                                        area.edge_release();
                                    } else {
                                        area.edge_move(f32::from(event.position.x - bounds.left()));
                                    }
                                    cx.stop_propagation();
                                }
                                if old
                                    != (
                                        area.host.edge.hovered,
                                        area.host.edge.at,
                                        area.host.edge.last,
                                    )
                                {
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
                                let Some(host) = shell.panes[side].as_mut() else {
                                    return;
                                };
                                let input = &mut host.input;
                                let Some(columns) = host.columns.as_mut() else {
                                    return;
                                };
                                if columns.area.host.edge.dragging {
                                    // The drag kept its moves, so the release places the still pointer the next frame re-hovers from.
                                    input.last_pointer = Some(event.position);
                                    input.pointer_origin = true;
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
        let Some(area) = self.panes[side]
            .as_mut()
            .and_then(|host| host.columns.as_mut())
            .map(|columns| &mut columns.area)
        else {
            return;
        };
        let stored = std::mem::take(&mut area.stored).last().copied();
        let tracks = area.stored_tracks.take();
        let data = &self.launch.settings.data;
        let mut preview = Vec::new();
        if let Some(width) = stored
            .filter(|&width| crate::window::selection_preview::host::share(data) != width as f32)
        {
            preview.push(("width".into(), Json::Num(width.to_string())));
        }
        if let Some(tracks) = tracks {
            let held = crate::window::selection_preview::host::tracks(data);
            if held != tracks.iter().map(|&w| w as f32).collect::<Vec<_>>() {
                let items = tracks.iter().map(|w| Json::Num(w.to_string())).collect();
                preview.push(("tracks".into(), Json::Arr(items)));
            }
        }
        if preview.is_empty() {
            return;
        }
        self.change_settings(
            Json::Obj(vec![("preview".into(), Json::Obj(preview))]),
            window,
            cx,
        );
    }
}
