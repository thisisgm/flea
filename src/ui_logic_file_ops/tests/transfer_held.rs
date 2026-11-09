// Ported checks from tests/js/transfer.js and tests/transferhold.qml, continued: shares and held lines.
use super::transfer::{open_at, MB};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::transfer::{unit, Fold, Held, Transfer, Wire, BEAT_MS};

#[test]
fn share_is_held_to_the_bar_ends() {
    // null and the string "0.4" have no f64 form, so only the numeric inputs are checked.
    // oracle tests/js/transfer.js:167
    assert_eq!(
        [f64::NAN, -1.0, 2.0, 0.25]
            .map(unit)
            .map(|v| v.to_string())
            .join(","),
        "0,0,1,0.25"
    );
}

#[test]
fn closing_sample_after_skipped_paths_keeps_the_card_where_it_was() {
    let skip_last = open_at(0).item_done(0, "a.jpg").item_done(1, "b.jpg");
    let skip_closed = skip_last.sampled(1, "b.jpg", 700, 0, 0);
    // oracle tests/js/transfer.js:178
    assert_eq!(
        format!(
            "{}|{}|{}",
            skip_closed.done, skip_closed.index, skip_closed.name
        ),
        "2|1|b.jpg"
    );
    // oracle tests/js/transfer.js:180
    assert_eq!(skip_closed.mark_writing("128GB").moved_bytes(), 700);
    let refused = skip_last.item_done(2, "c.jpg");
    let late_owed = refused.sampled(1, "b.jpg", 700, 0, 0);
    // oracle tests/js/transfer.js:184
    assert_eq!(
        format!(
            "{}|{}|{}",
            late_owed.done,
            late_owed.index,
            late_owed.moved_bytes()
        ),
        "3|2|700"
    );
}

#[test]
fn large_file_after_paced_ones_keeps_its_size_and_bar() {
    let cat = Catalog::english();
    let after_small = open_at(0).item_done(0, "a.jpg").item_done(1, "b.jpg");
    let owed_once = after_small.sampled(1, "b.jpg", 300, 0, 0);
    let big_at = owed_once.sampled(2, "big.mkv", 1000 * MB, 4200 * MB, 0);
    // oracle tests/js/transfer.js:191
    assert_eq!(big_at.file_line(&cat.i18n), "big.mkv · 4.2 GB");
    // oracle tests/js/transfer.js:193
    assert_eq!(
        format!("{}|{}|{}", big_at.bytes, big_at.total, big_at.moved_bytes()),
        "1000000000|4200000000|1000000300"
    );
    let big_later = big_at.sampled(2, "big.mkv", 2000 * MB, 4200 * MB, 0);
    // oracle tests/js/transfer.js:196
    assert_eq!(big_later.file_line(&cat.i18n), "big.mkv · 4.2 GB");
}

#[test]
fn thousand_held_lines_fold_into_the_card() {
    let thousand = Transfer {
        id: 7,
        n: 1000,
        scanned: 4_096_000,
        running: true,
        ..Default::default()
    };
    let held: Vec<Held> = (0..1000)
        .map(|i| Held {
            index: i,
            name: format!("f{i}.bin"),
        })
        .collect();
    let folded = thousand.items_done(&held);
    // oracle tests/js/transfer.js:205
    assert_eq!(format!("{}|{}", folded.done, folded.name), "1000|f999.bin");
    // oracle tests/js/transfer.js:207
    assert_eq!(thousand.items_done(&[]), thousand);
}

#[test]
fn two_held_lines_land_as_two_files_done() {
    let sample = Transfer {
        id: 8,
        n: 2,
        name: "a".into(),
        bytes: 40,
        total: 100,
        running: true,
        ..Default::default()
    };
    let held = [
        Held {
            index: 0,
            name: "a".into(),
        },
        Held {
            index: 1,
            name: "b".into(),
        },
    ];
    let two = sample.items_done(&held);
    // oracle tests/js/transfer.js:212
    assert_eq!(
        format!("{}|{}|{}|{}", two.done, two.name, two.moved, two.bytes),
        "2|b|100|0"
    );
}

#[test]
fn ok_lines_wait_for_the_beat_then_fold_once() {
    let mut wire = Wire::default();
    wire.started(7, false, 1000, false);
    for i in 0..1000 {
        wire.item(7, i, &format!("f{i}"), true, "", 0);
    }
    // oracle tests/transferhold.qml:91
    assert!(wire.transfer.id == 7);
    // oracle tests/transferhold.qml:93
    assert!(wire.transfer.done == 0);
    let redrew = wire.tick(BEAT_MS);
    // oracle tests/transferhold.qml:98
    assert!(wire.transfer.done == 1000);
    // oracle tests/transferhold.qml:99
    assert!(wire.transfer.name == "f999");
    // oracle tests/transferhold.qml:100
    assert!(redrew && !wire.tick(BEAT_MS * 2));
    // oracle tests/transferhold.qml:102, held at compile time below
}

#[test]
fn held_line_never_overwrites_a_later_sample_or_failure() {
    let mut wire = Wire::default();
    wire.started(8, false, 10, false);
    wire.item(8, 0, "a", true, "", 0);
    wire.progress(8, 1, "b", 50, 100, 0, "", "");
    // oracle tests/transferhold.qml:112
    assert!(wire.transfer.index == 1 && wire.transfer.name == "b");
    wire.item(8, 2, "c", true, "", 0);
    let fold = wire.item(8, 3, "d", false, "denied", 0);
    // oracle tests/transferhold.qml:118
    assert!(
        fold == Fold::Failed {
            name: "d".into(),
            error: "denied".into()
        } && wire.held.is_empty()
    );
    // oracle tests/transferhold.qml:122
    assert!(wire.transfer.done == 4 && wire.transfer.name == "d");
}

#[test]
fn held_line_never_rebuilds_a_card_after_done() {
    let mut wire = Wire::default();
    wire.started(8, false, 10, false);
    wire.item(8, 0, "a", true, "", 0);
    wire.progress(8, 1, "b", 50, 100, 0, "", "");
    wire.item(8, 2, "c", true, "", 0);
    wire.item(8, 3, "d", false, "denied", 0);
    wire.item(8, 4, "e", true, "", 0);
    wire.done(8, 1);
    // oracle tests/transferhold.qml:128
    assert!(wire.transfer.id == 0);
    // oracle tests/transferhold.qml:132
    assert!(wire.transfer.id == 0 && wire.transfer.done == 0 && !wire.tick(BEAT_MS * 5));
}

#[test]
fn held_line_of_one_transfer_never_lands_on_the_next() {
    let mut wire = Wire::default();
    wire.started(9, false, 5, false);
    wire.item(9, 0, "a", true, "", 0);
    wire.started(10, false, 5, false);
    wire.item(9, 1, "stale", true, "", 0);
    // oracle tests/transferhold.qml:142
    assert!(wire.held.is_empty());
    // oracle tests/transferhold.qml:146
    assert!(wire.transfer.id == 10 && wire.transfer.done == 0 && !wire.tick(BEAT_MS));
    // oracle tests/transferhold.qml:147
    assert!(wire.transfer.done == 0 && wire.transfer.name.is_empty());
}

#[test]
fn failure_reaches_the_card_without_the_beat() {
    let mut wire = Wire::default();
    wire.started(10, false, 5, false);
    let fold = wire.item(10, 0, "x", false, "denied", 0);
    // oracle tests/transferhold.qml:153
    assert!(wire.transfer.done == 1 && wire.transfer.failure_reported);
    // oracle tests/transferhold.qml:154
    assert!(
        fold == Fold::Failed {
            name: "x".into(),
            error: "denied".into()
        }
    );
}

// oracle tests/transferhold.qml:102
const _: () = assert!(BEAT_MS <= 100);
