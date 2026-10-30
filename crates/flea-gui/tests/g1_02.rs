#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use flea::protocol::{Reply, RowFacts};
use flea_gui::pane::{swap::Options, Kind, Move, Pane, ROW_HEIGHT};
use std::rc::Rc;

fn rows(count: usize) -> Vec<RowFacts> {
    (0..count)
        .map(|index| RowFacts {
            name: format!("file-{index}"),
            ..RowFacts::default()
        })
        .collect()
}
fn header(pane: &mut Pane, path: &str, total: usize, now: u64) {
    assert!(!pane
        .receive_at(
            Reply::Listed {
                path: path.into(),
                total
            },
            now
        )
        .unwrap());
}
fn answer(pane: &mut Pane, start: usize, count: usize, listing: usize, now: u64) -> bool {
    pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(count),
            listing,
        },
        now,
    )
    .unwrap()
}
fn settled() -> Pane {
    let mut pane = Pane::new("/fixture/a".into(), 620.);
    header(&mut pane, "/fixture/a", 1000, 1);
    assert!(answer(&mut pane, 0, 320, 1, 1));
    pane
}

#[path = "g1_02/regressions.rs"]
mod regressions;
#[path = "g1_02/requests.rs"]
mod requests;
#[path = "g1_02/snapshots.rs"]
mod snapshots;
#[path = "g1_02/states.rs"]
mod states;
#[path = "g1_02/timing.rs"]
mod timing;
#[path = "g1_02/transport.rs"]
mod transport;

#[path = "g1_02/locale.rs"]
mod locale;
