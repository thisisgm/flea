//! Pointer routing uses the track that painted the point, with neighbour backgrounds deferred through the navigator.
use super::PaneHost;
use crate::{
    columns::{geometry::Slot, menu, TrackRows},
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

struct Navigation<'a> {
    host: &'a mut PaneHost,
}
impl menu::Navigator for Navigation<'_> {
    fn path(&self) -> &str {
        self.host.pane.path()
    }
    fn busy(&self) -> menu::Busy {
        let gate = self.host.nav.gate;
        menu::Busy {
            in_flight: self.host.pane.busy(),
            search: self.host.pane.state.search || gate.search,
            collide_pending: gate.collision_pending,
            menu_visible: gate.menu_open,
            menu_action_open: gate.menu_pending,
            renaming: gate.editing,
            rename_pending: gate.rename_pending,
            filter_typing: gate.filter_typing,
            band: gate.selection_band,
            ..menu::Busy::default()
        }
    }
    fn in_flight(&self) -> bool {
        self.host.pane.busy()
    }
    fn listing_state(&self) -> crate::pane::states::Kind {
        self.host.pane.state.kind
    }
    fn open(&mut self, path: &str) {
        let _ = self.host.nav.open_place(&mut self.host.pane, path);
    }
    fn say_busy(&mut self) {}
    fn open_background_menu(&mut self, at: (f32, f32)) {
        self.host.nav.background_at = Some(at);
    }
}

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
            let position = (y / crate::list::pane::ROW_HEIGHT).floor() as usize;
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
        if event.button == MouseButton::Right && row.is_none() {
            let base = if view.slot == Slot::Active {
                Some(host.path.clone())
            } else {
                columns.area.background_base(view.slot)
            };
            if let Some(base) = base {
                let mut background = std::mem::take(&mut host.nav.menus.background);
                let at = (f32::from(event.position.x), f32::from(event.position.y));
                background.route(Some(&mut Navigation { host }), &base, Some(at));
                host.nav.menus.background = background;
                self.run_navigation(side, window, cx);
                self.columns_landed(side, window, cx);
            }
            return;
        }
        if event.button == MouseButton::Right && view.slot == Slot::Active {
            if !host.selected.contains(&index) {
                host.selected.clear();
            }
            self.open_menu(
                side,
                Some(index),
                (event.position.x.into(), event.position.y.into()),
                window,
                cx,
            );
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
        let Some(host) = self.panes[side].as_mut().filter(|host| !host.pane.busy()) else {
            return;
        };
        let Some(columns) = host.columns.as_mut() else {
            return;
        };
        let slot = columns.area.tracks().iter().find_map(|view| {
            let paint = columns.painters.get(view.slot)?.borrow();
            let drawn = paint.drawn.as_ref()?;
            (drawn.bounds.contains(&event.position)
                && !matches!(drawn.view().rows, TrackRows::Preview))
            .then_some(view.slot)
        });
        let Some(slot) = slot else { return };
        let dy = -f32::from(
            event
                .delta
                .pixel_delta(gpui::px(crate::list::pane::ROW_HEIGHT))
                .y,
        );
        if slot == Slot::Active {
            host.pane.scroll_by(dy);
        } else {
            columns.area.scroll_by(slot, dy);
        }
        cx.stop_propagation();
        cx.notify();
    }
}
