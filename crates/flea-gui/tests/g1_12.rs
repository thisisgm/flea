//! G1.12: the breadcrumb, the path entry and its suggestions, with every crumbs.js and pathbar.js case ported.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
use flea_gui::path::view::Inputs;
use std::fmt::Debug;

#[path = "g1_12/allocations.rs"]
mod allocations;
#[path = "g1_12/crumbs.rs"]
mod crumbs;
#[path = "g1_12/entry.rs"]
mod entry;
#[path = "g1_12/ey174.rs"]
mod ey174;
#[path = "g1_12/history.rs"]
mod history;
#[path = "g1_12/ipc.rs"]
mod ipc;
#[path = "g1_12/jump.rs"]
mod jump;
#[path = "g1_12/locale.rs"]
mod locale;
#[path = "g1_12/parity.rs"]
mod parity;
#[path = "g1_12/pathbar.rs"]
mod pathbar;
#[path = "g1_12/rank.rs"]
mod rank;
#[path = "g1_12/readers.rs"]
mod readers;
#[path = "g1_12/rects.rs"]
mod rects;
#[path = "g1_12/scale.rs"]
mod scale;
#[path = "g1_12/strip.rs"]
mod strip;
#[path = "g1_12/view.rs"]
mod view;
#[path = "g1_12/wide.rs"]
mod wide;
#[path = "g1_12/wire.rs"]
mod wire;

pub const HOME: &str = "/home/gm";

/// One ported JS check, named by its source line so a failure reads as the row it covers.
#[track_caller]
pub fn js<T: PartialEq + Debug>(source: &str, line: u32, got: T, want: T) {
    assert_eq!(got, want, "tests/js/{source}:{line}");
}

/// What the window hands the path surface for one pane showing path under HOME.
pub fn inputs(path: &str) -> Inputs {
    Inputs {
        paths: [path.to_owned().into(), "/".into()],
        home: HOME.into(),
        first: 40,
        ..Inputs::default()
    }
}
