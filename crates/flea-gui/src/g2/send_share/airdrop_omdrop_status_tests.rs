use super::*;
use crate::g2::send_share::airdrop_omdrop::DirDone;

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
    // A status that fails while confirming keeps the Off waiting; the retry its timer asks for runs it.
    let (mut om, token) = failed_with_off_waiting();
    let retry = om.status_due(token);
    assert!(
        started(&confirm(&mut om, &retry, ON, 0), Proc::Radio).is_some(),
        "the Off runs once the retried status answers"
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
fn a_status_asked_before_or_during_a_new_folder_does_not_put_the_old_folder_back() {
    let (mut om, _) = opened(ON);
    om.menu_closed();
    let Some(Act::Doctor(stale, _)) = om.menu_opened(1).into_iter().next() else {
        panic!()
    };
    let job = om.folder_argv("/home/u/New").unwrap().job;
    // Settings shows while `omdrop dir` runs: no status is asked that could read the old folder.
    assert!(om.ask_status().is_empty());
    assert!(om
        .menu_opened(2)
        .iter()
        .all(|a| !matches!(a, Act::Doctor(..))));
    assert!(
        om.status_done(stale, 0, ON, 3).is_empty(),
        "the old answer is dropped"
    );
    assert_eq!(om.page("/d", "h").folder, "/home/u/New");
    let DirDone::Asked(asked) = om.folder_done(job) else {
        panic!("nothing was chosen meanwhile")
    };
    let fresh: Vec<u64> = asked
        .iter()
        .filter_map(|a| match a {
            Act::Doctor(generation, _) => Some(*generation),
            _ => None,
        })
        .collect();
    let [fresh] = fresh[..] else {
        panic!("one fresh status once omdrop dir returns: {fresh:?}")
    };
    assert!(om.ask_status().is_empty(), "and only one");
    let moved = ON.replace("/home/u/Inbox", "/home/u/New");
    om.status_done(fresh, 0, &moved, 4);
    assert_eq!(om.page("/d", "h").folder, "/home/u/New");
}

// Omdrop off; Flea's `on` runs with an Off chosen meanwhile, returns, and the status that would confirm it fails.
fn failed_with_off_waiting() -> (Omdrop, u64) {
    let i18n = english();
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.receiving_off();
    let ended = om.ended(Proc::Radio, radio, 0, 0, &i18n);
    let Some(Act::Doctor(failed, _)) = ended.into_iter().find(|a| matches!(a, Act::Doctor(..)))
    else {
        panic!()
    };
    let acts = om.status_done(failed, 124, "", 0);
    let [Act::Refresh, Act::StatusLater(token, 2_000)] = acts[..] else {
        panic!("{acts:?}")
    };
    (om, token)
}

#[test]
fn a_failing_status_is_retried_after_its_delay_at_most_five_times_in_a_row() {
    let (mut om, mut token) = failed_with_off_waiting();
    let mut retries = 1;
    loop {
        let Some(Act::Doctor(generation, _)) = om.status_due(token).pop() else {
            panic!("the armed retry asks")
        };
        match om.status_done(generation, 124, "", 0)[..] {
            [Act::Refresh, Act::StatusLater(next, 2_000)] => token = next,
            [Act::Refresh] => break,
            ref acts => panic!("{acts:?}"),
        }
        retries += 1;
    }
    assert_eq!(retries, 5);
    assert!(
        om.status_due(token).is_empty(),
        "a spent token asks nothing"
    );
    // Off still waits, unconfirmed and not ready, for the next status a menu or Settings asks.
    assert_eq!(om.adapter.readiness, Readiness::NotReady);
    let asked = om.menu_opened(1);
    assert!(started(&confirm(&mut om, &asked, ON, 1), Proc::Radio).is_some());
}

#[test]
fn a_retry_is_dropped_once_anything_else_asks_withdraws_or_replaces_omdrop() {
    let i18n = english();
    // Another status asked meanwhile, as a menu open does.
    let (mut om, token) = failed_with_off_waiting();
    assert!(matches!(om.menu_opened(1)[..], [Act::Doctor(..), ..]));
    assert!(om.status_due(token).is_empty());
    // On chosen again, withdrawing the Off.
    let (mut om, token) = failed_with_off_waiting();
    om.turn_on(&i18n);
    assert!(om.status_due(token).is_empty());
    // The omdrop running AirDrop replaced.
    let (mut om, token) = failed_with_off_waiting();
    let _ = om.facts("/opt/omdrop");
    assert!(om.status_due(token).is_empty());
    // A new folder chosen: its own status follows `omdrop dir` instead.
    let (mut om, token) = failed_with_off_waiting();
    om.folder_argv("/home/u/New");
    assert!(om.status_due(token).is_empty());
}

#[test]
fn folders_chosen_while_omdrop_dir_runs_wait_and_only_the_latest_runs_next() {
    let (mut om, _) = opened(ON);
    let a = om.folder_argv("/home/u/A").unwrap().job;
    assert!(om.folder_argv("/home/u/B").is_none(), "B waits for A");
    assert!(om.folder_argv("/home/u/C").is_none(), "C replaces B");
    assert_eq!(om.page("/d", "h").folder, "/home/u/C");
    assert!(om.ask_status().is_empty());
    let DirDone::Next(next) = om.folder_done(a) else {
        panic!("the latest choice runs once A returns")
    };
    assert_eq!(next.argv, ["/usr/bin/omdrop", "dir", "/home/u/C"]);
    assert!(
        matches!(om.folder_done(a), DirDone::Asked(ref x) if x.is_empty()),
        "A answers once"
    );
    assert!(om.ask_status().is_empty(), "still none while C runs");
    let DirDone::Asked(asked) = om.folder_done(next.job) else {
        panic!()
    };
    assert!(
        matches!(asked[..], [Act::Doctor(..)]),
        "exactly one fresh status: {asked:?}"
    );
}

// Omdrop off; Flea's `on` returns with nothing chosen meanwhile, and the status that would confirm it fails.
fn failed_confirming_on() -> Omdrop {
    let i18n = english();
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    let ended = om.ended(Proc::Radio, radio, 0, 0, &i18n);
    let Some(Act::Doctor(failed, _)) = ended.into_iter().find(|a| matches!(a, Act::Doctor(..)))
    else {
        panic!()
    };
    let acts = om.status_done(failed, 124, "", 0);
    assert!(
        matches!(acts[..], [Act::Refresh, Act::StatusLater(..)]),
        "a confirmation is retried whatever waits on it: {acts:?}"
    );
    om
}

#[test]
fn an_off_chosen_after_the_confirming_status_failed_asks_for_one_at_once() {
    let mut om = failed_confirming_on();
    let asked = om.receiving_off();
    assert!(
        started(&confirm(&mut om, &asked, ON, 0), Proc::Radio).is_some(),
        "the status it asks confirms on, and the Off runs"
    );
}

#[test]
fn on_chosen_again_over_a_waiting_off_asks_for_a_status_at_once() {
    let i18n = english();
    let (mut om, token) = failed_with_off_waiting();
    let asked = om.turn_on(&i18n);
    assert!(
        om.status_due(token).is_empty(),
        "the armed retry is dropped"
    );
    assert!(started(&confirm(&mut om, &asked, ON, 0), Proc::Radio).is_none());
    assert_eq!(om.phase, Phase::On, "the status ended Confirming");
    let _ = failed_confirming_on();
}

#[test]
fn a_dir_run_from_before_omdrop_went_away_and_came_back_still_runs_alone() {
    let (mut om, _) = opened(ON);
    let a = om.folder_argv("/home/u/A").unwrap().job;
    assert!(om.folder_argv("/home/u/Gone").is_none());
    let _ = om.facts("");
    let _ = om.facts("/usr/bin/omdrop");
    // A still runs against the same folder setting, so B and C wait for it rather than run beside it.
    assert!(om.folder_argv("/home/u/B").is_none(), "B waits for A");
    assert!(om.folder_argv("/home/u/C").is_none(), "C replaces B");
    assert!(
        matches!(om.folder_done(a + 1), DirDone::Asked(ref x) if x.is_empty()),
        "a completion of a run that is not current changes nothing"
    );
    let DirDone::Next(c) = om.folder_done(a) else {
        panic!("A's completion starts C, the latest choice")
    };
    assert_eq!(c.argv, ["/usr/bin/omdrop", "dir", "/home/u/C"]);
    assert!(
        matches!(om.folder_done(a), DirDone::Asked(ref x) if x.is_empty()),
        "A answers once"
    );
    let DirDone::Asked(asked) = om.folder_done(c.job) else {
        panic!()
    };
    assert!(matches!(asked[..], [Act::Doctor(..)]), "{asked:?}");
    assert_eq!(om.page("/d", "h").folder, "/home/u/C");
}
