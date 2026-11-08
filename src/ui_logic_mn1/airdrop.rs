//! Send with AirDrop's flyout: Turn On AirDrop while the radio is down, Looking for devices… until one answers, then the devices.
use super::build::Peer;
use super::entry::{Entry, Text};

// The leaf that turns AirDrop on; a device leaf is its service ID behind PEER, so no device can collide with it.
pub const TURN_ON: &str = "on";
pub const PEER: &str = "peer:";

// Sample input: on with [(id "3a9f", label "Jane's iPhone"), true] gives one leaf, id "peer:3a9f", under the phone mark.
pub fn flyout(on: bool, devices: &[(Peer, bool)]) -> Vec<Entry> {
    if !on {
        return vec![Entry::leaf(TURN_ON, Text::Key("menu.airdrop.turn_on"))];
    }
    if devices.is_empty() {
        let mut looking = Entry::leaf("", Text::Key("menu.airdrop.looking"));
        looking.disabled = true;
        looking.busy = true;
        return vec![looking];
    }
    devices
        .iter()
        .map(|(peer, phone)| {
            Entry::leaf(format!("{PEER}{}", peer.id), Text::Data(peer.label.clone()))
                .with_glyph(if *phone { "smartphone" } else { "laptop" })
        })
        .collect()
}

// A chosen leaf's device service ID, or None for Turn On AirDrop and anything else.
pub fn peer_id(leaf: &str) -> Option<&str> {
    leaf.strip_prefix(PEER).filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two devices may share a name, so each test device's service ID is its name with a suffix.
    fn peer(name: &str) -> Peer {
        Peer {
            id: format!("{name}-id"),
            label: name.into(),
        }
    }

    #[test]
    fn off_offers_turn_on_and_on_waits_under_the_omi_until_a_device_answers() {
        let off = flyout(false, &[(peer("Jane's iPhone"), true)]);
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].id, TURN_ON);
        assert_eq!(off[0].label, Some(Text::Key("menu.airdrop.turn_on")));
        assert!(!off[0].disabled && !off[0].busy);
        let looking = flyout(true, &[]);
        assert_eq!(looking.len(), 1);
        assert_eq!(looking[0].label, Some(Text::Key("menu.airdrop.looking")));
        assert!(looking[0].disabled && looking[0].busy && !looking[0].steppable());
    }

    #[test]
    fn devices_wear_the_phone_or_laptop_mark_and_round_trip_their_service_ids() {
        let rows = flyout(
            true,
            &[
                (peer("Jane's iPhone"), true),
                (peer("MacBook Air"), false),
                (peer("on"), false),
            ],
        );
        let glyphs: Vec<_> = rows.iter().map(|e| e.glyph).collect();
        assert_eq!(glyphs, [Some("smartphone"), Some("laptop"), Some("laptop")]);
        assert_eq!(rows[0].label, Some(Text::Data("Jane's iPhone".into())));
        assert_eq!(peer_id(&rows[0].id), Some("Jane's iPhone-id"));
        // A device named like the turn-on leaf still reads as that device.
        assert_eq!(peer_id(&rows[2].id), Some("on-id"));
        assert_eq!(peer_id(TURN_ON), None);
        assert_eq!(peer_id(PEER), None);
    }

    fn row(on: bool, devices: &[(Peer, bool)]) -> Entry {
        let mut row = Entry::row("airdrop", Text::Key("menu.row.airdrop"), "");
        row.submenu = Some(flyout(on, devices));
        row
    }

    #[test]
    fn an_open_flyout_waiting_for_devices_stays_open_when_they_answer() {
        let waiting = [row(true, &[])];
        let found = [row(
            true,
            &[(peer("iPhone"), true), (peer("MacBook Air"), false)],
        )];
        let kept = super::super::refresh::refreshed_cursor(&waiting, &found, 0, Some(0), 0);
        assert_eq!((kept.submenu_row, kept.submenu_cursor), (Some(0), 0));
        // Another provider's refresh while it still waits keeps it open too.
        let again = super::super::refresh::refreshed_cursor(&waiting, &waiting, 0, Some(0), 0);
        assert_eq!(again.submenu_row, Some(0));
        // A device that went away still closes the flyout, as a vanished Taildrop peer does.
        let gone = super::super::refresh::refreshed_cursor(
            &found,
            &[row(true, &[(peer("MacBook Air"), false)])],
            0,
            Some(0),
            0,
        );
        assert_eq!(gone.submenu_row, None);
    }
}
