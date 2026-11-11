//! Carry lands through the existing DG2 decision and op1 operation authority.
use super::carry::{window_id, Carrying};
use super::join::request;
use crate::columns::TrackView;
use crate::g2::drag_drop::{self, facts, Dropping, Facts, Landing, Lift};
use crate::state::Shell;
use crate::window::startup::View;
use flea::ui_logic_drag_drop::Marker;
use flea::ui_logic_status_feedback::status::Hint;
use gpui::Window;

const CARRY_SERIALS: u64 = 1 << 61;

// What the click resolved against at the press; facts return later and land only while all of it still holds.
#[derive(Clone)]
struct Destination {
    path: String,
    listing: usize,
    mode: View,
    track: Option<TrackView>,
    remote: Option<super::carry_wire::Offer>,
    granted: bool,
}

impl Shell {
    /// A click on a Flea folder while a carry rides: the carry as the mouse drag's own Lift, with the keys held at the click, decided by dg2 into one op1 ask on `side`; true while a carry held the click.
    pub(crate) fn carry_land(
        &mut self,
        side: usize,
        dest: &str,
        ctrl: bool,
        shift: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.carry_land_on(side, dest, (ctrl, shift), None, window, cx)
    }

    /// A carry is pending or rides, so a left press over a listing is the carry's to take.
    pub(crate) fn carry_riding_or_pending(&self, cx: &gpui::App) -> bool {
        cx.try_global::<Carrying>()
            .is_some_and(|carrying| carrying.active())
    }

    /// A List press while a carry is pending or rides, inside the rows and off the scroll lane: true when the carry took it.
    pub(crate) fn carry_list_press(
        &mut self,
        side: usize,
        frame: crate::input::Frame,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if event.button != gpui::MouseButton::Left
            || !frame.bounds.contains(&event.position)
            || frame.in_lane(event.position)
        {
            return false;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        let Some(carrying) = cx
            .try_global::<Carrying>()
            .filter(|carrying| carrying.active())
        else {
            return false;
        };
        // Owned copy first: the cx borrow must end before stop_propagation.
        let carried = carrying.source_rows(window, side, host).to_vec();
        cx.stop_propagation();
        // The pane element holds the pointer for this very press before asking; any other owner still refuses it.
        let gate = crate::navigation::watch::Gate {
            pointer_held: false,
            ..host.nav.gate
        };
        if host.stopped || gate.owned() {
            return true;
        }
        let dest = drag_drop::join::pane_spot(host, event.position, &carried);
        let keys = (event.modifiers.control, event.modifiers.shift);
        self.carry_land_on(side, &dest, keys, None, window, cx);
        true
    }

    /// `carry_land` on a Columns track: the click lands only while that track still draws what it drew at the press.
    pub(crate) fn carry_land_on(
        &mut self,
        side: usize,
        dest: &str,
        (ctrl, shift): (bool, bool),
        track: Option<TrackView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(carrying) = cx.try_global::<Carrying>() else {
            return false;
        };
        let Some(lifted) = carrying.riding() else {
            return carrying.active();
        };
        if carrying.ipc.pending.is_some() {
            return true;
        }
        let remote = (!carrying.ipc.owned)
            .then(|| carrying.ipc.current.clone())
            .flatten();
        let at = window.mouse_position();
        let lift = Lift {
            side: lifted.source.side,
            listing: lifted.source.listing as usize,
            rows: lifted.rows.clone(),
            paths: Some(lifted.paths.clone()),
            marker: Marker {
                instance: drag_drop::instance().to_owned(),
                rows: lifted.rows.clone(),
                copy: false,
                source: lifted.source.folder.clone(),
                dev: 0,
                deletable: true,
                shift: false,
                listing: lifted.source.listing,
            },
            origin: (f32::from(at.x), f32::from(at.y)),
            serial: CARRY_SERIALS | lifted.serial,
            left: true,
        };
        let Some(host) = self.panes[side].as_ref() else {
            return true;
        };
        if host.pane.busy() || dest.is_empty() {
            if host.pane.busy() {
                let text = self.locale.catalog.tr("pane.busy");
                self.status_messages(cx, |messages, now| {
                    messages.say(&text, false, "", Hint::None, now)
                });
            }
            return true;
        }
        let destination = Destination {
            path: host.path.clone(),
            listing: host.pane.held.listing,
            mode: self.view,
            track,
            remote,
            granted: false,
        };
        let dest = dest.to_owned();
        let first = lifted.paths.first().cloned();
        let read_dest = dest.clone();
        let facts = cx
            .background_executor()
            .spawn(async move { facts::read(first.as_deref(), &read_dest) });
        cx.spawn_in(window, async move |this, cx| {
            let facts = facts.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.carry_finish(
                    side,
                    &destination,
                    &lift,
                    (ctrl, shift),
                    &dest,
                    facts,
                    window,
                    cx,
                )
            });
        })
        .detach();
        true
    }

    // The carry must still ride, the window keep the view, input gate and Columns track the click resolved in, and the pane still list that folder, or the click lands nothing; a refusal says its sentence and the carry keeps riding. True only when op1 took the ask.
    #[allow(clippy::too_many_arguments)]
    fn carry_finish(
        &mut self,
        side: usize,
        destination: &Destination,
        lift: &Lift,
        keys: (bool, bool),
        dest: &str,
        facts: Facts,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let serial = lift.serial & !CARRY_SERIALS;
        let still = cx.try_global::<Carrying>().is_some_and(|carrying| {
            if let Some(offer) = &destination.remote {
                // The source's grant outlives the peer's lease on its copy.
                if destination.granted {
                    carrying.ipc.pending.as_deref() == Some(&offer.token)
                } else {
                    offer.live()
                        && carrying
                            .ipc
                            .current
                            .as_ref()
                            .is_some_and(|current| current.token == offer.token)
                }
            } else {
                carrying
                    .carry
                    .riding()
                    .is_some_and(|lifted| lifted.serial == serial)
            }
        });
        if !still
            || self.lifecycle.phase() != crate::window::lifecycle::Phase::Open
            || self.view != destination.mode
            || self.input_blocked(cx)
            || self.panes[side].as_ref().is_none_or(|host| {
                host.path != destination.path
                    || host.pane.held.listing != destination.listing
                    || host.pane.busy()
                    || host.stopped
                    || destination.track.as_ref().is_some_and(|track| {
                        host.columns
                            .as_ref()
                            .is_none_or(|columns| !columns.area.tracks().contains(track))
                    })
            })
        {
            return false;
        }
        let mut landed = false;
        match drag_drop::decide(&Dropping::own(lift, keys.0, keys.1), dest, facts) {
            Landing::Ask(ask) => {
                let Some((request, _)) = request(&ask) else {
                    return false;
                };
                if let Some(offer) = destination.remote.as_ref().filter(|_| !destination.granted) {
                    let offer = offer.clone();
                    let token = offer.token.clone();
                    cx.global_mut::<Carrying>().ipc.pending = Some(token.clone());
                    let grant = cx.background_executor().spawn(async move {
                        super::carry_bus::call(offer.pid, "carryClaim", vec![offer.token])
                            .is_some_and(|reply| reply.stdout.trim() == "true")
                    });
                    let (mut destination, lift, dest) =
                        (destination.clone(), lift.clone(), dest.to_owned());
                    destination.granted = true;
                    cx.spawn_in(window, async move |this, cx| {
                        let granted = grant.await;
                        let open = this.update_in(cx, |shell, window, cx| {
                            let landed = granted
                                && shell.carry_finish(
                                    side,
                                    &destination,
                                    &lift,
                                    keys,
                                    &dest,
                                    facts,
                                    window,
                                    cx,
                                );
                            // A grant ends the source's carry, so a landing that then moved nothing must say so.
                            if cx.global_mut::<Carrying>().ipc.settle_claim(&token) && !landed {
                                shell.say_stopped(cx);
                            }
                            shell.carry_ended(window_id(window), cx);
                            cx.notify();
                        });
                        // A claim whose window closed first still frees the process carry for the other windows.
                        if open.is_err() {
                            let app: &gpui::AsyncApp = cx;
                            if app.update_global::<Carrying, _>(|carrying, _| {
                                carrying.ipc.settle_claim(&token)
                            }) {
                                app.update(|cx| cx.refresh_windows());
                            }
                        }
                    })
                    .detach();
                    return false;
                }
                if self.ask_operation(side, &request, &[], false, window, cx) {
                    // The granting source already ended its carry and told every peer; a newer offer keeps riding here.
                    if let Some(offer) = &destination.remote {
                        let carrying = cx.global_mut::<Carrying>();
                        if carrying
                            .ipc
                            .current
                            .as_ref()
                            .is_some_and(|current| current.token == offer.token)
                        {
                            carrying.ipc.current = None;
                        }
                    } else {
                        cx.global_mut::<Carrying>().carry.landed(serial);
                    }
                    self.carry_ended(window_id(window), cx);
                    landed = true;
                }
            }
            Landing::Refused(refused) => {
                let text = refused.say(&self.locale.catalog);
                self.status_messages(cx, |messages, now| {
                    messages.say(&text, false, "", Hint::None, now)
                });
            }
            Landing::Silent => {}
        }
        cx.notify();
        landed
    }
}
