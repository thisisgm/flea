//! The picker's lifecycle: menu open, reply and pick, each process an Action for the host.
use super::seams::{Action, ProviderState, RunKind, Wire};
use super::ProviderSend;
use flea::i18n::I18n;
use flea::ui_logic_mn1::build::Peer;
use flea::ui_logic_sd1::providers::{Facts, Role};
use flea::ui_logic_sd1::{dropbox, localsend, share, taildrop, wire, Send};

// The OEM Tailscale service bounds status queries at fifteen seconds, as ui/Taildrop.qml does.
pub(super) const STATUS_TIMEOUT_SECS: usize = 15;

impl ProviderSend {
    // A file menu open refreshes LocalSend by its warm policy and Taildrop now; nothing polls while shut.
    pub fn menu_opened(
        &mut self,
        wire: &dyn Wire,
        facts: &Facts,
        states: Option<&[ProviderState]>,
        hidden: &[String],
        now_ms: u64,
    ) -> Vec<Action> {
        self.menu_refresh(wire, facts, states, hidden, now_ms, None)
    }

    pub(super) fn menu_refresh(
        &mut self,
        wire: &dyn Wire,
        facts: &Facts,
        states: Option<&[ProviderState]>,
        hidden: &[String],
        now_ms: u64,
        only: Option<Role>,
    ) -> Vec<Action> {
        self.hidden = hidden.to_vec();
        self.gated = states.is_some();
        self.facts = facts.clone();
        self.menu_open = true;
        self.derive_providers(states);
        self.send_command = facts.send_command.clone();
        // A row that left the menu keeps no peers from an earlier open, as Taildrop.qml and LocalSend.qml empty theirs.
        if !self.row_on(Role::LocalSend) {
            self.ls_peers.clear();
        }
        if !self.row_on(Role::Tailscale) {
            self.td_peers.clear();
            self.td_addresses.clear();
        }
        let mut out = Vec::new();
        // LocalSend.qml refresh(installed): an uninstalled CLI empties the peers and forgets the answer.
        if facts.localsend_command.is_empty() {
            self.ls_peers.clear();
            self.ls_answered_once = false;
            self.ls_reason.clear();
        } else if self.row_on(Role::LocalSend)
            && only.is_none()
            && localsend::should_refresh(true, self.ls_checking, self.ls_asked_at_ms, now_ms)
        {
            let id = self.request_id();
            if wire.send(wire::peers_line(id)) {
                self.ls_asking = Some(id);
                self.ls_checking = true;
            }
        }
        // One status process at a time, as ui/Taildrop.qml holds; a menu opened over it waits.
        if self.row_on(Role::Tailscale) && only.is_none_or(|role| role == Role::Tailscale) {
            if self.td_checking {
                self.td_waiting = true;
            } else if let Some(argv) = self.start_status() {
                out.push(Action::Run(
                    RunKind::TaildropStatus(self.td_generation),
                    argv,
                ));
            }
        }
        if only.is_none_or(|role| role == Role::Dropbox) {
            out.extend(self.dropbox_refresh());
        }
        out
    }

    pub fn menu_closed(&mut self) {
        // The status run outlives the menu, as the QML singleton does; only a menu still open asks again.
        self.menu_open = false;
    }

    // A plugin switch while the menu stays open re-derives the rows from the stored facts, no runs.
    pub fn states_changed(&mut self, states: Option<&[ProviderState]>, hidden: &[String]) {
        self.hidden = hidden.to_vec();
        self.gated = states.is_some();
        self.derive_providers(states);
        // A switch that hides a row also forgets the peers that row was offering.
        if !self.row_on(Role::LocalSend) {
            self.ls_peers.clear();
        }
        if !self.row_on(Role::Tailscale) {
            self.td_peers.clear();
            self.td_addresses.clear();
        }
    }

    // The one status ask empties the old peers, as Taildrop.qml refresh() does, and takes a new generation.
    fn start_status(&mut self) -> Option<Vec<String>> {
        self.td_peers.clear();
        self.td_addresses.clear();
        self.td_code.clear();
        self.td_waiting = false;
        if self.facts.tailscale_command.is_empty() || self.send_command.is_empty() {
            self.td_reason.clear();
            self.td_checking = false;
            return None;
        }
        self.td_reason = "checking".into();
        self.td_generation += 1;
        self.td_checking = true;
        Some(vec![
            "timeout".into(),
            "--signal=KILL".into(),
            STATUS_TIMEOUT_SECS.to_string(),
            self.facts.tailscale_command.clone(),
            "status".into(),
            "--json".into(),
        ])
    }

    // One backend line in; the same line stays unclaimed for op1's Jobs projection at join.
    pub fn line(&mut self, text: &str, i18n: &I18n, now_ms: u64) -> Vec<Action> {
        let mut out = Vec::new();
        match wire::parse_line(text) {
            Some(wire::Event::Peers { id, peers, reason }) if self.ls_asking == Some(id) => {
                self.ls_asking = None;
                self.ls_checking = false;
                self.ls_answered_once = true;
                self.ls_asked_at_ms = Some(now_ms);
                self.builds.set(self.builds.get() + 1);
                self.ls_peers = peers
                    .iter()
                    .map(|peer| Peer {
                        id: localsend::row_id(&peer.name).into(),
                        label: peer.name.clone(),
                    })
                    .collect();
                self.ls_reason = reason;
            }
            Some(wire::Event::Peers { .. }) => {}
            Some(wire::Event::Sent { id, ok, reason }) => {
                let mut said = None;
                if let Some(send) = self
                    .sends
                    .iter_mut()
                    .find(|send| send.request == id && send.running())
                {
                    if send.apply(&wire::Event::Sent {
                        id,
                        ok,
                        reason: reason.clone(),
                    }) {
                        said = Some(localsend::verdict(i18n, ok, &reason));
                    }
                }
                // A finished send keeps no paths, as a 1,204-file transfer must not stay held.
                self.sends.retain(Send::running);
                if let Some(said) = said {
                    out.push(Action::Say(said, !ok));
                }
            }
            None => {}
        }
        out
    }

    // The captured Taildrop status run came back; a stale generation drops, a KILL timeout reads statusFailed.
    pub fn taildrop_done(
        &mut self,
        generation: u64,
        exit_code: i32,
        stdout: &str,
        stderr: &str,
        i18n: &I18n,
    ) -> Vec<Action> {
        if !self.td_checking || generation != self.td_generation {
            return Vec::new();
        }
        self.td_checking = false;
        let timeout = i18n.tr("send.providers.taildrop_timeout");
        let error = if super::dropbox_status::killed(exit_code) {
            timeout.as_str()
        } else {
            stderr
        };
        let state = taildrop::status(stdout, exit_code, error);
        self.builds.set(self.builds.get() + 1);
        self.td_peers = state
            .peers
            .iter()
            .map(|peer| Peer {
                id: peer.id.clone(),
                label: taildrop::label_or_unknown(i18n, &peer.label),
            })
            .collect();
        self.td_addresses = state
            .peers
            .iter()
            .map(|peer| (peer.id.clone(), peer.address.clone()))
            .collect();
        self.td_reason = state.reason;
        self.td_code = state.code;
        let mut out = Vec::new();
        let waited = self.td_waiting && self.menu_open && self.row_on(Role::Tailscale);
        self.td_waiting = false;
        if waited {
            if let Some(argv) = self.start_status() {
                out.push(Action::Run(
                    RunKind::TaildropStatus(self.td_generation),
                    argv,
                ));
            }
        }
        out
    }

    // A LocalSend pick: an empty pick and a gone CLI each say their sentence; a dead wire is the host's lost backend.
    pub fn send_localsend(
        &mut self,
        wire: &dyn Wire,
        peer: &str,
        paths: &[String],
        i18n: &I18n,
    ) -> Vec<Action> {
        if paths.is_empty() {
            return vec![Action::Say(localsend::dispatch_empty(i18n), true)];
        }
        // LocalSend.qml send() refuses an empty peer; LocalSend.js words that refusal as missing().
        if peer.is_empty() || !self.provider(Role::LocalSend).installed {
            // The provider's own reason words the refusal, as LocalSend.js missing() reads it.
            let reason = (!self.facts.localsend_reason.is_empty())
                .then_some(self.facts.localsend_reason.as_str());
            return vec![Action::Say(localsend::missing(i18n, reason), true)];
        }
        // A disabled, failed or hidden provider row never writes its send line.
        if !self.row_on(Role::LocalSend) {
            return vec![Action::Say(i18n.tr("send.providers.menu_stale"), true)];
        }
        let id = self.request_id();
        if !wire.send(wire::send_line(id, peer, paths)) {
            return Vec::new();
        }
        self.sends.push(Send::new(id, peer, paths));
        vec![Action::Say(localsend::sending(i18n, peer, paths), false)]
    }

    // The menu chose a Taildrop peer: only the cursor file is sent, and a stale pick refuses.
    pub fn send_taildrop(&self, peer_id: &str, paths: &[String], i18n: &I18n) -> Vec<Action> {
        // A disabled, failed, hidden or uninstalled Tailscale row spawns nothing.
        if !self.row_on(Role::Tailscale) {
            return vec![Action::Say(i18n.tr("send.providers.menu_stale"), true)];
        }
        if paths.len() != 1 || !paths[0].starts_with('/') {
            return vec![Action::Say(i18n.tr("send.providers.menu_stale"), true)];
        }
        let Some(peer) = self.td_peers.iter().find(|peer| peer.id == peer_id) else {
            return vec![Action::Say(
                share::taildrop_refusal(i18n, &self.td_code, &self.td_reason),
                true,
            )];
        };
        if self.send_command.is_empty() || self.td_checking {
            return vec![Action::Say(
                share::taildrop_refusal(i18n, &self.td_code, &self.td_reason),
                true,
            )];
        }
        let Some(address) = self.td_addresses.get(peer_id).cloned() else {
            return vec![Action::Say(
                share::taildrop_refusal(i18n, &self.td_code, &self.td_reason),
                true,
            )];
        };
        let argv = vec![self.send_command.clone(), address, paths[0].clone()];
        let failed = share::taildrop_failed(i18n, &peer.label, &paths[0]);
        vec![
            Action::Spawn(argv, failed),
            Action::Say(share::taildrop_sending(i18n, &peer.label, &paths[0]), false),
        ]
    }

    // Move to Dropbox routes through the ordinary transfer, refusing a stale membership first.
    pub fn dropbox_member(&self, root: &str, path: &str) -> bool {
        dropbox::contains(root, path)
    }

    // Copy Share Link: sharelink then the clipboard run; an empty pick is a stale menu, as Pane.qml answers it.
    pub fn copy_share_link(&mut self, path: &str, i18n: &I18n) -> Vec<Action> {
        // ShareLink rides the Dropbox provider, so its row gates the run the same way.
        if !self.row_on(Role::Dropbox) {
            return vec![Action::Say(i18n.tr("send.providers.menu_stale"), true)];
        }
        if path.is_empty() {
            return vec![Action::Say(i18n.tr("send.providers.menu_stale"), true)];
        }
        match self.link.copy(path) {
            Err(_) if self.link.pending.is_some() => {
                vec![Action::Say(share::ShareLink::busy_reason(i18n), true)]
            }
            Err(_) => vec![Action::Say(share::ShareLink::no_file_reason(i18n), true)],
            Ok(()) => vec![Action::Run(
                RunKind::ShareLink,
                vec!["dropbox-cli".into(), "sharelink".into(), path.into()],
            )],
        }
    }

    // Started and exit code apart: a run that never started says "asked", a non-zero exit "make".
    pub fn sharelink_done(
        &mut self,
        started: bool,
        exit_code: i32,
        output: &str,
        i18n: &I18n,
    ) -> Vec<Action> {
        match self.link.share_done(started, exit_code, output) {
            share::ShareStep::CopyClipboard(url) => {
                vec![Action::Run(RunKind::Clipboard, vec!["wl-copy".into(), url])]
            }
            share::ShareStep::Fail(step) => vec![Action::Say(
                share::ShareLink::fail_reason(i18n, &step),
                true,
            )],
            share::ShareStep::Idle | share::ShareStep::Copied(_) => Vec::new(),
        }
    }

    pub fn clipboard_done(&mut self, ok: bool, i18n: &I18n) -> Vec<Action> {
        match self.link.clipboard_done(ok) {
            share::ShareStep::Copied(_) => {
                vec![Action::Say(i18n.tr("send.providers.share_copied"), false)]
            }
            share::ShareStep::Fail(step) => vec![Action::Say(
                share::ShareLink::fail_reason(i18n, &step),
                true,
            )],
            share::ShareStep::Idle | share::ShareStep::CopyClipboard(_) => Vec::new(),
        }
    }

    // The backend ended: live sends are lost, while the Taildrop status run is a host process and still lands.
    pub fn backend_dead(&mut self) {
        self.ls_asking = None;
        self.ls_checking = false;
        for send in &mut self.sends {
            send.backend_dead();
        }
        self.sends.retain(Send::running);
    }
}
