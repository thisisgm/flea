//! The drag owner joined to the live Shell: the pane's on_drag lifts the pressed row's carried rows, its own drops and every native drop the router hands this window resolve their folder, read the disk facts off the UI thread, and send one ask through op1.
use super::files::{Arrived, Files, Held};
use super::router::{self, Registration};
use super::{decide, facts, lift, spot, Dropping, Facts, Landing, Lift, Rows};
use crate::{state::Shell, window::pane::PaneHost};
use flea::ui_logic_dg2 as core;
use flea::ui_logic_fb1::status::Hint;
use gpui::{
    App, AppContext, Context, Empty, ExternalDragPayload, Global, IntoElement, Point, Render,
    Window,
};
use std::collections::HashMap;
use std::rc::Rc;

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

/// The drag image; the flock look is the drag-cut-move board's, which this seam does not draw.
pub struct Flock;

impl Render for Flock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

// What waits for the facts: this window's own lift or an arrived native drop.
enum Owned {
    Own { lift: Lift, ctrl: bool, shift: bool },
    Native(Arrived),
}

impl Owned {
    fn first_source(&self) -> Option<String> {
        match self {
            Self::Own { lift, .. } => lift.paths.as_ref()?.first().cloned(),
            Self::Native(arrived) => arrived.urls.iter().find_map(|url| core::file_path(url)),
        }
    }

    fn dropping(&self) -> Dropping<'_> {
        match self {
            Self::Own { lift, ctrl, shift } => Dropping::own(lift, *ctrl, *shift),
            Self::Native(arrived) => Dropping::native(
                arrived.marker.as_deref(),
                &arrived.urls,
                arrived.plain.as_deref(),
                arrived.ctrl,
                arrived.shift,
                arrived.action,
            ),
        }
    }
}

/// Each window's live lift and router registration, held beside the Shell rather than in it.
#[derive(Default)]
pub struct Drags {
    lifts: HashMap<u64, Lift>,
    clients: HashMap<u64, Registration>,
    /// The marker tag and serial of each window's current lift, shared with its file client.
    held: HashMap<u64, Held>,
    serial: u64,
}

impl Drags {
    fn put(&mut self, key: u64, lift: Lift) {
        *self.held.entry(key).or_default().borrow_mut() =
            Some((lift.marker.instance.clone(), lift.serial));
        self.lifts.insert(key, lift);
    }

    fn take(&mut self, key: u64) -> Option<Lift> {
        if let Some(held) = self.held.get(&key) {
            *held.borrow_mut() = None;
        }
        self.lifts.remove(&key)
    }
}

impl Global for Drags {}

// File lifts number from here, so a tab strip filtering native source events by its own serial never takes one.
const FILE_SERIALS: u64 = 1 << 62;

fn window_key(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

fn at(point: Point<gpui::Pixels>) -> (f32, f32) {
    (f32::from(point.x), f32::from(point.y))
}

impl Shell {
    /// Registers this window with the process router and follows the drops it accepts; once per window.
    pub fn follow_file_drags(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        if cx.default_global::<Drags>().clients.contains_key(&key) {
            return;
        }
        let (send, receive) = async_channel::unbounded();
        let held = cx
            .global_mut::<Drags>()
            .held
            .entry(key)
            .or_default()
            .clone();
        let shared = router::shared(cx);
        let registration = shared.register(Rc::new(Files {
            window: key,
            send,
            held,
        }));
        cx.global_mut::<Drags>().clients.insert(key, registration);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(arrived) = receive.recv().await {
                let _ = this.update_in(cx, |shell, window, cx| {
                    shell.land_arrived(arrived, window, cx)
                });
            }
        })
        .detach();
    }

    /// The drag's start: lift the rows the press carries, or nothing when the press is not a live left press on a row; a selection reaching past the held rows refuses with its line.
    pub fn begin_file_drag(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = window_key(window);
        // A drag that lifts nothing must never land an earlier drag's rows.
        cx.default_global::<Drags>().take(key);
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
        let drags = cx.default_global::<Drags>();
        drags.serial += 1;
        let serial = FILE_SERIALS | drags.serial;
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
        if lifted.paths.is_none() {
            let text = core::too_wide(
                &self.locale.catalog,
                lifted.rows.len(),
                modifiers.control,
                modifiers.shift,
            );
            self.say_drop(side, text, cx);
            return false;
        }
        cx.default_global::<Drags>().put(key, lifted);
        true
    }

    /// What the platform gets when this window's drag leaves it.
    pub fn file_drag_payload(&self, window: &Window, cx: &App) -> Option<ExternalDragPayload> {
        cx.try_global::<Drags>()?
            .lifts
            .get(&window_key(window))
            .map(Lift::payload)
    }

    /// Escape during this window's own drag: the drag ends where it started and nothing moves.
    pub fn escape_file_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = window_key(window);
        let lifted = cx.default_global::<Drags>().take(key).is_some();
        lifted && cx.stop_active_drag(window)
    }

    /// The window closes: its lift and its router registration go with it.
    pub fn forget_file_drags(&mut self, window: &Window, cx: &mut App) {
        let key = window_key(window);
        let drags = cx.default_global::<Drags>();
        drags.take(key);
        drags.clients.remove(&key);
        drags.held.remove(&key);
    }

    /// The drag ended over this window: a drop that never left the slop is the tap pointer_up already handled.
    pub fn land_own_drag(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
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

    fn land_arrived(&mut self, arrived: Arrived, window: &mut Window, cx: &mut Context<Self>) {
        let point = gpui::point(gpui::px(arrived.position.0), gpui::px(arrived.position.1));
        let side = (0..self.panes.len()).find(|side| {
            self.panes[*side]
                .as_ref()
                .and_then(|host| host.input.frame)
                .is_some_and(|frame| frame.bounds.contains(&point))
        });
        if let Some(side) = side {
            self.land_at(side, point, Owned::Native(arrived), window, cx);
        }
    }

    fn land_at(
        &mut self,
        side: usize,
        point: Point<gpui::Pixels>,
        owned: Owned,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        if host.pane.busy() {
            let text = self.locale.catalog.tr("pane.busy");
            self.say_drop(side, text, cx);
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
}

/// The Flock view a pane's on_drag renders.
pub fn flock(cx: &mut App) -> gpui::Entity<Flock> {
    cx.new(|_| Flock)
}
