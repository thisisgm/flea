//! A source authorizes one landing; peers only paint its captured carry until that grant.
use super::{
    carry::{window_id, Carrying},
    carry_bus as bus,
    carry_wire::{now, Offer, LIFETIME_MS},
};
use crate::state::Shell;
use flea::ipc::wire::{Reply, Request};
use gpui::{Context, Window};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

#[derive(Default)]
pub(super) struct State {
    pub current: Option<Offer>,
    pub owned: bool,
    pub pending: Option<String>,
    pub newest: u128,
    latest: (u128, u32, u128),
    retired: HashMap<String, u128>,
    leases: HashMap<String, (u64, u128)>,
    started: HashSet<u64>,
}

impl State {
    fn remember(&mut self, offer: &Offer) {
        self.leases.retain(|_, (_, until)| *until > now());
        self.leases
            .insert(offer.token.clone(), (offer.lifted.serial, offer.expires));
    }

    fn authorizes(&self, token: &str, offer: &Offer) -> bool {
        self.leases
            .get(token)
            .is_some_and(|(serial, until)| *serial == offer.lifted.serial && *until > now())
    }

    fn end_lift(&mut self, offer: &Offer) -> Vec<String> {
        let mut tokens = vec![offer.token.clone()];
        self.leases.retain(|token, (serial, until)| {
            if *serial != offer.lifted.serial {
                return true;
            }
            if *until > now() && *token != offer.token {
                tokens.push(token.clone());
            }
            false
        });
        for token in &tokens {
            self.retire(token.clone());
        }
        tokens
    }

    pub fn retire(&mut self, token: String) {
        self.retired.retain(|_, until| *until > now());
        self.retired.insert(token, now() + LIFETIME_MS);
    }

    // Refreshes of a lift this peer dropped stay refused, while a newer lift still rides.
    pub fn drop_lift(&mut self, offer: &Offer) {
        self.retire(offer.token.clone());
        self.latest = self.latest.max((offer.lift_stamp, offer.pid, u128::MAX));
    }

    // The grant answer frees the claim it ends, landed or not; true when this call freed it.
    pub fn settle_claim(&mut self, token: &str) -> bool {
        if self.pending.as_deref() != Some(token) {
            return false;
        }
        self.pending = None;
        if self
            .current
            .as_ref()
            .is_some_and(|offer| offer.token == token)
        {
            self.current = None;
        }
        self.retire(token.to_owned());
        true
    }

    pub fn admit(&mut self, offer: &Offer) -> bool {
        let order = (offer.lift_stamp, offer.pid, offer.stamp);
        if !offer.live() || order <= self.latest || self.retired.contains_key(&offer.token) {
            return false;
        }
        self.newest = self.newest.max(offer.stamp);
        self.latest = order;
        true
    }
}

impl Carrying {
    fn claimable(&self, token: &str) -> Option<Offer> {
        self.ipc
            .current
            .as_ref()
            .filter(|offer| {
                self.ipc.owned
                    && offer.live()
                    && self.ipc.authorizes(token, offer)
                    && self.carry.riding() == Some(&offer.lifted)
            })
            .cloned()
    }
}

pub(super) fn send(function: &'static str, arguments: Vec<String>, cx: &gpui::App) {
    cx.background_executor()
        .spawn(async move {
            bus::broadcast(function, arguments);
        })
        .detach();
}

impl Shell {
    pub(super) fn carry_advertise(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let carrying = cx.global_mut::<Carrying>();
        let Some(lifted) = carrying.carry.riding().cloned() else {
            return;
        };
        let lift_stamp = carrying
            .ipc
            .current
            .as_ref()
            .filter(|offer| carrying.ipc.owned && offer.lifted == lifted)
            .map(|offer| offer.lift_stamp);
        let mut offer = Offer::new(lifted, (*carrying.flock).clone(), carrying.ipc.newest);
        if let Some(lift_stamp) = lift_stamp {
            offer.lift_stamp = lift_stamp;
        }
        carrying.ipc.newest = offer.stamp;
        carrying.ipc.latest = (offer.lift_stamp, offer.pid, offer.stamp);
        carrying.ipc.owned = true;
        carrying.ipc.remember(&offer);
        carrying.ipc.current = Some(offer.clone());
        send(
            "carryOffer",
            vec![offer.pid.to_string(), offer.token.clone()],
            cx,
        );
        self.carry_expiry(offer, window, cx);
    }

    pub(super) fn carry_expiry(
        &mut self,
        offer: Offer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owned = cx.global::<Carrying>().ipc.owned;
        let delay = Duration::from_millis(if owned {
            (LIFETIME_MS / 2) as u64
        } else {
            offer.expires.saturating_sub(now()) as u64
        });
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let carrying = cx.global_mut::<Carrying>();
                let current = carrying
                    .ipc
                    .current
                    .as_ref()
                    .is_some_and(|current| current.token == offer.token);
                if owned {
                    if current
                        && carrying.ipc.owned
                        && carrying.carry.riding() == Some(&offer.lifted)
                    {
                        shell.carry_advertise(window, cx);
                    }
                    return;
                }
                let claiming = carrying.ipc.pending.as_deref() == Some(&offer.token);
                // A claim in flight keeps its offer, so Escape still drops the lift; the grant callback settles both.
                if current && !carrying.ipc.owned && !claiming {
                    carrying.ipc.current = None;
                    carrying.ipc.retire(offer.token.clone());
                    shell.carry_ended(window_id(window), cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn carry_discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let carrying = cx.default_global::<Carrying>();
        // Each window of this process observes its own activation.
        if !carrying.ipc.started.insert(window_id(window)) {
            return;
        }
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.carry_refresh(window, cx);
            }
        })
        .detach();
        self.carry_refresh(window, cx);
    }

    fn carry_refresh(&mut self, window: &Window, cx: &mut Context<Self>) {
        let found = cx.background_executor().spawn(async { bus::discover() });
        cx.spawn_in(window, async move |this, cx| {
            let offers = found.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                for offer in offers {
                    shell.carry_accept(offer, window, cx);
                }
            });
        })
        .detach();
    }

    fn carry_accept(&mut self, offer: Offer, window: &mut Window, cx: &mut Context<Self>) {
        let carrying = cx.default_global::<Carrying>();
        if !carrying.ipc.admit(&offer) {
            return;
        }
        carrying.carry.cancel();
        carrying.ipc.owned = false;
        carrying.flock = Rc::new(offer.flock.clone());
        carrying.ipc.current = Some(offer.clone());
        self.carry_ended(window_id(window), cx);
        self.carry_expiry(offer, window, cx);
        cx.notify();
    }

    pub(super) fn carry_replace_shared(&mut self, cx: &mut Context<Self>) {
        let carrying = cx.global_mut::<Carrying>();
        if carrying.ipc.owned {
            if let Some(offer) = carrying.ipc.current.take() {
                for token in carrying.ipc.end_lift(&offer) {
                    send("carryCancel", vec![token], cx);
                }
                return;
            }
        }
        let pending = carrying.ipc.pending.take();
        let token = match carrying.ipc.current.take() {
            Some(offer) => {
                carrying.ipc.drop_lift(&offer);
                Some(offer.token)
            }
            None => pending,
        };
        if let Some(token) = token {
            carrying.ipc.retire(token.clone());
            send("carryCancel", vec![token], cx);
        }
    }

    pub(super) fn carry_end_shared(&mut self, cx: &mut Context<Self>) {
        let carrying = cx.global_mut::<Carrying>();
        let owned = carrying.ipc.owned;
        if let Some(offer) = carrying.ipc.current.take_if(|offer| {
            owned
                && carrying
                    .carry
                    .riding()
                    .is_none_or(|lifted| lifted.serial != offer.lifted.serial)
        }) {
            for token in carrying.ipc.end_lift(&offer) {
                send("carryCancel", vec![token], cx);
            }
        }
    }

    pub(crate) fn carry_ipc_call(
        &mut self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Reply> {
        let Request::Call {
            target,
            function,
            arguments,
        } = request
        else {
            return None;
        };
        if target != "flea"
            || !matches!(
                function.as_str(),
                "carryOffer" | "carryPeek" | "carryCancel" | "carryTaken" | "carryClaim"
            )
        {
            return None;
        }
        if let Err(reply) = flea::ipc::registry::validate(target, function, arguments) {
            return Some(reply);
        }
        let token = &arguments[0];
        match function.as_str() {
            "carryOffer" => {
                let Ok(pid) = token.parse::<u32>() else {
                    return Some(Reply::value("false"));
                };
                let token = arguments[1].clone();
                let found = cx
                    .background_executor()
                    .spawn(async move { bus::fetch(pid, &token) });
                cx.spawn_in(window, async move |this, cx| {
                    if let Some(offer) = found.await {
                        let _ = this.update_in(cx, |shell, window, cx| {
                            shell.carry_accept(offer, window, cx)
                        });
                    }
                })
                .detach();
            }
            "carryPeek" => {
                let carrying = cx.default_global::<Carrying>();
                let offer = carrying.ipc.current.as_ref().filter(|offer| {
                    carrying.ipc.owned
                        && offer.live()
                        && (token.is_empty() || offer.token == *token)
                        && carrying.carry.riding() == Some(&offer.lifted)
                });
                return Some(Reply::value(offer.map(Offer::encode).unwrap_or_default()));
            }
            "carryClaim" => {
                let carrying = cx.default_global::<Carrying>();
                let offer = carrying.claimable(token);
                let Some(offer) = offer else {
                    return Some(Reply::value("false"));
                };
                carrying.carry.landed(offer.lifted.serial);
                carrying.ipc.current = None;
                for token in carrying.ipc.end_lift(&offer) {
                    send("carryTaken", vec![token], cx);
                }
                self.carry_ended(window_id(window), cx);
                cx.notify();
                return Some(Reply::value("true"));
            }
            "carryCancel" | "carryTaken" => {
                let carrying = cx.default_global::<Carrying>();
                let mut cancelled = Vec::new();
                let owned = carrying.claimable(token);
                if function == "carryCancel" {
                    if let Some(offer) = owned {
                        carrying.carry.cancel();
                        carrying.ipc.current = None;
                        cancelled = carrying.ipc.end_lift(&offer);
                    }
                }
                carrying.ipc.retire(token.clone());
                if let Some(offer) = carrying.ipc.current.take_if(|offer| offer.token == *token) {
                    carrying.ipc.drop_lift(&offer);
                    carrying.carry.cancel();
                }
                if function == "carryCancel" && carrying.ipc.pending.as_ref() == Some(token) {
                    carrying.ipc.pending = None;
                }
                for token in cancelled {
                    send("carryCancel", vec![token], cx);
                }
                self.carry_ended(window_id(window), cx);
                cx.notify();
            }
            _ => unreachable!(),
        }
        Some(Reply::value("true"))
    }
}
