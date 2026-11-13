//! Provisional display bytes never satisfy or outlive their replacement walk.
use super::*;

fn folder(name: &str, source: Option<&str>) -> RowFacts {
    RowFacts {
        name: name.into(),
        source: source.map(str::to_owned),
        dir: true,
        ..Default::default()
    }
}

#[test]
fn fxsizeblink_identity_keeps_renamed_folder_but_rejects_a_replacement() {
    let old = [
        folder("alpha", Some("1:2:16384")),
        folder("beta", Some("1:3:16384")),
    ];
    let new = [
        folder("beta", Some("1:4:16384")),
        folder("renamed", Some("1:2:16384")),
    ];
    let mut sizes = DirSizes::default();
    sizes.remember(
        20,
        Size {
            bytes: 42,
            partial: false,
        },
        240,
    );
    sizes.remember(
        21,
        Size {
            bytes: 99,
            partial: false,
        },
        240,
    );
    let carried = sizes.relisted(&old, 20, &new, 80, 240);
    assert_eq!(carried.size_for(80), None, "a replacement is a new row");
    assert_eq!(carried.size_for(81).map(|size| size.bytes), Some(42));
    assert_eq!(carried.plan(&new, 80, 80, 81), [80, 81]);
}

#[test]
fn fxsizeblink_a_resort_carry_never_turns_provisional_bytes_into_answers() {
    let rows = [folder("alpha", None), folder("beta", None)];
    let answer = Size {
        bytes: 42,
        partial: false,
    };
    let mut sizes = DirSizes::default();
    sizes.remember(0, answer, 240);
    sizes.remember(1, answer, 240);
    let mut next = sizes.relisted(&rows, 0, &rows, 0, 240);
    next.applied(&[1]);
    next.remember(
        0,
        Size {
            bytes: 64,
            partial: false,
        },
        240,
    );
    let resorted = next.carried(&rows, 0, &rows, 0);
    assert_eq!(resorted.size_for(0).map(|size| size.bytes), Some(64));
    assert_eq!(resorted.size_for(1), None, "beta's walk never answered");
    assert_eq!(resorted.plan(&rows, 0, 0, 1), [1]);
}

#[test]
fn fxsizeblink_cancellation_drops_provisional_bytes_and_partial_failure_replaces_them() {
    let rows = [folder("alpha", None), folder("beta", None)];
    let answer = Size {
        bytes: 42,
        partial: false,
    };
    let mut sizes = DirSizes::default();
    sizes.remember(0, answer, 240);
    sizes.remember(1, answer, 240);
    let mut next = sizes.relisted(&rows, 0, &rows, 0, 240);
    next.applied(&[0, 1]);
    assert_eq!(next.size_for(0), Some(answer));
    next.cancel_rows(&[0]);
    assert_eq!(next.size_for(0), None);
    assert_eq!(next.size_for(1), Some(answer));
    let failed = Size {
        bytes: 0,
        partial: true,
    };
    next.remember(1, failed, 240);
    assert_eq!(
        next.size_for(1),
        Some(failed),
        "failure cannot retain the old exact size"
    );
    next.cancelled();
    assert_eq!(
        next.size_for(1),
        Some(failed),
        "a completed partial result remains explicit"
    );
    let mut pending = sizes.relisted(&rows, 0, &rows, 0, 240);
    pending.applied(&[0, 1]);
    let mut again = pending.relisted(&rows, 0, &rows, 0, 240);
    assert_eq!(again.size_for(0), Some(answer));
    again.cancelled();
    assert_eq!(
        again.size_for(0),
        None,
        "full cancellation forgets provisional display bytes"
    );
}

#[test]
fn fxsizeblink_inline_answers_replace_provisional_display_without_another_walk() {
    let rows = [folder("alpha", None)];
    let mut sizes = DirSizes::default();
    sizes.remember(
        0,
        Size {
            bytes: 42,
            partial: false,
        },
        240,
    );
    let mut next = sizes.relisted(&rows, 0, &rows, 0, 240);
    let fresh = [RowFacts {
        inline: Some(84),
        ..folder("alpha", None)
    }];
    next.inlined(&fresh, 0, 240);
    assert_eq!(next.size_for(0).map(|size| size.bytes), Some(84));
    assert!(next.plan(&fresh, 0, 0, 0).is_empty());
}

#[test]
fn fxsizeblink_size_carry_stays_bounded_to_240_answers_in_a_large_listing() {
    let rows: Vec<_> = (0..320)
        .map(|index| folder(&format!("d{index}"), None))
        .collect();
    let mut sizes = DirSizes::default();
    for row in 0..320 {
        sizes.remember(
            row,
            Size {
                bytes: row as u64,
                partial: false,
            },
            240,
        );
    }
    let next = sizes.relisted(&rows, 0, &rows, 0, 240);
    assert_eq!(next.previous.len(), 240);
    assert!(next.file.is_empty());
    assert_eq!(next.plan(&rows, 0, 80, 90), (80..=90).collect::<Vec<_>>());
}

#[test]
fn fxsizeblink_offscreen_provisional_answers_cannot_evict_the_returned_viewport() {
    let rows: Vec<_> = (0..320)
        .map(|index| folder(&format!("d{index}"), None))
        .collect();
    let answer = Size {
        bytes: 42,
        partial: false,
    };
    let mut sizes = DirSizes::default();
    for row in 0..240 {
        sizes.remember(row, answer, 240);
    }
    let mut next = sizes.relisted(&rows, 0, &rows, 0, 240);
    let visible: Vec<_> = (0..100).collect();
    next.retain_previous(&visible);
    next.applied(&visible);
    for row in visible {
        next.remember(row, answer, 240);
    }
    // Scroll to a different held screen, then return to the first before the next re-read.
    for row in 240..320 {
        next.remember(row, answer, 240);
    }
    let again = next.relisted(&rows, 0, &rows, 0, 240);
    for row in 0..100 {
        assert_eq!(again.size_for(row), Some(answer), "returned row {row}");
    }
}
