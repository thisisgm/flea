//! FIX-MENUS: the menu model rows the GPUI fixes feed, Flash to drive on a zip (EY-488), Taildrop that cannot run (EY-185) and its peer mark (EY-537).
use super::{
    build::listing_entries,
    testkit::{entry, state},
    Context,
};

// Ops.js noteZip arms Flash to drive on a zip only through its one image member, as Menu.js isDiskImage reads it.
#[test]
fn fixmenus_ey488_a_zip_offers_flash_to_drive_only_with_its_image_member() {
    let zip = |member: &str| Context {
        row_name: "archive-iso.zip".into(),
        flash_zip_member: member.into(),
        ..state()
    };
    assert!(entry(&listing_entries(&zip("archive-iso.iso")), "flashDrive").is_some());
    assert!(entry(&listing_entries(&zip("")), "flashDrive").is_none());
    assert!(entry(&listing_entries(&zip("notes.txt")), "flashDrive").is_none());
}

// Menu.js submenuGlyph: each Taildrop peer row draws the server mark, as v0.3.12 does before the machine's name.
#[test]
fn fixmenus_ey537_a_taildrop_peer_row_draws_the_server_mark() {
    let rows = listing_entries(&Context {
        taildrop_peers: vec![super::build::Peer {
            id: "macbookair".into(),
            label: "macbookair".into(),
        }],
        ..state()
    });
    let peers = entry(&rows, "taildrop")
        .and_then(|row| row.submenu.as_ref())
        .expect("the Taildrop row has its peer flyout");
    assert_eq!(
        peers.iter().map(|peer| peer.glyph).collect::<Vec<_>>(),
        [Some("server")]
    );
}

// GM 2026-10-29: a Taildrop that cannot answer keeps its row, red and disabled, with no peers to offer.
#[test]
fn fixmenus_ey185_an_installed_taildrop_with_nothing_to_offer_stays_red() {
    let rows = listing_entries(&Context {
        taildrop_peers: Vec::new(),
        taildrop_refreshing: false,
        ..state()
    });
    let row = entry(&rows, "taildrop").expect("the Taildrop row stays");
    assert!(row.disabled && row.errored, "{row:?}");
}
