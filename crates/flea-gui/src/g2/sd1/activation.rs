//! A chosen provider refreshes under its captured menu before file identity is validated.
use super::join::Senders;
use crate::state::Shell;
use flea::ui_logic_sd1::providers::Role;
use gpui::{Context, Window};

#[derive(Clone)]
pub(super) struct Activation {
    pub side: usize,
    pub id: u64,
    action: String,
    leaf: String,
    role: Role,
    fresh_requested: bool,
    ready: bool,
}

impl Senders {
    pub(super) fn destination_needed(&self, side: usize) -> bool {
        // Activation validates the existing destination capture; capturing again would accept a replacement folder.
        self.activation.as_ref().is_none_or(|a| a.side != side)
    }

    pub(super) fn activation_role(&self, side: usize) -> Option<Role> {
        self.activation
            .as_ref()
            .filter(|a| a.side == side && a.fresh_requested && !a.ready)
            .map(|a| a.role)
    }
}

impl Shell {
    pub(in crate::g2::sd1) fn provider_query(
        &mut self,
        side: usize,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        self.menu.providers.asked[side] = id;
        if self.menu.providers.destination_needed(side) {
            self.menu.providers.destination[side] = None;
        }
        self.menu_send(side, format!("{{\"c\":\"formats\",\"id\":{id}}}"), cx);
    }

    pub(crate) fn provider_before_activation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(capture), Some((action, leaf))) = (&self.menu.captured, &self.menu.pending)
        else {
            return false;
        };
        // Tailscale's row carries its plugin-send action on the stage; LocalSend sends without a refresh.
        let Some(role) =
            crate::g2::plugins::menu::send_role(action).filter(|r| *r != Role::LocalSend)
        else {
            return false;
        };
        let current = self.menu.providers.activation.as_ref().is_some_and(|a| {
            a.side == capture.side && a.id == capture.id && a.action == *action && a.leaf == *leaf
        });
        if !current {
            self.menu.providers.activation = Some(Activation {
                side: capture.side,
                id: capture.id,
                action: action.clone(),
                leaf: leaf.clone(),
                role,
                fresh_requested: false,
                ready: false,
            });
        }
        self.provider_activation_ready(window, cx);
        self.menu
            .providers
            .activation
            .as_ref()
            .is_none_or(|a| !a.ready)
    }

    pub(in crate::g2::sd1) fn provider_activation_ready(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.menu.providers.activation.clone().filter(|a| !a.ready) else {
            return;
        };
        let current = self
            .menu
            .captured
            .as_ref()
            .is_some_and(|c| c.side == a.side && c.id == a.id)
            && self.menu.pending.as_ref() == Some(&(a.action.clone(), a.leaf.clone()));
        if !current || self.menu.providers.asked[a.side] != 0 {
            return;
        }
        if !a.fresh_requested {
            self.menu
                .providers
                .activation
                .as_mut()
                .unwrap()
                .fresh_requested = true;
            self.provider_query(a.side, a.id, cx);
            return;
        }
        let providers = &self.menu.providers;
        let sender = &providers.sides[a.side];
        if sender.row_on(a.role) {
            let busy = match a.role {
                Role::Tailscale => sender.td_checking,
                Role::Dropbox => sender.db.checking,
                Role::LocalSend | Role::AirDrop => false,
            };
            if busy {
                return;
            }
        }
        self.menu.providers.activation.as_mut().unwrap().ready = true;
        self.activate_menu(window, cx);
    }
}
