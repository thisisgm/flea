//! Joins the retained List/header Elements to the live shell's input, settings and writer.
use crate::{
    list::{
        columns::{self, Column, StoredWidths, Switched},
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

// Qt's startDragDistance: a press that travels farther is a drag, never a tap (Header.qml's TapHandler).
const TAP_SLOP: f32 = 10.;

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
            list.view.switched = Switched::from_settings(&self.launch.settings.data);
            list.view.stored = StoredWidths::from_settings(&self.launch.settings.data);
            list.view.order = host.nav.drawn_order();
            list.view.dual = self.view == View::Dual;
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
                .child(HeaderElement::new(Rc::clone(&host.list)))
                .child(header::probe_element(side, Rc::clone(&host.list)))
                .into_any_element(),
            Metrics::for_typeface(&host.list.borrow().typeface).header,
        ))
    }

    pub(crate) fn list_states(&self, side: usize, window: &Window, cx: &App) -> RowStates {
        let host = self.panes[side].as_ref().expect("a mounted pane");
        RowStates {
            focused: side == self.focus_side
                && self.focus.is_focused(window)
                && !self.input_blocked(cx),
            selected: host.input.selection.snapshot.clone(),
            hovered: host.input.hovered.filter(|_| {
                host.input.pointer_origin && host.input.band.is_none() && !self.input_blocked(cx)
            }),
            renaming: self.renaming_row(side),
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
            let key = title_key(&layout, &metrics, x);
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
                if event.pressed_button != Some(MouseButton::Left) || travel > TAP_SLOP {
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
                    if travel <= TAP_SLOP && title_key(&layout, &list.drawn.metrics, at) == key {
                        tap = Some(key);
                    }
                }
                if blocked || self.view != View::List {
                    list.header.drag.cancel();
                }
                list.end_drag()
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

/// The sort key of the title under x, empty over the mode column and the padding.
fn title_key(layout: &columns::Layout, metrics: &Metrics, x: f32) -> &'static str {
    Column::ALL
        .into_iter()
        .find(|column| {
            layout
                .cell(*column)
                .is_some_and(|cell| x >= cell.x && x < cell.right())
        })
        .map(|column| match column {
            Column::Mode => "",
            Column::Size => "size",
            Column::Date => "mtime",
            Column::Kind => "kind",
        })
        .unwrap_or_else(|| {
            header::HeaderState::title_cell(layout, metrics, None)
                .filter(|cell| x >= cell.x && x < cell.right())
                .map_or("", |_| "name")
        })
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
