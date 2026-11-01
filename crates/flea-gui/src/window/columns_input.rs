//! Pointer routing uses the track that painted the point; neighbour menus capture paths without navigation.
use crate::{
    columns::{geometry::Slot, TrackRows},
    state::Shell,
};
use gpui::{Context, MouseButton, MouseDownEvent, ScrollWheelEvent, Window};

/// Menu owners consume this only after the clicked directory's own rows have committed.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundMenu {
    pub side: usize,
    pub path: String,
    pub at: (f32, f32),
}
impl gpui::EventEmitter<BackgroundMenu> for Shell {}

impl Shell {
    pub(crate) fn columns_pointer(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view != super::super::startup::View::Columns {
            return;
        }
        self.columns_scroll_hold(side, event.position);
        // Carry25's card gate stops only a carry landing; a plain press keeps the gate Columns had before carry.
        let blocked = self.input_blocked(cx);
        let Some(host) = self.panes[side].as_mut().filter(|host| !host.stopped) else {
            return;
        };
        let Some(columns) = &host.columns else { return };
        let tracks = columns.area.tracks();
        let hit = tracks.iter().enumerate().find_map(|(at, view)| {
            let paint = columns.painters.get(view.slot)?.borrow();
            let drawn = paint.drawn.as_ref()?;
            if !std::rc::Rc::ptr_eq(&drawn.tracks, tracks)
                || drawn.at != at
                || drawn
                    .held
                    .as_ref()
                    .is_some_and(|held| held.listing != host.pane.held.listing)
            {
                return None;
            }
            // The preview track's own views take their pointer: its play mark, links and scroll.
            if !drawn.bounds.contains(&event.position)
                || matches!(drawn.view().rows, TrackRows::Preview)
            {
                return None;
            }
            let y = f32::from(event.position.y - drawn.bounds.origin.y) + drawn.scroll;
            let position = drawn.grow.position_at(y, drawn.row_height);
            // Under a filter a position past its rows is background, never the listing row of that number.
            let index = drawn.row_at(position).unwrap_or(usize::MAX);
            let row = match &drawn.view().rows {
                TrackRows::Peek(rows) => rows.get(index).map(|row| (row.name.clone(), row.dir)),
                TrackRows::Listing => drawn
                    .held
                    .as_ref()?
                    .row(index)
                    .map(|row| (row.name.clone(), row.dir)),
                _ => None,
            };
            Some((view.clone(), index, row))
        });
        let Some((view, index, row)) = hit else {
            return;
        };
        // The active track's mounted input owns ordinary left presses through their release.
        if view.slot == Slot::Active
            && event.button == MouseButton::Left
            && !blocked
            && !cx
                .try_global::<crate::g2::dg1::carry::Carrying>()
                .is_some_and(|carrying| carrying.carry.active())
        {
            return;
        }
        // The window's focus-on-press stops here with the event, so a neighbour column takes the keyboard back as a list press does (Pane.qml focusPress).
        if !self.focus.is_focused(window) {
            window.focus(&self.focus, cx);
            self.keyboard.context = flea::keys::Context::Listing;
        }
        cx.stop_propagation();
        if host.pane.busy() || !view.enabled || host.nav.gate.owned() {
            return;
        }
        if event.button == MouseButton::Left {
            if let Some(carrying) = cx
                .try_global::<crate::g2::dg1::carry::Carrying>()
                .filter(|carrying| carrying.carry.active())
            {
                host.input.press = None;
                if blocked {
                    return;
                }
                let dest = if view.slot == Slot::Active {
                    let carried = carrying.source_rows(window, side, host);
                    crate::g2::dg2::join::row_spot(host, row.as_ref().map(|_| index), carried)
                } else {
                    match row.as_ref() {
                        Some((name, true)) => crate::navigation::history::join(&view.path, name),
                        _ => view.path.clone(),
                    }
                };
                let keys = (event.modifiers.control, event.modifiers.shift);
                self.carry_land_on(side, &dest, keys, Some(view), window, cx);
                return;
            }
        }
        if event.button == MouseButton::Right {
            let at = (event.position.x.into(), event.position.y.into());
            if view.slot == Slot::Active {
                if row.is_some() && !host.selected.contains(&index) {
                    host.selected.clear();
                }
                self.open_menu(side, row.as_ref().map(|_| index), at, window, cx);
            } else if !view.path.is_empty() {
                let facts = match &view.rows {
                    TrackRows::Peek(rows) => rows.get(index).map(|row| flea::protocol::RowFacts {
                        name: row.name.clone(),
                        dir: row.dir,
                        icon: row.icon.clone(),
                        mode: if row.dir { 0o040000 } else { 0o100000 },
                        ..Default::default()
                    }),
                    _ => None,
                };
                self.columns_neighbour_menu(
                    side,
                    crate::g2::mn1::target::PathMenu {
                        base: view.path.clone(),
                        row: facts,
                    },
                    at,
                    window,
                    cx,
                );
            }
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }
        let Some((name, dir)) = row else { return };
        if view.slot == Slot::Active {
            host.pane.cursor = index;
            match host.filter.shown.as_deref() {
                Some(rows) => host
                    .pane
                    .reveal_view(rows.iter().position(|&row| row == index).unwrap_or(0)),
                None => host.pane.reveal_cursor(),
            }
        }
        if dir && (view.slot != Slot::Active || event.click_count == 2) {
            let target = crate::navigation::history::join(&view.path, &name);
            let _ = self.step(
                side,
                super::super::navigate::Step::OpenPlace(target),
                window,
                cx,
            );
        }
        if !dir && view.slot != Slot::Active && event.click_count == 2 && !blocked {
            self.open_file(
                side,
                crate::navigation::history::join(&view.path, &name),
                window,
                cx,
            );
        }
        self.sync_columns(window, cx);
        cx.notify();
    }

    pub(crate) fn columns_landed(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_columns(window, cx);
        self.menu_target_landed(side, window, cx);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if let Some(at) = host.nav.background_at.take() {
            let path = host.path.clone();
            self.open_menu(side, None, at, window, cx);
            cx.emit(BackgroundMenu { side, path, at });
        }
    }

    pub(crate) fn columns_wheel(
        &mut self,
        side: usize,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        self.columns_scroll_wheel(side, event, cx);
    }
}
