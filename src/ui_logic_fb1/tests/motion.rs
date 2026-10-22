use super::english;
use crate::ui_logic_fb1::{
    out_cubic, reload_begin, reload_line, Begin, OpenRefusal, Reload, CLOSE_MS, DWELL_MS, OPEN_MS,
    TRANSLATE_UP_PX,
};

#[test]
fn structural_motion_is_out_cubic_180_to_reveal_and_140_to_hide() {
    assert_eq!((OPEN_MS, CLOSE_MS, DWELL_MS), (180, 140, 120));
    assert_eq!(TRANSLATE_UP_PX, 10.);
    assert_eq!(
        (out_cubic(0.), out_cubic(1.), out_cubic(2.), out_cubic(-1.)),
        (0., 1., 1., 0.)
    );
    assert_eq!(out_cubic(0.5), 0.875);
    let mut previous = 0.;
    for step in 1..=100 {
        let value = out_cubic(step as f32 / 100.);
        assert!(value >= previous);
        previous = value;
    }
}

#[test]
fn a_reload_counts_items_like_the_status_bar() {
    let catalog = english();
    assert_eq!(reload_line(1, &catalog), "Reloaded · 1 item changed");
    assert_eq!(reload_line(2, &catalog), "Reloaded · 2 items changed");
    assert_eq!(
        reload_line(1200, &catalog),
        "Reloaded · 1,200 items changed"
    );
}

#[test]
fn a_reload_begins_only_where_it_can_and_owes_a_notice_only_for_changed_rows() {
    assert_eq!(reload_begin(true, false, "", "/a", false), Begin::Loading);
    assert_eq!(reload_begin(false, true, "", "/a", false), Begin::Quiet);
    assert_eq!(
        reload_begin(false, false, "/b", "/a", false),
        Begin::OpenWaited("/b".into())
    );
    assert_eq!(reload_begin(false, false, "/a", "/a", false), Begin::Reread);
    assert_eq!(reload_begin(false, false, "/b", "/", true), Begin::Recent);
    let mut reload = Reload::default();
    assert_eq!(reload.landed(5), None);
    reload.arm(6);
    reload.counted(1);
    assert_eq!(reload.landed(6), Some(1));
    assert!(!reload.armed());
    reload.arm(6);
    assert_eq!(reload.landed(4), Some(2));
    reload.arm(6);
    assert_eq!(reload.landed(6), None);
    reload.counted(3);
    assert_eq!(reload.landed(6), None);
    reload.arm(6);
    reload.counted(0);
    assert_eq!(reload.landed(9), None);
}

#[test]
fn open_refusals_are_whole_foreground_sentences() {
    let catalog = english();
    let said: Vec<_> = [
        OpenRefusal::FolderWithFiles,
        OpenRefusal::Archives,
        OpenRefusal::Programs,
        OpenRefusal::NotLoaded,
    ]
    .iter()
    .map(|refusal| catalog.tr(refusal.key()))
    .collect();
    assert_eq!(
        said,
        [
            "A folder can’t be opened together with files.",
            "Archives open one at a time.",
            "Programs open one at a time.",
            "Some marked files haven’t loaded yet.",
        ]
    );
}
