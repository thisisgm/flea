use flea::keys;
use flea_gui::input::slow::{self, SlowClick, SlowGate, CLEARED};

fn gate() -> SlowGate {
    SlowGate {
        click_rename: true,
        ..SlowGate::default()
    }
}

/// slowclick.js root(): the second tap on the sole selected cursor row, `gap` after the first.
fn armed(gap: u64, gate: SlowGate) -> (SlowClick, bool) {
    let mut slow = SlowClick::default();
    assert!(
        !slow.arm(4, 0, 1000, slow::DOUBLE_CLICK_MS, gate, true),
        "the first tap only selects"
    );
    let arms = slow.arm(4, 0, 1000 + gap, slow::DOUBLE_CLICK_MS, gate, true);
    (slow, arms)
}

#[test]
fn slow_click_arms_after_the_interval_and_fires_only_on_the_still_held_row() {
    assert_eq!(slow::DOUBLE_CLICK_MS, 400);
    assert!(
        !armed(200, gate()).1,
        "a second tap inside the interval is the double click"
    );
    assert!(armed(500, gate()).1);
    assert!(
        !armed(400, gate()).1,
        "exactly at the interval is still the double click"
    );
    let mut moved = SlowClick::default();
    moved.arm(4, 0, 1000, 400, gate(), true);
    assert!(
        !moved.arm(7, 0, 2000, 400, gate(), true),
        "a tap on another row re-arms instead"
    );
    let (mut fired, _) = armed(500, gate());
    assert_eq!(fired.fire(gate(), &[4], 4), Some(4));
    assert_eq!(fired.index, CLEARED);
    let (mut doubled, _) = armed(500, gate());
    doubled.cancel();
    assert_eq!(
        doubled.fire(gate(), &[4], 4),
        None,
        "a cancelled timer renames nothing"
    );
    let (mut left, _) = armed(500, gate());
    assert_eq!(left.fire(gate(), &[5], 5), None);
    let (mut stepped, _) = armed(500, gate());
    assert_eq!(stepped.fire(gate(), &[4], 6), None);
    for blocked in [
        SlowGate {
            renaming: true,
            ..gate()
        },
        SlowGate {
            menu: true,
            ..gate()
        },
        SlowGate {
            drag: true,
            ..gate()
        },
        SlowGate {
            band: true,
            ..gate()
        },
        SlowGate {
            search: true,
            ..gate()
        },
        SlowGate {
            single_click: true,
            ..gate()
        },
        SlowGate {
            click_rename: false,
            ..gate()
        },
    ] {
        let (mut slow, _) = armed(500, gate());
        assert_eq!(
            slow.fire(blocked, &[4], 4),
            None,
            "{blocked:?} blocks the timer"
        );
    }
    let mut modified = SlowClick::default();
    modified.arm(4, 0, 1000, 400, gate(), true);
    assert!(
        !modified.arm(4, keys::CONTROL, 2000, 400, gate(), true),
        "a ctrl tap never arms"
    );
    let mut multi = SlowClick::default();
    multi.arm(4, 0, 1000, 400, gate(), true);
    assert!(!multi.arm(
        4,
        0,
        2000,
        400,
        gate(),
        slow::was_sole_selection(2, true, true)
    ));
    for refused in [
        SlowGate {
            drag: true,
            ..gate()
        },
        SlowGate {
            search: true,
            ..gate()
        },
        SlowGate {
            click_rename: false,
            ..gate()
        },
        SlowGate {
            single_click: true,
            ..gate()
        },
    ] {
        assert!(!armed(1000, refused).1, "{refused:?} never arms");
    }
    let mut first_click = SlowClick::default();
    first_click.arm(4, 0, 1000, 400, gate(), true);
    assert!(
        !first_click.arm(4, 0, 4000, 400, gate(), false),
        "a first click on an unselected row arms nothing"
    );
    assert!(slow::was_sole_selection(1, true, true));
    assert!(!slow::was_sole_selection(1, true, false));
}
