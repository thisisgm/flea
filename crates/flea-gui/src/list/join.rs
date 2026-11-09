//! Joins the retained List/header Elements to the live shell's input, settings and writer.
use crate::{
    list::{
        columns::{Column, StoredWidths, Switched},
        header, HeaderElement, Metrics, RowStates,
    },
    state::Shell,
    window::{navigate::Step, startup::View},
};
use flea::jsondoc::Json;
use gpui::{
    canvas, div, prelude::*, AnyElement, App, Context, DispatchPhase, MouseButton, MouseDownEvent,
    MouseMoveEvent, Window,
};
use std::rc::Rc;

#[path = "join_frame.rs"]
mod frame;

impl Shell {
    fn list_header_visible(&self, side: usize) -> bool {
        self.slots.covers[side].is_none()
            && matches!(self.shown_view(), View::List | View::Dual)
            && (side == 0 || self.view == View::Dual)
    }

    pub(crate) fn list_header(
        &mut self,
        side: usize,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, f32)> {
        if !self.list_header_visible(side) {
            if let Some(host) = &self.panes[side] {
                let mut list = host.list.borrow_mut();
                list.header.drag.cancel();
                list.header.press = None;
            }
            return None;
        }
        let host = self.panes[side].as_ref()?;
        {
            let mut list = host.list.borrow_mut();
            list.view.dual = self.view == View::Dual;
            list.view.switched =
                Switched::from_settings(&self.launch.settings.data).for_dual(list.view.dual);
            list.view.stored = StoredWidths::from_settings(&self.launch.settings.data);
            list.view.order = host.nav.drawn_order();
            list.view.set_searching(host.pane.state.search);
            // A listing out keeps the handles, as Header.qml gates only on view, Dual and search.
            if host.stopped || list.view.dual || list.view.searching {
                list.header.drag.cancel();
                list.header.hot = None;
            }
        }
        Some((
            div()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |shell, event, window, cx| {
                        shell.list_header_down(side, event, window, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |shell, event, window, cx| {
                        shell.header_menu_open(side, event, window, cx);
                    }),
                )
                .child(HeaderElement::new(Rc::clone(&host.list)))
                .child(header::probe_element(side, Rc::clone(&host.list)))
                .into_any_element(),
            Metrics::for_typeface(&host.list.borrow().typeface).header,
        ))
    }

    pub(crate) fn list_states(&self, side: usize, window: &Window, cx: &App) -> RowStates {
        let host = self.panes[side].as_ref().expect("a mounted pane");
        let cue = self.file_row_cue(side, window, cx);
        RowStates {
            drop_target: cue.as_ref().map(|cue| cue.0),
            drop_label: cue.map(|cue| cue.1),
            // v0.3.12 Row.qml reads the current pane's listArea.activeFocus: a showing Jobs card keeps it (EY-522), a deactivated window loses it.
            focused: window.is_window_active()
                && side == self.focus_side
                && self.focus.is_focused(window)
                && !self.wheel_blocked(cx),
            other_pane: self.view == View::Dual && side != self.focus_side,
            selected: host.input.selection.snapshot.clone(),
            // A held drag is a platform drag in v0.3.12, which delivers no hover, so no row lights under it.
            hovered: host.input.hovered.filter(|_| {
                host.input.pointer_origin
                    && host.input.band.is_none()
                    && !cx.has_active_drag()
                    && !self.input_blocked(cx)
                    // PaneRail.qml: a revealed overlay rail holds the pointer, so the rows left of its edge light nothing.
                    && !self.rail.as_ref().is_some_and(|join| {
                        join.host
                            .read(cx)
                            .cover()
                            .is_some_and(|cover| window.mouse_position().x < cover.right())
                    })
            }),
            renaming: self.renaming_row(side),
            grow: self.grows[side],
            ..self
                .settings
                .row_states(host.pane.held.listing, host.pane.cursor)
        }
    }

    fn list_header_down(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        if host.stopped || host.pane.state.search || self.card_holds_keyboard(cx) {
            return;
        }
        let mut list = host.list.borrow_mut();
        let x = f32::from(event.position.x - list.header.drawn.bounds.left());
        let layout = list.header.drawn.layout;
        let metrics = list.drawn.metrics;
        if let Some(column) = header::handle_at(&layout, &metrics, &list.view, x) {
            list.header.press = None;
            if event.click_count == 2 {
                // The held rows belong to the listing being replaced, so a busy pane fits nothing.
                list.header.drag.cancel();
                let patch = (!host.pane.busy())
                    .then(|| list.autofit(column, &host.pane.held, window))
                    .flatten();
                drop(list);
                if let Some(patch) = patch {
                    self.save_list_width(side, patch);
                }
            } else {
                let width = layout.cell(column).expect("a drawn handle").width;
                list.header.drag.begin(column, x, width);
            }
        } else {
            // Header.qml sorts from a TapHandler: remember the title now, sort on the release.
            let key = header::title_key(&layout, &metrics, x, list.view.dual);
            list.header.press = (!key.is_empty()).then(|| {
                (
                    key,
                    f32::from(event.position.x),
                    f32::from(event.position.y),
                )
            });
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn list_header_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let card = self.card_holds_keyboard(cx);
        for (side, host) in self.panes.iter().enumerate() {
            let Some(host) = host else { continue };
            let mut list = host.list.borrow_mut();
            if !self.list_header_visible(side) || card {
                list.header.drag.cancel();
                list.header.hot = None;
                list.header.press = None;
                continue;
            }
            if let Some((_, x, y)) = list.header.press {
                let travel =
                    (f32::from(event.position.x) - x).hypot(f32::from(event.position.y) - y);
                if event.pressed_button != Some(MouseButton::Left) || travel > header::TAP_SLOP {
                    list.header.press = None;
                }
            }
            if let Some(column) = list.header.drag.column {
                if event.pressed_button == Some(MouseButton::Left) && !host.stopped {
                    let x = f32::from(event.position.x - list.header.drawn.bounds.left());
                    if list.header.drag.move_to(column, x) {
                        cx.notify();
                    }
                } else {
                    list.header.drag.cancel();
                    cx.notify();
                }
                continue;
            }
            let bounds = list.header.drawn.bounds;
            let hot = bounds
                .contains(&event.position)
                .then(|| {
                    header::handle_at(
                        &list.header.drawn.layout,
                        &list.drawn.metrics,
                        &list.view,
                        f32::from(event.position.x - bounds.left()),
                    )
                })
                .flatten();
            if hot != list.header.hot {
                list.header.hot = hot;
                cx.notify();
            }
        }
    }

    pub(crate) fn list_header_input(cx: &mut Context<Self>) -> impl IntoElement {
        let moved = cx.listener(Self::list_header_move);
        let released = cx.listener(Self::list_header_up);
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble {
                        moved(event, window, cx);
                    }
                });
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        released(event, window, cx);
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    pub(crate) fn list_area(&self, side: usize) -> impl IntoElement {
        let host = self.panes[side].as_ref().expect("a mounted pane");
        let list = Rc::clone(&host.list);
        let ready = host.pane.state.kind == crate::pane::states::Kind::Ready;
        let scroll = host.pane.scroll_y;
        let listing = host.pane.held.listing;
        canvas(
            move |bounds, _, _| {
                let mut list = list.borrow_mut();
                if !ready {
                    list.hide_rows(bounds, scroll, listing);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    pub(crate) fn list_header_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let card = self.card_holds_keyboard(cx);
        for side in 0..2 {
            let mut tap = None;
            let patch = self.panes[side].as_ref().and_then(|host| {
                let mut list = host.list.borrow_mut();
                let press = list.header.press.take();
                let blocked = host.stopped
                    || host.pane.state.search
                    || card
                    || !matches!(self.view, View::List | View::Dual)
                    || !self.list_header_visible(side);
                if let (Some((key, x, y)), false) = (press, blocked) {
                    let travel =
                        (f32::from(event.position.x) - x).hypot(f32::from(event.position.y) - y);
                    let at = f32::from(event.position.x - list.header.drawn.bounds.left());
                    let layout = list.header.drawn.layout;
                    if travel <= header::TAP_SLOP
                        && header::title_key(&layout, &list.drawn.metrics, at, list.view.dual)
                            == key
                    {
                        tap = Some(key);
                    }
                }
                if blocked || self.view != View::List {
                    list.header.drag.cancel();
                }
                let over = list.header.drawn.bounds.contains(&event.position);
                list.end_drag(over)
            });
            if let Some(patch) = patch {
                self.save_list_width(side, patch);
                cx.notify();
            }
            if let Some(key) = tap {
                let _ = self.step(side, Step::SortColumn(key.into()), window, cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn fit_list_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != View::List {
            let text = self.locale.catalog.tr("feedback.autofit.list_required");
            self.status_messages(cx, |messages, now| {
                messages.explain(
                    &text,
                    flea::ui_logic_status_feedback::status::Hint::None,
                    now,
                );
            });
            return;
        }
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        if host.pane.busy() || host.stopped {
            return;
        }
        let patches: Vec<_> = Column::ALL
            .into_iter()
            .filter_map(|column| {
                host.list
                    .borrow_mut()
                    .autofit(column, &host.pane.held, window)
            })
            .collect();
        // Header.qml autofitAll writes one columnWidths patch, so no window sees a half-fitted header.
        if let Some(patch) = merged_widths(&patches) {
            self.save_list_width(side, patch);
        }
        cx.notify();
    }

    fn save_list_width(&mut self, side: usize, patch: String) {
        let Ok(Json::Obj(fields)) = flea::jsondoc::parse(&patch) else {
            return;
        };
        let Json::Obj(settings) = &mut self.launch.settings.data else {
            return;
        };
        for (key, value) in fields {
            let Json::Obj(updates) = value else { continue };
            let at = settings
                .iter()
                .position(|(name, _)| name == &key)
                .unwrap_or_else(|| {
                    settings.push((key, Json::Obj(Vec::new())));
                    settings.len() - 1
                });
            if !matches!(settings[at].1, Json::Obj(_)) {
                settings[at].1 = Json::Obj(Vec::new());
            }
            let Json::Obj(widths) = &mut settings[at].1 else {
                unreachable!()
            };
            for (name, value) in updates {
                widths.retain(|(key, _)| key != &name);
                widths.push((name, value));
            }
        }
        let failure = self.locale.catalog.tr("navigation.save_failed");
        if let Some(host) = &self.panes[side] {
            host.nav.writer.borrow_mut().write(patch, failure);
        }
    }
}

/// Several width patches as the one patch a fit writes; None when no column changed.
fn merged_widths(patches: &[String]) -> Option<String> {
    let mut widths = Vec::new();
    for patch in patches {
        let Ok(Json::Obj(fields)) = flea::jsondoc::parse(patch) else {
            continue;
        };
        for (_, value) in fields {
            if let Json::Obj(items) = value {
                widths.extend(items);
            }
        }
    }
    let widths: Vec<_> = widths
        .into_iter()
        .filter_map(|(key, value)| match value {
            Json::Num(number) => Some(format!("\"{key}\":{number}")),
            _ => None,
        })
        .collect();
    (!widths.is_empty()).then(|| format!("{{\"columnWidths\":{{{}}}}}", widths.join(",")))
}
