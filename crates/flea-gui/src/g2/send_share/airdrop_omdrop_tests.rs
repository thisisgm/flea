use super::*;
use std::cell::RefCell;

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-gui-omdrop");
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

const ON: &str = r#"{"receiving": true, "visible": true, "name": "Omarchy", "dir": "/home/u/Inbox", "mode": "forever"}"#;
const OFF: &str = r#"{"receiving": false, "visible": false, "name": "Omarchy", "dir": "/home/u/Inbox", "mode": "off"}"#;
const PHONE: &str = r#"{"mac": "f2:f2:ee:c4:5a:f7", "rssi": null, "name": "Jane's iPhone"}"#;

fn started(acts: &[Act], proc: Proc) -> Option<(u64, Vec<String>)> {
    acts.iter().find_map(|act| match act {
        Act::Start(p, generation, argv) if *p == proc => Some((*generation, argv.clone())),
        _ => None,
    })
}

// Omdrop runs AirDrop, a file menu is open, and Omdrop's status answered.
fn opened(status: &str) -> (Omdrop, Vec<Act>) {
    let mut om = Omdrop::default();
    let _ = om.facts("/usr/bin/omdrop");
    let acts: [Act; 1] = om.menu_opened(0).try_into().expect("one status run");
    let [Act::Doctor(generation, argv)] = acts else {
        panic!()
    };
    assert_eq!(argv[3..], ["/usr/bin/omdrop", "status", "--json"]);
    let acts = om.status_done(generation, 0, status, 0);
    (om, acts)
}

// The fresh status an ended `omdrop on` or `off` asked for, answered with `status`.
fn confirm(om: &mut Omdrop, acts: &[Act], status: &str, now_ms: u64) -> Vec<Act> {
    let Some(Act::Doctor(generation, _)) = acts.iter().find(|a| matches!(a, Act::Doctor(..)))
    else {
        panic!("an ended on or off asks Omdrop's status: {acts:?}");
    };
    om.status_done(*generation, 0, status, now_ms)
}

#[test]
fn omdrop_already_on_lists_devices_as_they_name_themselves_and_sends_to_the_address() {
    let (mut om, acts) = opened(ON);
    assert!(om.adapter.ready() && om.on());
    let (browse, argv) = started(&acts, Proc::Browse).expect("an open menu looks for devices");
    assert_eq!(
        argv,
        ["/usr/bin/omdrop", "peers", "-n", "--json", "--stream"]
    );
    let unnamed = r#"{"mac": "b2:c4:98:5e:e6:be", "rssi": null, "name": null}"#;
    assert!(om.line(Proc::Browse, browse, unnamed).is_empty());
    assert_eq!(om.line(Proc::Browse, browse, PHONE), [Act::Refresh]);
    assert!(
        om.line(Proc::Browse, browse, PHONE).is_empty(),
        "a device is listed once"
    );
    let wire = Lines::default();
    let mut outbox = Outbox::default();
    let paths = ["/a/b.png".to_string()];
    let said = om.send(&mut outbox, &wire, "f2:f2:ee:c4:5a:f7", &paths, &english());
    assert!(matches!(&said[..], [Act::Say(_, false)]), "{said:?}");
    let sent = wire.0.borrow();
    assert!(
        sent[0].contains(r#""peer":"f2:f2:ee:c4:5a:f7""#),
        "{sent:?}"
    );
    assert!(sent[0].contains(r#""name":"Jane's iPhone""#), "{sent:?}");
}

#[test]
fn turn_on_runs_omdrop_on_and_a_fresh_status_starts_the_first_lookup_behind_a_shut_menu() {
    let i18n = english();
    let (mut om, acts) = opened(OFF);
    assert_eq!(om.phase, Phase::Off);
    assert!(started(&acts, Proc::Browse).is_none());
    let (radio, argv) = started(&om.turn_on(&i18n), Proc::Radio).expect("omdrop on runs");
    assert_eq!(argv, ["setsid", "-w", "/usr/bin/omdrop", "on"]);
    // Choosing Turn On AirDrop shuts the menu before omdrop on returns.
    om.menu_closed();
    let ended = om.ended(Proc::Radio, radio, 0, 1_000, &i18n);
    assert_eq!(om.phase, Phase::Confirming, "exit 0 alone proves nothing");
    let acts = confirm(&mut om, &ended, ON, 1_000);
    assert_eq!(om.phase, Phase::On);
    let (browse, _) =
        started(&acts, Proc::Browse).expect("the first lookup starts with the menu shut");
    assert_eq!(om.line(Proc::Browse, browse, PHONE), [Act::Refresh]);
    assert!(
        om.ended(Proc::Browse, browse, 0, 40_000, &i18n).is_empty(),
        "no lookup restarts behind a shut menu"
    );
    assert_eq!(om.devices().len(), 1, "the next open lists what it found");
}

#[test]
fn a_failed_omdrop_on_says_why_and_status_decides_the_state() {
    let i18n = english();
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.line(
        Proc::Radio,
        radio,
        "Omdrop could not be turned on: the radio backend is missing.",
    );
    let acts = om.ended(Proc::Radio, radio, 1, 0, &i18n);
    assert_eq!(
        acts[0],
        Act::Say(
            "AirDrop · Omdrop could not be turned on: the radio backend is missing.".into(),
            true
        )
    );
    confirm(&mut om, &acts, OFF, 0);
    assert_eq!(om.phase, Phase::Off);
}

#[test]
fn receiving_off_runs_omdrop_off_and_shows_on_until_status_says_off() {
    let i18n = english();
    let (mut om, _) = opened(ON);
    let acts = om.receiving_off();
    assert!(
        !acts.contains(&Act::Stop(Proc::Radio)),
        "nothing kills omdrop off: {acts:?}"
    );
    let (off, argv) = started(&acts, Proc::Radio).expect("omdrop off runs");
    assert_eq!(argv, ["/usr/bin/omdrop", "off"]);
    assert_eq!(om.phase, Phase::Stopping);
    assert!(om.page("/d", "h").receiving);
    let ended = om.ended(Proc::Radio, off, 0, 0, &i18n);
    assert!(
        om.page("/d", "h").receiving,
        "on until Omdrop's status says off"
    );
    confirm(&mut om, &ended, OFF, 0);
    assert_eq!(om.phase, Phase::Off);
    assert!(!om.page("/d", "h").receiving);
}

#[test]
fn one_omdrop_on_or_off_runs_at_a_time_and_a_switched_off_plugin_kills_neither() {
    let i18n = english();
    let (mut om, _) = opened(ON);
    let (off, _) = started(&om.receiving_off(), Proc::Radio).unwrap();
    assert!(
        om.turn_on(&i18n).is_empty(),
        "no omdrop on beside a running off"
    );
    assert!(om.receiving_off().is_empty(), "no second omdrop off");
    assert!(
        om.menu_opened(1)
            .iter()
            .all(|a| !matches!(a, Act::Doctor(..))),
        "no status while it runs"
    );
    let acts = om.disable();
    assert!(!acts.contains(&Act::Stop(Proc::Radio)), "{acts:?}");
    assert_eq!(om.phase, Phase::Stopping);
    let ended = om.ended(Proc::Radio, off, 0, 0, &i18n);
    assert!(om.turn_on(&i18n).is_empty(), "nor before its status lands");
    confirm(&mut om, &ended, OFF, 0);
    assert_eq!(om.phase, Phase::Off);
    assert!(started(&om.turn_on(&i18n), Proc::Radio).is_some());
}

#[test]
fn a_refused_omdrop_off_says_why_and_status_shows_omdrop_still_on() {
    let i18n = english();
    let (mut om, _) = opened(ON);
    let (off, _) = started(&om.receiving_off(), Proc::Radio).unwrap();
    let refusal =
        "Still sending to bPhone18. Wait for it, or turn it off anyway with: omdrop off --force";
    om.line(Proc::Radio, off, refusal);
    let acts = om.ended(Proc::Radio, off, 1, 0, &i18n);
    assert_eq!(acts[0], Act::Say(format!("AirDrop · {refusal}"), true));
    confirm(&mut om, &acts, ON, 0);
    assert_eq!(
        om.phase,
        Phase::On,
        "Receiving shows on again, so Off can be chosen again"
    );
    assert!(started(&om.receiving_off(), Proc::Radio).is_some());
}

#[test]
fn receiving_on_starts_the_receiver_when_only_the_radio_is_up() {
    let i18n = english();
    let (mut om, _) = opened(r#"{"receiving": false, "visible": true}"#);
    assert_eq!(om.phase, Phase::On);
    assert!(!om.page("/d", "h").receiving);
    let (_, argv) = started(&om.turn_on(&i18n), Proc::Radio).expect("omdrop on runs");
    assert_eq!(argv[2..], ["/usr/bin/omdrop", "on"]);
    let (mut both, _) = opened(ON);
    assert!(both.turn_on(&i18n).is_empty(), "nothing to start");
}

#[test]
fn a_running_receiver_with_the_radio_down_shows_and_can_be_turned_off() {
    let (mut om, acts) =
        opened(r#"{"receiving": true, "visible": false, "visibility": "contacts"}"#);
    assert_eq!(om.phase, Phase::Off);
    assert!(
        started(&acts, Proc::Browse).is_none(),
        "no lookup without the radio"
    );
    let page = om.page("/d", "h");
    assert!(page.receiving && page.contacts && page.omdrop, "{page:?}");
    let (_, argv) = started(&om.receiving_off(), Proc::Radio).expect("Off reaches the receiver");
    assert_eq!(argv, ["/usr/bin/omdrop", "off"]);
}

#[test]
fn a_status_asked_before_on_or_off_does_not_land_and_exit_zero_needs_a_status() {
    let i18n = english();
    let (mut om, _) = opened(OFF);
    om.menu_closed();
    let Some(Act::Doctor(stale, _)) = om
        .menu_opened(1)
        .into_iter()
        .find(|a| matches!(a, Act::Doctor(..)))
    else {
        panic!("each open asks Omdrop's status");
    };
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    assert!(
        om.status_done(stale, 0, OFF, 2).is_empty(),
        "the old answer is dropped"
    );
    assert_eq!(om.phase, Phase::Starting);
    let ended = om.ended(Proc::Radio, radio, 0, 3, &i18n);
    // omdrop on exits 0 when an off stopped it while starting; status then says off.
    confirm(&mut om, &ended, OFF, 3);
    assert_eq!(om.phase, Phase::Off);
    assert!(!om.page("/d", "h").receiving);
}

#[test]
fn omdrop_turned_off_behind_flea_drops_its_devices_on_the_next_status() {
    let i18n = english();
    let (mut om, acts) = opened(ON);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    om.line(Proc::Browse, browse, PHONE);
    om.menu_closed();
    let generation = match &om.menu_opened(10)[..] {
        [Act::Doctor(generation, _), ..] => *generation,
        other => panic!("Omdrop's status is asked on every open: {other:?}"),
    };
    let acts = om.status_done(generation, 0, OFF, 10);
    assert_eq!(om.phase, Phase::Off);
    assert!(om.devices().is_empty());
    assert!(acts.contains(&Act::Stop(Proc::Browse)), "{acts:?}");
    assert!(
        om.ended(Proc::Browse, browse, 0, 40_000, &i18n).is_empty(),
        "the stopped lookup is forgotten"
    );
}

#[test]
fn a_lookup_that_ends_early_waits_for_a_later_open_instead_of_looping() {
    let i18n = english();
    let (mut om, acts) = opened(ON);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    assert!(om.ended(Proc::Browse, browse, 0, 100, &i18n).is_empty());
    assert!(
        started(&om.menu_opened(200), Proc::Browse).is_some(),
        "the next open looks again"
    );
}

#[test]
fn the_page_shows_omdrops_own_name_and_folder_and_a_new_folder_goes_to_omdrop_dir() {
    let (mut om, _) = opened(ON);
    let page = om.page("/home/u/Downloads", "host");
    assert_eq!(
        (page.folder.as_str(), page.shown_as.as_str()),
        ("/home/u/Inbox", "Omarchy")
    );
    assert!(page.omdrop && page.receiving && !page.contacts);
    assert_eq!(
        om.folder_argv("/home/u/New").unwrap(),
        ["/usr/bin/omdrop", "dir", "/home/u/New"]
    );
    assert_eq!(om.page("/d", "h").folder, "/home/u/New");
}

#[test]
fn an_inactive_omdrop_asks_nothing_and_a_new_path_resets_what_was_known() {
    let mut om = Omdrop::default();
    assert!(!om.active() && om.ask_status().is_empty());
    let _ = om.facts("/usr/bin/omdrop");
    assert_eq!(om.adapter.readiness, Readiness::Checking);
    let _ = om.facts("");
    assert_eq!(om.adapter.readiness, Readiness::Missing);
    assert!(om.menu_opened(0).is_empty());
}

fn no_lookup(acts: &[Act]) -> bool {
    started(acts, Proc::Browse).is_none()
}

#[test]
fn a_switched_off_plugin_starts_no_lookup_from_any_answer_that_lands_later() {
    let i18n = english();
    // Switched off while omdrop on runs: its exit and the status it asks for start nothing.
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    om.disable();
    let ended = om.ended(Proc::Radio, radio, 0, 1_000, &i18n);
    assert!(no_lookup(&confirm(&mut om, &ended, ON, 1_000)));
    assert_eq!(om.phase, Phase::On, "Omdrop is on as it said");
    // Switched off while that status was still out.
    let (mut om, _) = opened(OFF);
    let (radio, _) = started(&om.turn_on(&i18n), Proc::Radio).unwrap();
    let ended = om.ended(Proc::Radio, radio, 0, 1_000, &i18n);
    om.disable();
    assert!(no_lookup(&confirm(&mut om, &ended, ON, 1_000)));
    // Switched off with an open menu's ordinary status out.
    let (mut om, _) = opened(OFF);
    let asked = om.menu_opened(5);
    om.disable();
    assert!(no_lookup(&confirm(&mut om, &asked, ON, 5)));
    assert!(no_lookup(&om.menu_opened(6)), "nor does a menu");
    om.enable();
    assert!(
        started(&om.menu_opened(7), Proc::Browse).is_some(),
        "switched on again, the next open looks"
    );
}

#[test]
fn another_omdrop_or_none_drops_what_the_old_one_found_and_its_late_answers() {
    let i18n = english();
    let (mut om, acts) = opened(ON);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    om.line(Proc::Browse, browse, PHONE);
    let (off, _) = started(&om.receiving_off(), Proc::Radio).unwrap();
    let acts = om.facts("/opt/omdrop/bin/omdrop");
    assert!(acts.contains(&Act::Stop(Proc::Browse)), "{acts:?}");
    assert!(om.devices().is_empty() && !om.on());
    assert!(om.line(Proc::Browse, browse, PHONE).is_empty());
    assert!(om.ended(Proc::Browse, browse, 0, 40_000, &i18n).is_empty());
    assert!(
        om.ended(Proc::Radio, off, 0, 40_000, &i18n).is_empty(),
        "the old omdrop off asks no status"
    );
    let _ = om.facts("");
    assert!(om.menu_opened(50_000).is_empty(), "no omdrop, no lookup");
}

#[test]
fn an_omdrop_send_is_answered_through_the_windows_one_numbering() {
    let (mut om, acts) = opened(ON);
    let (browse, _) = started(&acts, Proc::Browse).unwrap();
    om.line(Proc::Browse, browse, PHONE);
    let mut air = super::super::airdrop::AirDrop::default();
    let paths = ["/a/b.png".to_string()];
    om.send(
        &mut air.outbox,
        &Lines::default(),
        "f2:f2:ee:c4:5a:f7",
        &paths,
        &english(),
    );
    let declined = r#"{"t":"airdropsent","id":1,"ok":false,"code":"declined","reason":""}"#;
    assert_eq!(
        air.backend_line(declined, &english()),
        [Act::Say("Jane's iPhone declined the files.".into(), true)]
    );
}

#[path = "airdrop_omdrop_status_tests.rs"]
mod status;
