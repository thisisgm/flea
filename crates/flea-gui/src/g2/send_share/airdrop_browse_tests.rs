use super::*;

// Radio up and a menu open: the browse it started, at time zero.
fn browsing() -> (AirDrop, u64) {
    let i18n = english();
    let mut air = window(None);
    opened(&mut air, 0);
    let radio = start(&on(&mut air, &i18n), Proc::Radio).unwrap();
    let acts = air.line(Proc::Radio, radio, UP, 0, &i18n);
    (air, start(&acts, Proc::Browse).unwrap())
}

#[test]
fn a_browse_that_exits_0_before_its_seconds_waits_for_the_next_menu_open_a_gap_on() {
    let i18n = english();
    let (mut air, browse) = browsing();
    assert!(
        air.ended(Proc::Browse, browse, 0, 5, &i18n).is_empty(),
        "no restart"
    );
    air.menu_closed();
    assert!(
        start(&opened(&mut air, 5 + BROWSE_GAP_MS - 1), Proc::Browse).is_none(),
        "a menu opened at once does not start another"
    );
    air.menu_closed();
    assert!(start(&opened(&mut air, 5 + BROWSE_GAP_MS), Proc::Browse).is_some());
}

#[test]
fn a_browse_that_ran_its_whole_seconds_listens_again_at_once() {
    let i18n = english();
    let (mut air, browse) = browsing();
    let whole = owly::BROWSE_SECS * 1_000;
    assert!(start(
        &air.ended(Proc::Browse, browse, 0, whole, &i18n),
        Proc::Browse
    )
    .is_some());
}
