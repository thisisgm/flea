//! The drag owner joined to the live Shell: the pane's on_drag lifts the pressed row's carried rows, its own drops and every native drop the router hands this window resolve their folder, read the disk facts off the UI thread, and send one ask through op1.
use super::files::{Arrived, Files, Held};
use super::router::{self, Registration};
use super::{decide, facts, lift, spot, Dropping, Facts, Landing, Lift, Rows};
use crate::{state::Shell, window::pane::PaneHost};
use flea::ui_logic_dg2 as core;
use flea::ui_logic_fb1::status::Hint;
use gpui::{App, Context, ExternalDragPayload, Global, Point, Window};
use std::collections::HashMap;
use std::rc::Rc;

#[path = "crumb_drop.rs"]
mod crumb_drop;
#[path = "owned.rs"]
mod owned;
#[path = "peek_lift.rs"]
mod peek_lift;
#[path = "rail_drop.rs"]
pub(crate) mod rail_drop;
#[path = "spring.rs"]
pub(super) mod spring;
#[path = "tab_drop.rs"]
mod tab_drop;
use owned::Owned;
#[path = "arrival.rs"]
mod arrival;
#[path = "feedback.rs"]
mod feedback;
#[path = "wide.rs"]
mod wide;

#[path = "flock.rs"]
mod image;
#[path = "native.rs"]
mod native;
pub use image::{flock, Flock};

#[path = "drags.rs"]
mod drags;
#[path = "row_drop.rs"]
mod row_drop;
pub use drags::Drags;

struct HostRows<'a>(&'a PaneHost);

impl Rows for HostRows<'_> {
    fn folder(&self) -> &str {
        &self.0.path
    }
    fn listing(&self) -> usize {
        self.0.pane.held.listing
    }
    fn row(&self, index: usize) -> Option<(&str, bool)> {
        self.0
            .pane
            .held
            .row(index)
            .map(|row| (row.name.as_str(), row.dir))
    }
    fn selected(&self) -> &[usize] {
        &self.0.selected
    }
}

/// The folder a landing at `point` in a List pane resolves to, as a drop does: the folder row under it unless
/// `carried` holds that row, else the pane's own folder.
pub(crate) fn pane_spot(host: &PaneHost, point: Point<gpui::Pixels>, carried: &[usize]) -> String {
    if host.drawn_view == Some(crate::window::startup::View::Columns) {
        return crate::window::columns::drop_spot(host, point, carried)
            .unwrap_or_else(|| host.path.clone());
    }
    row_spot(host, host.input.hit(point), carried)
}

/// The same for a row another view has already hit-tested and validated (a Grid tile), None for its background.
pub(crate) fn row_spot(host: &PaneHost, row: Option<usize>, carried: &[usize]) -> String {
    spot(
        &HostRows(host),
        row.filter(|row| !carried.contains(row)),
        None,
    )
}

/// The value a pane's on_drag carries; the lift is captured from the press when the drag starts.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    pub side: usize,
}

// File lifts number from here, so a tab strip filtering native source events by its own serial never takes one.
const FILE_SERIALS: u64 = 1 << 62;

fn window_key(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

fn at(point: Point<gpui::Pixels>) -> (f32, f32) {
    (f32::from(point.x), f32::from(point.y))
}

impl Shell {
    /// The drag's start: lift the rows the press carries, or nothing when the press is not a live left press on a row; a selection reaching past the held rows asks the backend for its paths.
    pub fn begin_file_drag(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = window_key(window);
        // A drag that lifts nothing must never land an earlier drag's rows.
        cx.default_global::<Drags>().take(key);
        self.clear_file_rest(window, cx);
        self.clear_file_cue(window, cx);
        self.clear_rail_aim(window, cx);
        let drags = cx.default_global::<Drags>();
        drags.serial += 1;
        let serial = FILE_SERIALS | drags.serial;
        if let Some((lifted, picture)) = self.peek_lift(side, window, serial) {
            let Some(host) = self.panes[side].as_ref() else {
                return false;
            };
            let flock = image::pictured(host, picture, window, cx);
            cx.default_global::<Drags>().flocks.insert(key, flock);
            cx.default_global::<Drags>().put(key, lifted);
            self.say_held_drag(window, cx);
            return true;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let Some(press) = host.input.press.as_ref() else {
            return false;
        };
        let current = press.event.button == gpui::MouseButton::Left
            && press.listing == host.pane.held.listing
            && host.pane.held.row(press.row).map(|row| row.name.as_str()) == press.name.as_deref();
        if !current || host.pane.busy() {
            return false;
        }
        // FileDrag.liftBegan: a lift ends the pending slow click, so the next tap on the row only records itself.
        self.rename.slow = None;
        let modifiers = press.event.modifiers;
        let Some(lifted) = lift(
            &HostRows(host),
            side,
            press.row,
            at(press.event.position),
            modifiers.control,
            modifiers.shift,
            serial,
        ) else {
            return false;
        };
        if lifted.paths.is_none() && !self.ask_lift_paths(side, &lifted, window, cx) {
            let text = core::too_wide(
                &self.locale.catalog,
                lifted.rows.len(),
                modifiers.control,
                modifiers.shift,
            );
            self.say_drop(side, text, cx);
            return false;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let flock = image::captured(host, &lifted.rows, window, cx);
        cx.default_global::<Drags>().flocks.insert(key, flock);
        cx.default_global::<Drags>().put(key, lifted);
        // FileDrag.startOffer: the lift says its line with no folder name until a target names one.
        self.say_held_drag(window, cx);
        true
    }

    /// This window's lift while its drag is in flight, for the dragState reader.
    pub(crate) fn file_drag_lift<'a>(&self, window: &Window, cx: &'a App) -> Option<&'a Lift> {
        cx.try_global::<Drags>()?.lifts.get(&window_key(window))
    }

    /// The drag ended over this window: a drop that never left the slop is the tap pointer_up already handled.
    pub fn land_own_drag(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_file_cue(window, cx);
        self.clear_file_rest(window, cx);
        self.clear_rail_aim(window, cx);
        let key = window_key(window);
        let Some(lift) = cx.default_global::<Drags>().take(key) else {
            return;
        };
        let position = at(window.mouse_position());
        if !lift.travelled(position) {
            return;
        }
        let modifiers = window.modifiers();
        let owned = Owned::Own {
            lift,
            ctrl: modifiers.control,
            shift: modifiers.shift,
        };
        self.land_at(side, window.mouse_position(), owned, window, cx);
    }

    fn land_at(
        &mut self,
        side: usize,
        point: Point<gpui::Pixels>,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx) {
            return;
        }
        if let Some((side, dest)) = self.file_crumb_destination(point, cx) {
            self.land_crumb(side, dest, owned, window, cx);
            return;
        }
        if let Some(tab) = self.file_tab_destination(point) {
            self.land_then(self.focus_side, wide::Then::Tab(tab), owned, window, cx);
            return;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        if host.pane.busy() {
            self.refuse_drop_loading(side, cx);
            return;
        }
        // Only this pane's own lift, in the listing it was lifted from, carries rows a drop must skip.
        let carried = match &owned {
            Owned::Own { lift, .. }
                if lift.side == side && lift.listing == host.pane.held.listing =>
            {
                lift.rows.as_slice()
            }
            _ => &[],
        };
        let dest = pane_spot(host, point, carried);
        // Recent stands on "/" and search results on their root, neither a folder of its own, so a drop on their blank ground lands nowhere (PaneWire.qml).
        if (host.recent.on || host.search.owner.active()) && dest == host.path {
            return;
        }
        self.land_then(side, wide::Then::Into(dest), owned, window, cx);
    }

    // The folder is resolved: read the disk facts off the UI thread, then decide and ask op1.
    fn land_into(
        &mut self,
        side: usize,
        dest: String,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let path = host.path.clone();
        let first = owned.first_source();
        let read_dest = dest.clone();
        let facts = cx
            .background_executor()
            .spawn(async move { facts::read(first.as_deref(), &read_dest) });
        cx.spawn_in(window, async move |this, cx| {
            let facts = facts.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.finish_drop(side, &path, owned, &dest, facts, window, cx)
            });
        })
        .detach();
    }

    // The pane must still list the folder the drop was resolved in, or the drop is dropped.
    #[allow(clippy::too_many_arguments)]
    fn finish_drop(
        &mut self,
        side: usize,
        path: &str,
        owned: Owned,
        dest: &str,
        facts: Facts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panes[side]
            .as_ref()
            .is_none_or(|host| host.path != path)
        {
            return;
        }
        if let Some((request, probe)) = owned.shelf_ask(dest) {
            let probe: Vec<&str> = probe.iter().map(String::as_str).collect();
            self.ask_operation(side, &request, &probe, false, window, cx);
            return;
        }
        match decide(&owned.dropping(), dest, facts) {
            Landing::Ask(ask) => {
                if let Some((request, _)) = crate::g2::dg1::join::request(&ask) {
                    self.ask_operation(side, &request, &[], false, window, cx);
                }
            }
            Landing::Refused(refused) => {
                let text = refused.say(&self.locale.catalog);
                self.say_drop(side, text, cx);
            }
            Landing::Silent => {}
        }
    }

    fn say_drop(&mut self, side: usize, text: String, cx: &mut Context<Self>) {
        if let Some(host) = self.panes[side].as_mut() {
            host.status = text.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(&text, false, "", Hint::None, now)
        });
        cx.notify();
    }

    fn refuse_drop_loading(&mut self, side: usize, cx: &mut Context<Self>) {
        let text = self.locale.catalog.tr("pane.busy");
        self.say_drop(side, text, cx);
    }
}

#[cfg(test)]
pub(super) fn pending_drop(window: &Window, side: usize, cx: &App) -> bool {
    cx.global::<Drags>()
        .waiting
        .contains_key(&(window_key(window), side))
}
