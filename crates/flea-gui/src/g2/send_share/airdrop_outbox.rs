//! The window's sends to AirDrop devices, one numbering whichever of owly and Omdrop ran each send.
use super::airdrop::Act;
use super::seams::Wire;
use flea::i18n::I18n;
use flea::ui_logic_context_menus::build::Peer;
use flea::ui_logic_send_share::airdrop as owly;

#[derive(Default)]
pub(crate) struct Outbox {
    next: u64,
    // Each request this window sent and the device it went to, until the backend answers it.
    sends: Vec<(u64, String)>,
}

impl Outbox {
    // A device leaf chosen: the backend runs the send as a job; `reachable` is empty while the radio is down.
    pub(crate) fn send(
        &mut self,
        reachable: &[owly::Peer],
        wire: &dyn Wire,
        peer: &str,
        paths: &[String],
        i18n: &I18n,
    ) -> Vec<Act> {
        let stale = vec![Act::Say(i18n.tr("send.providers.menu_stale"), true)];
        let Some(name) = reachable
            .iter()
            .find(|d| d.id == peer)
            .map(|d| d.name.clone())
        else {
            return stale;
        };
        if paths.is_empty() {
            return stale;
        }
        self.next += 1;
        // A pane whose backend is gone cannot run the job, so its menu is out of date.
        if !wire.send(owly::send_line(self.next, peer, &name, paths)) {
            return stale;
        }
        let said = owly::sending(i18n, &name, paths);
        self.sends.push((self.next, name));
        vec![Act::Say(said, false)]
    }

    // One backend line; only this window's own sends answer.
    pub(crate) fn answered(&mut self, text: &str, i18n: &I18n) -> Vec<Act> {
        let Some((id, code, reason)) = owly::sent(text) else {
            return Vec::new();
        };
        let Some(at) = self.sends.iter().position(|(request, _)| *request == id) else {
            return Vec::new();
        };
        let (_, peer) = self.sends.remove(at);
        owly::sent_said(i18n, &peer, &code, &reason)
            .map_or_else(Vec::new, |said| vec![Act::Say(said, code != "delivered")])
    }
}

// The flyout's devices in the order they answered, each with its phone mark.
pub(crate) fn listed(devices: &[owly::Peer]) -> Vec<(Peer, bool)> {
    let peer = |d: &owly::Peer| Peer {
        id: d.id.clone(),
        label: d.name.clone(),
    };
    devices.iter().map(|d| (peer(d), d.phone)).collect()
}
