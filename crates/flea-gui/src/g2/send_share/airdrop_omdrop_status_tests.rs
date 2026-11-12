use super::*;

#[test]
fn a_failed_status_keeps_what_omdrop_last_confirmed_so_off_stays_possible() {
    let (mut om, _) = opened(ON);
    om.menu_closed();
    let Some(Act::Doctor(generation, _)) = om.menu_opened(1).into_iter().next() else {
        panic!("each open asks Omdrop's status");
    };
    assert_eq!(om.status_done(generation, 1, "", 1), [Act::Refresh]);
    assert_eq!(om.phase, Phase::On);
    let page = om.page("/d", "h");
    assert_eq!(
        page.adapter.readiness,
        Readiness::NotReady,
        "unknown, not ready and not off"
    );
    assert!(
        page.receiving && page.folder == "/home/u/Inbox",
        "the confirmed receiver and folder stand"
    );
    assert!(
        started(&om.receiving_off(), Proc::Radio).is_some(),
        "the running receiver can still be turned off"
    );
    let mut fresh = Omdrop::default();
    let _ = fresh.facts("/usr/bin/omdrop");
    let Some(Act::Doctor(generation, _)) = fresh.ask_status().into_iter().next() else {
        panic!()
    };
    fresh.status_done(generation, 124, "", 0);
    assert_eq!(
        fresh.adapter.readiness,
        Readiness::NotReady,
        "no answer yet"
    );
}

#[test]
fn receiving_off_while_omdrop_on_runs_is_carried_out_once_it_is_confirmed() {
    let i18n = english();
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    assert_eq!(om.receiving_off(), [Act::Refresh]);
    assert!(
        !om.page("/d", "h").receiving,
        "the page shows the Off chosen"
    );
    let ended = om.ended(Proc::Radio, radio, 0, 0, &i18n);
    let (_, argv) = started(&confirm(&mut om, &ended, ON, 0), Proc::Radio)
        .expect("the Off chosen meanwhile runs once on is confirmed");
    assert_eq!(argv, ["/usr/bin/omdrop", "off"]);
    // Choosing on again before the confirmation withdraws that Off.
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.receiving_off();
    om.turn_on(&i18n);
    let ended = om.ended(Proc::Radio, radio, 0, 0, &i18n);
    assert!(started(&confirm(&mut om, &ended, ON, 0), Proc::Radio).is_none());
    // A status that fails while confirming keeps the Off waiting for the next answer.
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.receiving_off();
    let ended = om.ended(Proc::Radio, radio, 0, 0, &i18n);
    let Some(Act::Doctor(failed, _)) = ended.iter().find(|a| matches!(a, Act::Doctor(..))).cloned()
    else {
        panic!("{ended:?}")
    };
    assert_eq!(om.status_done(failed, 124, "", 0), [Act::Refresh]);
    let retry = om.ask_status();
    assert!(
        started(&confirm(&mut om, &retry, ON, 0), Proc::Radio).is_some(),
        "the Off runs once an answer comes"
    );
    // An `on` that failed leaves nothing to turn off, and an Off chosen twice runs once.
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.receiving_off();
    om.receiving_off();
    let ended = om.ended(Proc::Radio, radio, 1, 0, &i18n);
    assert!(started(&confirm(&mut om, &ended, OFF, 0), Proc::Radio).is_none());
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.receiving_off();
    om.receiving_off();
    let ended = om.ended(Proc::Radio, radio, 0, 0, &i18n);
    let acts = confirm(&mut om, &ended, ON, 0);
    assert_eq!(
        acts.iter()
            .filter(|a| matches!(a, Act::Start(Proc::Radio, ..)))
            .count(),
        1
    );
    // A second Off while off still runs is not repeated after a refusal.
    let (mut om, _) = opened(ON);
    let (off, _) = started(&om.receiving_off(), Proc::Radio).unwrap();
    assert!(om.receiving_off().is_empty());
    let ended = om.ended(Proc::Radio, off, 1, 0, &i18n);
    assert!(started(&confirm(&mut om, &ended, ON, 0), Proc::Radio).is_none());
}

#[test]
fn a_status_asked_before_a_new_folder_does_not_put_the_old_folder_back() {
    let (mut om, _) = opened(ON);
    om.menu_closed();
    let Some(Act::Doctor(stale, _)) = om.menu_opened(1).into_iter().next() else {
        panic!()
    };
    om.folder_argv("/home/u/New");
    assert!(
        om.status_done(stale, 0, ON, 2).is_empty(),
        "the old answer is dropped"
    );
    assert_eq!(om.page("/d", "h").folder, "/home/u/New");
    assert!(
        matches!(om.ask_status()[..], [Act::Doctor(..)]),
        "a fresh status is asked once omdrop dir returns"
    );
}
