//! Rows filled from an outside list: Taildrop peers on a folder row and Open With's application catalogue.
use super::build::listing_entries;
use super::build_tests::st;
use super::testkit::*;

#[test]
fn a_folder_row_offers_no_taildrop_peers() {
    // tests/js/menu.js "cold folder listing suppresses file-only peers": a folder takes no Taildrop peer.
    let rows = listing_entries(&st(|p| p.row_is_file = false));
    let taildrop = rows
        .iter()
        .find(|e| e.action == "taildrop")
        .expect("Taildrop installed draws its row");
    check(
        "cold folder listing suppresses file-only peers",
        taildrop.submenu.as_ref().map_or(0, Vec::len),
        0,
    );
    check("folder Taildrop row is dimmed", taildrop.disabled, true);
    // GM 2026-10-29: it reads red with no state word, so the menu keeps its size.
    check(
        "folder Taildrop row is red and says no word",
        (taildrop.errored, taildrop.hint.is_none()),
        (true, true),
    );
}

// An empty application list takes Open With away only once the catalogue answered; one past the deadline keeps it.
#[test]
fn open_with_leaves_only_on_an_answered_empty_catalogue() {
    let answered = st(|p| p.open_with_loaded = true);
    check(
        "an answered empty catalogue takes Open With away",
        entry(&listing_entries(&answered), "openWith").is_none(),
        true,
    );
    let late = listing_entries(&st(|p| {
        p.open_with_loaded = true;
        p.open_with_late = true;
    }));
    check(
        "a catalogue past the deadline keeps it, with Another Application alone",
        entry(&late, "openWith")
            .and_then(|e| e.submenu.as_deref())
            .map(|leaves| {
                leaves
                    .iter()
                    .map(|leaf| leaf.id.to_string())
                    .collect::<Vec<_>>()
            }),
        Some(vec![super::rows::OPEN_WITH_OTHER.to_string()]),
    );
}
