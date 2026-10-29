//! Carry over the live Shell: X lifts through the clipboard key hook, the backend's paths answer makes it ride, a click on a Flea folder lands it as the mouse drag's own Lift through dg2's facts and decide into op1, and esc or the source's end drops it; one process carry, so a click in another Flea window lands it too.
use super::flock::{FlockElement, NameCache};
use super::join::{picks, PaneWire};
use super::seams::Wire;
use crate::{palette::Theme, row::marks::Mark, state::Shell};
use flea::ui_logic_dg1::carry::{Answer, Carry, LiftOut, Source};
use flea::ui_logic_fb1::status::Hint;
use gpui::{App, Window};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// The flock a carry paints: up to three marks, front first, and the front row's name, captured at the lift.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flock {
    pub marks: Vec<Mark>,
    pub name: String,
}

/// The process carry and the window whose footer says it.
#[derive(Default)]
pub struct Carrying {
    pub carry: Carry,
    pub flock: Rc<Flock>,
    name: Rc<RefCell<NameCache>>,
    said: HashMap<u64, (u64, u64)>,
}

impl gpui::Global for Carrying {}

impl Carrying {
    /// Only the captured source listing excludes its carried rows from folder targets.
    pub(crate) fn source_rows(
        &self,
        window: &Window,
        side: usize,
        host: &crate::window::pane::PaneHost,
    ) -> &[usize] {
        self.carry
            .riding()
            .filter(|lifted| {
                lifted.source.window == window_id(window)
                    && lifted.source.side == side
                    && lifted.source.folder == host.path
                    && lifted.source.listing == host.pane.held.listing as u64
            })
            .map_or(&[], |lifted| lifted.rows.as_slice())
    }
}

/// Board carry.html: the flock sits 19 px right and 27 px below the resting pointer, at 0.8 opacity.
pub const OFFSET: (f32, f32) = (19., 27.);
pub const OPACITY: f32 = 0.8;
/// Each layer behind the front: its offset and opacity, nearest first.
pub const LAYERS: [(f32, f32, f32); 3] = [(0., 0., 1.), (-4., -4., 0.65), (-8., -8., 0.4)];
pub const GLYPH: f32 = 19.;
pub const NAME_AT: (f32, f32) = (28., 1.);
pub const NAME_SIZE: f32 = 14.;

pub(super) fn window_id(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

impl Shell {
    /// The X key: the selection, else the cursor row, resolved by the backend as a copy's rows are; the pointer is never moved.
    pub(crate) fn carry_lift(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> bool {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let rows = picks(host);
        let source = Source {
            window: window_id(window),
            side,
            folder: host.path.clone(),
            listing: host.pane.held.listing as u64,
        };
        let busy = host.nav.gate.paths_pending || self.clipboard.board.paths_pending(side as u64);
        let languages = host.list.borrow().view.lang_marks;
        let held = &host.pane.held;
        let flock = Flock {
            marks: rows
                .iter()
                .take(LAYERS.len())
                .map(|index| {
                    held.row(*index)
                        .map_or(Mark::File, |row| Mark::for_facts(row, languages))
                })
                .collect(),
            name: rows
                .first()
                .and_then(|index| held.row(*index))
                .map(|row| row.name.clone())
                .unwrap_or_default(),
        };
        let catalog = self.locale.catalog.clone();
        let out = cx
            .default_global::<Carrying>()
            .carry
            .lift(&catalog, &rows, source, busy);
        match out {
            LiftOut::Nothing => {}
            LiftOut::Busy(said) => {
                self.status_messages(cx, |messages, now| {
                    messages.say(&said.text, false, "", Hint::None, now)
                });
            }
            LiftOut::Ask(line) => {
                self.carry_ended(window_id(window), cx);
                let backend = self.panes[side]
                    .as_ref()
                    .and_then(|host| host.backend.as_ref());
                let sent = PaneWire(backend).send(line);
                let carrying = cx.global_mut::<Carrying>();
                if sent {
                    carrying.flock = Rc::new(flock);
                } else {
                    carrying.carry.failed(window_id(window), side);
                    self.say_stopped(cx);
                }
                if let Some(host) = self.panes[side].as_mut() {
                    host.nav.gate.clip_pending =
                        sent || self.clipboard.board.paths_pending(side as u64);
                }
            }
        }
        cx.notify();
        true
    }

    /// Whether this window side's backend owes the carry its paths answer, so no other asker takes the slot.
    pub(crate) fn carry_pending(&self, side: usize, window: &Window, cx: &App) -> bool {
        cx.try_global::<Carrying>()
            .is_some_and(|carrying| carrying.carry.pending(window_id(window), side))
    }

    /// The paths answer, claimed only while the carry asked on this side: a fresh answer rides and says so, a stale one says the stale line.
    pub(crate) fn carry_paths(
        &mut self,
        side: usize,
        paths: &[String],
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.carry_pending(side, window, cx) {
            return false;
        }
        let listing = self.panes[side]
            .as_ref()
            .map_or(u64::MAX, |host| host.pane.held.listing as u64);
        let catalog = self.locale.catalog.clone();
        let id = window_id(window);
        let out =
            cx.global_mut::<Carrying>()
                .carry
                .resolved(&catalog, id, side, paths.to_vec(), listing);
        self.carry_slot_released(side);
        match out {
            Answer::Rides => self.carry_say(id, cx),
            Answer::Stale(said) => {
                self.status_messages(cx, |messages, now| {
                    messages.say(&said.text, false, "", Hint::None, now)
                });
            }
            // carry_pending claimed it above, so even a foreign answer cannot reach here; both end the claim.
            Answer::Drained | Answer::Foreign => {}
        }
        cx.notify();
        true
    }

    /// A refused paths request ends a pending lift on this side.
    pub(crate) fn carry_paths_failed(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let id = window_id(window);
        let ours = cx.try_global::<Carrying>().is_some()
            && cx.global_mut::<Carrying>().carry.failed(id, side);
        if ours {
            self.carry_slot_released(side);
            cx.notify();
        }
        ours
    }

    // The pane's backend is gone or its pipe refused the line: the request never left, so say so.
    pub(super) fn say_stopped(&mut self, cx: &mut gpui::Context<Self>) {
        let text = self.locale.catalog.tr("feedback.error.stopped");
        self.status_messages(cx, |messages, now| {
            messages.say(&text, true, "", Hint::None, now)
        });
    }

    fn carry_slot_released(&mut self, side: usize) {
        let pending = self.clipboard.board.paths_pending(side as u64);
        if let Some(host) = self.panes[side].as_mut() {
            host.nav.gate.clip_pending = pending;
        }
    }

    // The footer's notice for a riding carry, remembered by serial so its end withdraws exactly it.
    fn carry_say(&mut self, window: u64, cx: &mut gpui::Context<Self>) {
        let Some((text, _hint)) =
            Carry::status(&cx.global::<Carrying>().carry, &self.locale.catalog)
        else {
            return;
        };
        let lifted = cx
            .global::<Carrying>()
            .carry
            .riding()
            .expect("status needs a riding Carry")
            .serial;
        let serial = self.status_messages(cx, |messages, now| {
            messages.say(&text, false, "", Hint::Carry, now);
            messages.notice_serial()
        });
        cx.global_mut::<Carrying>()
            .said
            .insert(window, (lifted, serial));
    }

    // Withdraws the carry's own notice when this window said it and no carry rides; a newer notice is left standing, and another window's notice waits for that window's own sync.
    pub(super) fn carry_end_said(&mut self, window: u64, cx: &mut gpui::Context<Self>) {
        let Some((lifted, serial)) = cx
            .try_global::<Carrying>()
            .and_then(|carrying| carrying.said.get(&window).copied())
        else {
            return;
        };
        if cx
            .global::<Carrying>()
            .carry
            .riding()
            .is_some_and(|current| current.serial == lifted)
        {
            return;
        }
        self.status_messages(cx, |messages, now| messages.withdraw(serial, now));
        cx.global_mut::<Carrying>().said.remove(&window);
    }

    // A carry that ended here repaints every other Flea window, so each withdraws its own notice and drops the flock on that frame.
    pub(super) fn carry_ended(&mut self, window: u64, cx: &mut gpui::Context<Self>) {
        for handle in cx.windows() {
            if handle.window_id().as_u64() != window {
                let _ = handle.update(cx, |_, window, _| window.refresh());
            }
        }
        self.carry_end_said(window, cx);
    }

    /// Once per frame: a carry that ended in another window leaves this window's notice behind, and this withdraws it.
    pub(crate) fn carry_sync(&mut self, window: &Window, cx: &mut gpui::Context<Self>) {
        self.carry_end_said(window_id(window), cx);
    }

    /// Esc: drops a lifted or resolving carry; true when one was.
    pub(crate) fn carry_escape(&mut self, window: &Window, cx: &mut gpui::Context<Self>) -> bool {
        let dropped =
            cx.try_global::<Carrying>().is_some() && cx.global_mut::<Carrying>().carry.cancel();
        if dropped {
            self.carry_ended(window_id(window), cx);
            cx.notify();
        }
        dropped
    }

    /// The source window closed or left its workspace: its carry ends; another window's keeps riding.
    pub(crate) fn carry_window_ended(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let id = window_id(window);
        let ended = cx.try_global::<Carrying>().is_some()
            && cx.global_mut::<Carrying>().carry.end_window(id);
        if ended {
            self.carry_ended(id, cx);
            cx.notify();
        } else {
            self.carry_end_said(id, cx);
        }
        ended
    }

    /// The source pane's backend drained or failed: the rows it named can no longer move, so the carry ends.
    pub(crate) fn carry_source_ended(
        &mut self,
        side: usize,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let id = window_id(window);
        if cx.try_global::<Carrying>().is_some()
            && cx.global_mut::<Carrying>().carry.end_source(id, side)
        {
            self.carry_ended(id, cx);
            cx.notify();
        }
    }

    /// The flock to paint at the resting pointer while a carry rides, else None.
    pub(crate) fn carry_flock(&self, window: &Window, cx: &App) -> Option<FlockElement> {
        let carrying = cx.try_global::<Carrying>()?;
        carrying.carry.riding()?;
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        Some(FlockElement {
            flock: Rc::clone(&carrying.flock),
            name: Rc::clone(&carrying.name),
            at: window.mouse_position(),
            ground: theme.background,
            ink: theme.foreground,
        })
    }
}
