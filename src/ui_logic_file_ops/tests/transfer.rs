// Ported checks from tests/js/transfer.js and tests/transferhold.qml; the ledger names each oracle line.
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::transfer::Transfer;
use crate::ui_logic_file_ops::{joined, Part};

const GB: u64 = 1_000_000_000;
pub(super) const MB: u64 = 1_000_000;

// The runs of a byte line joined with a bar, the way the oracle's drawn() prints them.
fn drawn(parts: &[Part]) -> String {
    parts
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

// One letter per run: f for a figure, w for a word.
fn inked(parts: &[Part]) -> String {
    parts
        .iter()
        .map(|p| if p.figure { 'f' } else { 'w' })
        .collect()
}

// The figure flag of each run, in order.
fn figures(parts: &[Part]) -> Vec<bool> {
    parts.iter().map(|p| p.figure).collect()
}

// One regular file of 1 GB out of 2 GB.
fn one_file() -> Transfer {
    Transfer {
        id: 1,
        n: 1,
        name: "big.bin".into(),
        bytes: GB,
        total: 2 * GB,
        ..Default::default()
    }
}

// A batch of 21 items with a running sum and no total of its own.
fn many() -> Transfer {
    Transfer {
        id: 2,
        n: 21,
        index: 3,
        name: "plate-17.raw".into(),
        done: 3,
        bytes: 12 * MB,
        total: 94 * MB,
        moved: 300 * MB,
        ..Default::default()
    }
}

// One directory: n of 1, no total of its own, the sweep says 8.4 GB.
fn one_tree() -> Transfer {
    Transfer {
        id: 5,
        n: 1,
        name: "captures".into(),
        bytes: 300 * MB,
        scanned: 8400 * MB,
        ..Default::default()
    }
}

// A five-item copy with item four in flight and the bytes of the item sampled.
fn copying() -> Transfer {
    Transfer {
        id: 12,
        n: 5,
        index: 4,
        name: "clip-05.mp4".into(),
        done: 4,
        bytes: 1_300_000_000,
        total: 1_300_000_000,
        moved: 2_100_000_000,
        scanned: 3_400_000_000,
        running: true,
        ..Default::default()
    }
}

// A four-item transfer with item index as the one in flight and no samples yet.
pub(super) fn open_at(index: u64) -> Transfer {
    Transfer {
        id: 12,
        n: 4,
        index,
        done: index,
        scanned: 3000,
        running: true,
        ..Default::default()
    }
}

#[test]
fn byte_line_names_what_moved_of_what_and_time_left() {
    let cat = Catalog::english();
    let parts = one_file().byte_parts(&cat.i18n, MB);
    // oracle tests/js/transfer.js:22
    assert_eq!(
        drawn(&parts),
        "1.0 GB| of |2.0 GB| · |1.0 MB/s| · |16:40| left"
    );
    // oracle tests/js/transfer.js:25
    assert_eq!(inked(&parts), "fwfwfwfw");
    // oracle tests/js/transfer.js:30
    assert_eq!(
        drawn(&one_file().byte_parts(&cat.i18n, 0)),
        "1.0 GB| of |2.0 GB| · |0 B/s"
    );
}

#[test]
fn many_items_state_only_what_they_know() {
    let cat = Catalog::english();
    let parts = many().byte_parts(&cat.i18n, MB);
    // oracle tests/js/transfer.js:36
    assert!(
        joined(&parts) == "312.0 MB copied · 1.0 MB/s" && figures(&parts) == [true, false, true],
        "{parts:?}"
    );
    let moved = Transfer {
        moving: true,
        ..many()
    }
    .byte_parts(&cat.i18n, MB);
    // oracle tests/js/transfer.js:38
    assert!(
        joined(&moved) == "312.0 MB moved · 1.0 MB/s" && figures(&moved) == [true, false, true],
        "{moved:?}"
    );
    // oracle tests/js/transfer.js:45
    assert!(
        joined(&parts) == "312.0 MB copied · 1.0 MB/s" && figures(&parts) == [true, false, true],
        "{parts:?}"
    );
}

#[test]
fn settled_batch_names_total_and_time_left() {
    let cat = Catalog::english();
    let settled = Transfer {
        scanned: 8400 * MB,
        ..many()
    };
    // oracle tests/js/transfer.js:47
    assert_eq!(
        drawn(&settled.byte_parts(&cat.i18n, MB)),
        "312.0 MB| of |8.4 GB| · |1.0 MB/s| · |2:14:48| left"
    );
    // oracle tests/js/transfer.js:50
    assert_eq!(
        drawn(&settled.byte_parts(&cat.i18n, 0)),
        "312.0 MB| of |8.4 GB| · |0 B/s"
    );
    // oracle tests/js/transfer.js:54
    assert_eq!(
        settled.sampled(4, "plate-18.raw", 2 * MB, 0, 0).scanned,
        8400 * MB
    );
    // oracle tests/js/transfer.js:55
    assert_eq!(many().sampled(4, "x", 1, 0, 77).scanned, 77);
}

#[test]
fn total_arriving_late_joins_the_line_then() {
    let cat = Catalog::english();
    let early = many().sampled(3, "one.bin", 100 * MB, 0, 0);
    // oracle tests/js/transfer.js:60
    assert_eq!(
        joined(&early.byte_parts(&cat.i18n, MB)),
        "400.0 MB copied · 1.0 MB/s"
    );
    let late = early.sampled(3, "one.bin", 100 * MB, 0, 8400 * MB);
    // oracle tests/js/transfer.js:64
    assert_eq!(
        joined(&late.byte_parts(&cat.i18n, MB)),
        "400.0 MB of 8.4 GB · 1.0 MB/s · 2:13:20 left"
    );
}

#[test]
fn one_directory_takes_its_own_scan_as_total() {
    let cat = Catalog::english();
    // oracle tests/js/transfer.js:72
    assert_eq!(
        drawn(&one_tree().byte_parts(&cat.i18n, MB)),
        "300.0 MB| of |8.4 GB| · |1.0 MB/s| · |2:15:00| left"
    );
    let outran = Transfer {
        bytes: 9000 * MB,
        ..one_tree()
    };
    // oracle tests/js/transfer.js:76
    assert_eq!(
        drawn(&outran.byte_parts(&cat.i18n, MB)),
        "9.0 GB| of |8.4 GB| · |1.0 MB/s| · |0:00| left"
    );
    // oracle tests/js/transfer.js:81
    assert_eq!(
        Transfer {
            id: 3,
            n: 1,
            name: "captures".into(),
            ..Default::default()
        }
        .byte_parts(&cat.i18n, 0)
        .len(),
        0
    );
}

#[test]
fn finished_items_add_what_the_wire_named() {
    let after_file = Transfer {
        bytes: 2 * GB,
        ..one_file()
    }
    .item_done(0, "big.bin");
    // oracle tests/js/transfer.js:87
    assert_eq!(after_file.moved_bytes(), 2 * GB);
    let tree = Transfer {
        id: 4,
        n: 2,
        name: "captures".into(),
        bytes: 700 * MB,
        ..Default::default()
    };
    // oracle tests/js/transfer.js:91
    assert_eq!(tree.item_done(0, "captures").moved_bytes(), 700 * MB);
    // oracle tests/js/transfer.js:93
    assert_eq!(
        tree.item_done(0, "captures")
            .item_done(1, "empty")
            .moved_bytes(),
        700 * MB
    );
}

#[test]
fn file_line_never_names_the_running_count() {
    let cat = Catalog::english();
    let tree = Transfer {
        name: "captures".into(),
        total: 0,
        bytes: 700 * MB,
        ..Default::default()
    };
    // oracle tests/js/transfer.js:98
    assert_eq!(tree.file_line(&cat.i18n), "captures");
}

#[test]
fn extract_names_its_verb_and_draws_no_bytes() {
    let cat = Catalog::english();
    let extracting = Transfer {
        id: 9,
        n: 1,
        name: "slow.zip".into(),
        running: true,
        extract: true,
        ..Default::default()
    };
    // oracle tests/js/transfer.js:104
    assert_eq!(extracting.head(&cat.i18n), "Extracting");
    // oracle tests/js/transfer.js:105
    assert_eq!(extracting.file_line(&cat.i18n), "slow.zip");
    // oracle tests/js/transfer.js:106
    assert_eq!(extracting.byte_parts(&cat.i18n, 0).len(), 0);
    // oracle tests/js/transfer.js:107
    assert_eq!(extracting.fraction(), 0.0);
}

#[test]
fn bar_follows_the_bytes_against_the_sweep_total() {
    let cat = Catalog::english();
    let one_folder =
        Transfer::started(3, false, 1, false).sampled(0, "Photos", 1_200_000_000, 0, 3_400_000_000);
    // oracle tests/js/transfer.js:113
    assert_eq!((one_folder.fraction() * 1000.0).round() / 1000.0, 0.353);
    let two_folders = Transfer {
        moved: 2_000_000_000,
        ..Transfer::started(4, false, 2, false).sampled(1, "Music", 720_000_000, 0, 3_400_000_000)
    };
    // oracle tests/js/transfer.js:117
    assert_eq!(two_folders.fraction(), 0.8);
    // oracle tests/js/transfer.js:119
    assert!(joined(&one_folder.byte_parts(&cat.i18n, 0)).contains("3.4 GB"));
    // oracle tests/js/transfer.js:121
    assert_eq!(one_file().head(&cat.i18n), "Copying 1 of 1");
    let big = Transfer {
        n: 1204,
        index: 203,
        ..Default::default()
    };
    // oracle tests/js/transfer.js:122
    assert_eq!(big.head(&cat.i18n), "Copying 204 of 1,204");
}

#[test]
fn final_flush_headlines_the_drive_and_keeps_its_bytes() {
    let cat = Catalog::english();
    let flushed = copying().item_done(4, "clip-05.mp4").mark_writing("128GB");
    // oracle tests/js/transfer.js:131
    assert_eq!(flushed.head(&cat.i18n), "Writing to 128GB");
    // oracle tests/js/transfer.js:133
    assert_eq!(
        copying()
            .item_done(4, "clip-05.mp4")
            .mark_writing("")
            .head(&cat.i18n),
        "Writing to the drive"
    );
    // oracle tests/js/transfer.js:136
    assert_eq!(flushed.file_line(&cat.i18n), "");
    // oracle tests/js/transfer.js:138
    assert_eq!(
        drawn(&flushed.byte_parts(&cat.i18n, MB)),
        "3.4 GB| copied · waiting for the drive"
    );
    let moved = Transfer {
        moving: true,
        ..copying()
    }
    .item_done(4, "clip-05.mp4")
    .mark_writing("128GB");
    // oracle tests/js/transfer.js:141
    assert_eq!(
        drawn(&moved.byte_parts(&cat.i18n, MB)),
        "3.4 GB| moved · waiting for the drive"
    );
    // oracle tests/js/transfer.js:145
    assert_eq!(flushed.fraction(), 1.0);
    let paced = Transfer {
        id: 12,
        n: 5,
        index: 3,
        name: "clip-04.mp4".into(),
        done: 4,
        scanned: 3_400_000_000,
        running: true,
        ..Default::default()
    }
    .item_done(4, "clip-05.mp4");
    let closed = paced.sampled(4, "clip-05.mp4", 3_400_000_000, 0, 0);
    // oracle tests/js/transfer.js:151
    assert_eq!(closed.done, 5);
    // oracle tests/js/transfer.js:152
    assert_eq!(closed.moved_bytes(), 3_400_000_000);
    // oracle tests/js/transfer.js:153
    assert_eq!(
        drawn(&closed.mark_writing("128GB").byte_parts(&cat.i18n, MB)),
        "3.4 GB| copied · waiting for the drive"
    );
    // oracle tests/js/transfer.js:156
    assert!(flushed.running);
    // oracle tests/js/transfer.js:158
    assert!(!flushed.cancel_enabled());
    // oracle tests/js/transfer.js:160
    assert!(copying().cancel_enabled());
    // oracle tests/js/transfer.js:162
    assert!(!Transfer::default().cancel_enabled());
    // oracle tests/js/transfer.js:164
    assert_eq!(
        format!(
            "{}|{}|{}|{}",
            flushed.moved, flushed.bytes, flushed.total, flushed.name
        ),
        "3400000000|0|0|"
    );
}
