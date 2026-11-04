//! The secondary lane, byte total and activity tests, split from the status tests.
use crate::ui_logic_fb1::status::{
    activity_changed, cancel_activity, displayed_starting, secondary_text, selection_bytes,
    Activity, RowSize, Walked,
};

#[test]
fn the_secondary_lane_joins_its_parts_in_the_shipped_order() {
    assert_eq!(
        secondary_text("z undoes", false, false, "", &[], false, "", ""),
        " · z undoes"
    );
    assert_eq!(secondary_text("", false, false, "", &[], false, "", ""), "");
    assert_eq!(
        secondary_text(
            "esc dismisses · z undoes",
            true,
            false,
            "",
            &[],
            false,
            "",
            ""
        ),
        " · esc dismisses · z undoes"
    );
    assert_eq!(
        secondary_text(
            "esc dismisses",
            true,
            true,
            "Converting 1 of 3",
            &[],
            false,
            "",
            ""
        ),
        " · esc dismisses · Converting 1 of 3"
    );
    assert_eq!(
        secondary_text("", false, true, "", &[], true, "3 found", ""),
        " · 3 found"
    );
    assert_eq!(
        secondary_text(
            "",
            false,
            false,
            "",
            &[],
            false,
            "",
            "c.txt selected for retry"
        ),
        " · c.txt selected for retry"
    );
    assert_eq!(
        secondary_text(
            "",
            true,
            false,
            "",
            &[],
            false,
            "",
            "c.txt selected for retry"
        ),
        ""
    );
    let queued = [
        Activity {
            owner: 1,
            text: "Copying 1 of 5".into(),
            ..Activity::default()
        },
        Activity {
            owner: 2,
            text: "Moving 1 of 3".into(),
            ..Activity::default()
        },
    ];
    assert_eq!(
        secondary_text("", false, true, "Copying 1 of 5", &queued, false, "", ""),
        " · Moving 1 of 3"
    );
}

#[test]
fn the_byte_total_needs_every_selected_row_held_and_measured() {
    let rows = [
        RowSize {
            dir: false,
            bytes: 1000,
        },
        RowSize {
            dir: false,
            bytes: 2000,
        },
        RowSize {
            dir: true,
            bytes: 4096,
        },
        RowSize {
            dir: false,
            bytes: 4000,
        },
    ];
    let finished = |index| {
        (index == 2).then_some(Walked {
            bytes: 8000,
            partial: false,
        })
    };
    let partial = |index| {
        (index == 2).then_some(Walked {
            bytes: 8000,
            partial: true,
        })
    };
    let nothing = |_| None;
    assert_eq!(selection_bytes(&[0, 1], 0, &rows, finished), Some(3000));
    assert_eq!(selection_bytes(&[0, 2], 0, &rows, finished), Some(9000));
    assert_eq!(selection_bytes(&[0, 2], 0, &rows, partial), None);
    assert_eq!(selection_bytes(&[0, 2], 0, &rows, nothing), None);
    assert_eq!(selection_bytes(&[0, 9], 0, &rows, finished), None);
    assert_eq!(selection_bytes(&[0, 1, 2, 3, 4], 0, &rows, finished), None);
    assert_eq!(selection_bytes(&[], 0, &rows, finished), None);
    assert_eq!(
        selection_bytes(&[100, 101], 100, &rows, nothing),
        Some(3000)
    );
    assert_eq!(selection_bytes(&[99], 100, &rows, nothing), None);
}

#[test]
fn activities_keep_their_owner_and_cancellation() {
    let (left, right) = (1, 2);
    let mut activities = Vec::new();
    activity_changed(&mut activities, left, "Copying 1 of 5", 1, true, false);
    assert!(cancel_activity(&mut activities));
    assert!(activities[0].cancelling);
    let before = activities.clone();
    assert!(!cancel_activity(&mut activities));
    assert_eq!(activities, before);
    activity_changed(&mut activities, right, "Moving 1 of 3", 1, true, false);
    assert_eq!(activities[0].owner, left);
    assert!(!activities[1].cancelling);
    activity_changed(&mut activities, left, "Copying 2 of 5", 1, true, false);
    assert!(activities[0].cancelling);
    activity_changed(&mut activities, right, "", 0, false, false);
    assert_eq!(activities[0].owner, left);
    activity_changed(&mut activities, right, "Moving 1 of 2", 2, true, false);
    activity_changed(&mut activities, left, "", 0, false, false);
    assert_eq!(activities[0].owner, right);
    assert!(!activities[0].cancelling);
    cancel_activity(&mut activities);
    activity_changed(&mut activities, right, "Moving 1 of 1", 3, true, false);
    assert!(!activities[0].cancelling);
    activity_changed(&mut activities, right, "", 0, false, false);
    assert!(activities.is_empty());
    assert!(!cancel_activity(&mut activities));
}

#[test]
fn only_the_displayed_starting_line_lights_the_busy_mark() {
    let mut starting = Vec::new();
    activity_changed(&mut starting, 1, "Starting Pixel 8", 0, false, true);
    assert!(displayed_starting(&starting));
    let mut behind = vec![Activity {
        owner: 2,
        text: "Copying 1 of 5".into(),
        running: true,
        ..Activity::default()
    }];
    behind.extend(starting.clone());
    assert!(!displayed_starting(&behind));
    assert!(!displayed_starting(&[]));
    let mut plain = Vec::new();
    activity_changed(&mut plain, 1, "Starting Pixel 8", 0, false, false);
    assert!(!displayed_starting(&plain));
}
