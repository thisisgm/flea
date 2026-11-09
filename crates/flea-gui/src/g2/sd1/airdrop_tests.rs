use super::*;
use std::cell::RefCell;

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-gui-airdrop");
    I18n::load(flea::i18n::Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

#[derive(Default)]
struct Lines(RefCell<Vec<String>>);
impl Wire for Lines {
    fn send(&self, line: String) -> bool {
        self.0.borrow_mut().push(line);
        true
    }
}

const READY: &str =
    r#"{"device":"8086:2725","driver":"iwlwifi","hardware":true,"status":"tested","missing":[]}"#;

// owly installed, doctor answered ready, the radio still down.
fn ready() -> AirDrop {
    let mut air = AirDrop::default();
    air.facts("/usr/bin/owly");
    let acts: [Act; 1] = air.ask_doctor(0).try_into().expect("one doctor run");
    let [Act::Doctor(generation, argv)] = acts else {
        panic!()
    };
    assert_eq!(argv.last().map(String::as_str), Some("--json"));
    assert_eq!(air.doctor_done(generation, 0, READY, 0), [Act::Refresh]);
    air
}

fn started(acts: &[Act], proc: Proc) -> Option<(u64, Vec<String>)> {
    acts.iter().find_map(|act| match act {
        Act::Start(p, generation, argv) if *p == proc => Some((*generation, argv.clone())),
        _ => None,
    })
}

// Ready and turned on: the radio said awdl0 is up while a menu was open.
fn on_with_menu() -> (AirDrop, u64) {
    let i18n = english();
    let mut air = ready();
    air.menu_opened(1);
    let (radio, _) =
        started(&air.turn_on("/home/u/Downloads", &i18n), Proc::Radio).expect("the radio starts");
    let acts = air.line(
        Proc::Radio,
        radio,
        "awdl0 is up: fe80::1%awdl0 (02:00:00:00:00:01). Ctrl-C takes it down.",
    );
    assert_eq!(air.radio, Radio::On);
    let (_, receive) = started(&acts, Proc::Receive).expect("the receiver starts once awdl0 is up");
    assert_eq!(
        receive,
        [
            "timeout",
            "600",
            "/usr/bin/owly",
            "receive",
            "--dir",
            "/home/u/Downloads"
        ]
    );
    let (browse, _) = started(&acts, Proc::Browse).expect("an open menu browses at once");
    (air, browse)
}

#[test]
fn a_missing_owly_runs_nothing_and_says_so_on_the_page() {
    let mut air = AirDrop::default();
    air.facts("");
    assert!(air.ask_doctor(0).is_empty());
    assert!(air.menu_opened(0).is_empty());
    assert_eq!(air.adapter.readiness, Readiness::Missing);
    let acts = air.turn_on("/d", &english());
    assert!(matches!(acts[0], Act::Say(_, true)), "{acts:?}");
    assert_eq!(air.radio, Radio::Off);
    assert_eq!(
        air.page("/d", "minipc").adapter.readiness,
        Readiness::Missing
    );
}

#[test]
fn doctor_is_asked_once_while_fresh_and_a_stale_answer_never_lands() {
    let mut air = AirDrop::default();
    air.facts("/usr/bin/owly");
    let first = air.ask_doctor(0);
    assert_eq!(first.len(), 1);
    assert!(air.ask_doctor(10).is_empty(), "one run at a time");
    assert!(
        air.doctor_done(99, 0, READY, 10).is_empty(),
        "another generation is ignored"
    );
    let Act::Doctor(generation, _) = &first[0] else {
        panic!()
    };
    air.doctor_done(
        *generation,
        1,
        r#"{"device":"8086:24fd","hardware":true,"status":"broken","missing":[]}"#,
        10,
    );
    assert_eq!(air.adapter.readiness, Readiness::Broken);
    assert!(
        air.ask_doctor(20_000).is_empty(),
        "fresh for thirty seconds"
    );
    assert_eq!(air.ask_doctor(40_001).len(), 1);
}

#[test]
fn an_unsupported_adapter_refuses_to_turn_on() {
    let mut air = AirDrop::default();
    air.facts("/usr/bin/owly");
    let acts: [Act; 1] = air.ask_doctor(0).try_into().unwrap();
    let [Act::Doctor(generation, _)] = acts else {
        panic!()
    };
    air.doctor_done(
        generation,
        1,
        r#"{"device":"8086:24fd","hardware":true,"status":"broken","missing":[]}"#,
        0,
    );
    assert!(matches!(
        air.turn_on("/d", &english())[0],
        Act::Say(_, true)
    ));
    assert_eq!(air.radio, Radio::Off);
}

#[test]
fn browse_lists_devices_as_each_answers_and_a_shut_menu_stops_it() {
    let (mut air, browse) = on_with_menu();
    assert!(
        air.devices().is_empty(),
        "Looking for devices… until one answers"
    );
    assert_eq!(
        air.line(
            Proc::Browse,
            browse,
            r#"3a9f   "Jane's iPhone" ([fe80::1%7]:8770)"#
        ),
        [Act::Refresh]
    );
    assert!(
        air.line(
            Proc::Browse,
            browse,
            r#"3a9f   "Jane's iPhone" ([fe80::1%7]:8770)"#
        )
        .is_empty(),
        "no duplicate"
    );
    air.line(
        Proc::Browse,
        browse,
        r#"0f1e   "MacBook Air" ([fe80::2%7]:8770)"#,
    );
    assert!(
        air.line(Proc::Browse, browse - 1, r#"aa   "Old" ([fe80::3%7]:8770)"#)
            .is_empty(),
        "a replaced browse is ignored"
    );
    let names: Vec<_> = air
        .devices()
        .into_iter()
        .map(|(p, phone)| (p.label, phone))
        .collect();
    assert_eq!(
        names,
        [
            ("Jane's iPhone".to_string(), true),
            ("MacBook Air".to_string(), false)
        ]
    );
    // The browse's thirty seconds ran out under an open menu: it listens again.
    assert!(started(
        &air.ended(Proc::Browse, browse, 0, &english()),
        Proc::Browse
    )
    .is_some());
    assert_eq!(air.menu_closed(), [Act::Stop(Proc::Browse)]);
    assert!(air.menu_closed().is_empty());
    assert_eq!(
        air.devices().len(),
        2,
        "the radio's devices stay listed for the next open"
    );
}

#[test]
fn a_send_goes_to_the_backend_as_a_job_and_its_answer_is_said_once() {
    let i18n = english();
    let (mut air, browse) = on_with_menu();
    air.line(
        Proc::Browse,
        browse,
        r#"3a9f   "Jane's iPhone" ([fe80::1%7]:8770)"#,
    );
    let wire = Lines::default();
    let paths = vec!["/a/b.png".to_string()];
    assert_eq!(
        air.send(&wire, "3a9f", &paths, &i18n),
        [Act::Say(
            "Sending b.png to Jane's iPhone with AirDrop".into(),
            false
        )]
    );
    assert_eq!(
        wire.0.borrow()[0],
        r#"{"c":"airdrop","op":"send","id":1,"peer":"3a9f","paths":["/a/b.png"]}"#
    );
    let declined = r#"{"t":"airdropsent","id":1,"ok":false,"code":"declined","reason":""}"#;
    assert_eq!(
        air.backend_line(declined, &i18n),
        [Act::Say("Jane's iPhone declined the files.".into(), true)]
    );
    assert!(air.backend_line(declined, &i18n).is_empty(), "said once");
    // A device that never answered this radio sends nothing.
    assert!(matches!(
        air.send(&wire, "77c2", &paths, &i18n)[0],
        Act::Say(_, true)
    ));
    // A radio that is not on sends nothing, even to a device still listed.
    air.radio = Radio::Starting;
    assert!(matches!(
        air.send(&wire, "3a9f", &paths, &i18n)[0],
        Act::Say(_, true)
    ));
    air.radio = Radio::On;
    // A pane whose backend is gone says its menu is out of date rather than nothing.
    assert_eq!(
        air.send(&Gone, "3a9f", &paths, &i18n),
        [Act::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
    assert_eq!(wire.0.borrow().len(), 1);
}

struct Gone;
impl Wire for Gone {
    fn send(&self, _line: String) -> bool {
        false
    }
}

#[test]
fn two_devices_with_one_name_are_both_listed_and_each_is_sent_to_by_its_service_id() {
    let i18n = english();
    let (mut air, browse) = on_with_menu();
    air.line(
        Proc::Browse,
        browse,
        r#"3a9f   "iPhone" ([fe80::1%7]:8770)"#,
    );
    air.line(
        Proc::Browse,
        browse,
        r#"0f1e   "iPhone" ([fe80::2%7]:8770)"#,
    );
    let ids: Vec<_> = air.devices().into_iter().map(|(p, _)| p.id).collect();
    assert_eq!(ids, ["3a9f", "0f1e"]);
    let wire = Lines::default();
    air.send(&wire, "0f1e", &["/a/b.png".to_string()], &i18n);
    assert!(wire.0.borrow()[0].contains(r#""peer":"0f1e""#));
}

#[test]
fn turning_off_stops_every_run_and_late_lines_change_nothing() {
    let (mut air, browse) = on_with_menu();
    let acts = air.turn_off();
    assert_eq!(
        acts,
        [
            Act::Stop(Proc::Radio),
            Act::Stop(Proc::Receive),
            Act::Stop(Proc::Browse),
            Act::Refresh
        ]
    );
    assert_eq!(air.radio, Radio::Off);
    assert!(air
        .line(Proc::Browse, browse, r#"aa   "Late" ([fe80::3%7]:8770)"#)
        .is_empty());
    assert!(air.turn_off().is_empty());
    assert!(!air.page("/d", "h").receiving);
}

#[test]
fn the_ten_minutes_ending_turns_off_quietly_and_a_radio_that_never_came_up_says_why() {
    let i18n = english();
    let mut air = ready();
    let (radio, _) = started(&air.turn_on("/d", &i18n), Proc::Radio).unwrap();
    air.line(
        Proc::Radio,
        radio,
        "awdl0 is up: fe80::1%awdl0 (m). Ctrl-C takes it down.",
    );
    air.line(Proc::Radio, radio, owly::RADIO_EXPIRED);
    let acts = air.ended(Proc::Radio, radio, 1, &i18n);
    assert!(!acts.iter().any(|a| matches!(a, Act::Say(..))), "{acts:?}");
    assert!(acts.contains(&Act::Stop(Proc::Receive)) && !acts.contains(&Act::Stop(Proc::Radio)));
    assert_eq!(air.radio, Radio::Off);
    let (radio, _) = started(&air.turn_on("/d", &i18n), Proc::Radio).unwrap();
    assert!(
        air.page("/d", "h").receiving,
        "Everyone for 10 Minutes shows while the radio starts"
    );
    air.line(Proc::Radio, radio, "bringing the radio up...");
    air.line(
        Proc::Radio,
        radio,
        "owly: the radio is in use by another program",
    );
    let acts = air.ended(Proc::Radio, radio, 2, &i18n);
    assert_eq!(
        acts[0],
        Act::Say(
            "AirDrop · the radio is in use by another program".into(),
            true
        )
    );
    assert_eq!(air.radio, Radio::Off);
}

#[test]
fn a_radio_that_goes_down_before_its_ten_minutes_end_says_why() {
    let i18n = english();
    let (mut air, _) = on_with_menu();
    let radio = air.gens[slot(Proc::Radio)];
    air.line(Proc::Radio, radio, "owly: the radio went down: link lost");
    let acts = air.ended(Proc::Radio, radio, 1, &i18n);
    assert_eq!(
        acts[0],
        Act::Say("AirDrop · the radio went down: link lost".into(), true)
    );
    assert_eq!(air.radio, Radio::Off);
}

#[test]
fn a_receiver_that_exits_takes_the_radio_down_and_only_a_failure_says_why() {
    let i18n = english();
    let (mut air, _) = on_with_menu();
    let receive = air.gens[slot(Proc::Receive)];
    let acts = air.ended(Proc::Receive, receive, owly::TIMEOUT_EXPIRED, &i18n);
    assert_eq!(
        acts,
        [
            Act::Stop(Proc::Radio),
            Act::Stop(Proc::Browse),
            Act::Refresh
        ],
        "its ten minutes ran out"
    );
    assert_eq!(air.radio, Radio::Off);
    let (mut air, _) = on_with_menu();
    let receive = air.gens[slot(Proc::Receive)];
    air.line(Proc::Receive, receive, "owly: awdl0: no such device");
    let acts = air.ended(Proc::Receive, receive, 2, &i18n);
    assert_eq!(
        acts[0],
        Act::Say("AirDrop · awdl0: no such device".into(), true)
    );
    assert!(acts.contains(&Act::Stop(Proc::Radio)) && !acts.contains(&Act::Stop(Proc::Receive)));
    assert!(!air.page("/d", "h").receiving);
}

#[test]
fn a_failed_browse_is_not_started_again_until_the_menu_opens_again() {
    let (mut air, browse) = on_with_menu();
    assert!(air.ended(Proc::Browse, browse, 2, &english()).is_empty());
    air.menu_closed();
    assert!(started(&air.menu_opened(2), Proc::Browse).is_some());
}

#[test]
fn a_doctor_still_running_when_owly_goes_away_never_lands() {
    let mut air = AirDrop::default();
    air.facts("/usr/bin/owly");
    let acts: [Act; 1] = air.ask_doctor(0).try_into().unwrap();
    let [Act::Doctor(generation, _)] = acts else {
        panic!()
    };
    air.facts("");
    assert!(air.doctor_done(generation, 0, READY, 1).is_empty());
    assert_eq!(air.adapter.readiness, Readiness::Missing);
}
