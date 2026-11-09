use super::*;
use std::path::PathBuf;

fn english() -> I18n {
    let none = PathBuf::from("/nonexistent/flea-gui-airdrop-machine");
    I18n::load(flea::i18n::Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

// A marked root of the test's own for the machine's AirDrop state.
fn machine_dir(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("flea-airdrop-machine-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    root
}

const UP: &str = "awdl0 is up: fe80::1%awdl0 (02:00:00:00:00:01). Ctrl-C takes it down.";

// A window whose doctor said ready, sharing the machine's state under `dir`, or keeping to itself without one.
fn window(dir: Option<&PathBuf>) -> AirDrop {
    let mut air = AirDrop {
        shared: dir.map_or_else(Shared::default, |d| Shared::at(d.clone())),
        ..AirDrop::default()
    };
    air.facts("/usr/bin/owly");
    air.adapter = Adapter {
        readiness: Readiness::Ready,
        card: "Intel AX210".into(),
    };
    air
}

fn start(acts: &[Act], proc: Proc) -> Option<u64> {
    acts.iter().find_map(|act| match act {
        Act::Start(p, generation, _) if *p == proc => Some(*generation),
        _ => None,
    })
}

#[test]
fn a_second_window_reads_the_first_windows_radio_and_never_starts_its_own() {
    let dir = machine_dir("read");
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio =
        start(&first.turn_on("/d", &i18n), Proc::Radio).expect("the first window takes the radio");
    assert!(second
        .turn_on("/d", &i18n)
        .iter()
        .all(|a| !matches!(a, Act::Start(..))));
    assert!(
        second.page("/d", "h").receiving,
        "the second window's page shows the radio on"
    );
    assert!(
        start(&second.menu_opened(0), Proc::Browse).is_none(),
        "nothing to browse until awdl0 is up"
    );
    second.menu_closed();
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    let acts = second.menu_opened(0);
    let browse = start(&acts, Proc::Browse).expect("the second window browses the first's radio");
    assert!(start(&acts, Proc::Radio).is_none() && start(&acts, Proc::Receive).is_none());
    second.line(
        Proc::Browse,
        browse,
        r#"3a9f   "iPhone" ([fe80::1%7]:8770)"#,
        0,
        &i18n,
    );
    assert_eq!(second.devices().len(), 1, "and lists what it found");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn receiving_off_in_the_second_window_stops_the_first_windows_radio_on_its_next_look() {
    let dir = machine_dir("stop");
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio = start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    second.look();
    assert!(second.watch(), "a reader watches the owner's radio");
    assert!(second.page("/d", "h").receiving);
    let acts = second.turn_off();
    assert!(
        !acts.contains(&Act::Stop(Proc::Radio)),
        "a reader has no radio to stop"
    );
    assert!(!second.page("/d", "h").receiving);
    second.tick(0, &i18n);
    assert!(
        !second.page("/d", "h").receiving,
        "the reader's own look before the owner's keeps Off"
    );
    assert!(first.watch());
    let (acts, going) = first.tick(0, &i18n);
    assert!(
        acts.contains(&Act::Stop(Proc::Radio)) && acts.contains(&Act::Stop(Proc::Receive)),
        "{acts:?}"
    );
    assert!(going);
    assert_eq!(
        first.tick(0, &i18n),
        (Vec::new(), false),
        "nothing left to watch"
    );
    assert!(!second.tick(0, &i18n).1);
    // owly is still giving Wi-Fi back: Receiving On waits and starts no second radio until its run exits.
    assert!(start(&second.turn_on("/d", &i18n), Proc::Radio).is_none());
    assert!(second.page("/d", "h").receiving, "the waiting On shows on");
    assert!(second.watch(), "and the watch tries it again");
    assert!(start(&second.tick(0, &i18n).0, Proc::Radio).is_none());
    first.ended(Proc::Radio, radio, 0, 0, &i18n);
    assert!(start(&second.tick(0, &i18n).0, Proc::Radio).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn receiving_on_while_this_windows_radio_goes_down_starts_it_once_owly_has_exited() {
    let i18n = english();
    let mut air = window(None);
    let radio = start(&air.turn_on("/d", &i18n), Proc::Radio).unwrap();
    air.line(Proc::Radio, radio, UP, 0, &i18n);
    air.turn_off();
    assert!(
        start(&air.turn_on("/d", &i18n), Proc::Radio).is_none(),
        "no second radio inside owly's teardown"
    );
    assert!(air.page("/d", "h").receiving);
    assert!(start(&air.ended(Proc::Radio, radio, 0, 0, &i18n), Proc::Radio).is_some());
}

#[test]
fn receiving_off_takes_back_a_receiving_on_still_waiting_for_owly() {
    let i18n = english();
    let mut air = window(None);
    let radio = start(&air.turn_on("/d", &i18n), Proc::Radio).unwrap();
    air.line(Proc::Radio, radio, UP, 0, &i18n);
    air.turn_off();
    air.turn_on("/d", &i18n);
    air.turn_off();
    assert!(!air.page("/d", "h").receiving);
    assert!(
        start(&air.ended(Proc::Radio, radio, 0, 0, &i18n), Proc::Radio).is_none(),
        "the On taken back never starts"
    );
}

#[test]
fn receiving_on_in_a_reader_while_the_owners_radio_goes_down_still_stops_its_browse() {
    let dir = machine_dir("drain");
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio = start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    start(&second.menu_opened(0), Proc::Browse).expect("the reader browses the owner's radio");
    first.turn_off();
    let acts = second.turn_on("/d", &i18n);
    assert!(acts.contains(&Act::Stop(Proc::Browse)), "{acts:?}");
    assert!(start(&acts, Proc::Radio).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// A reader browsing the owner's radio when it went down and owly has exited, at time zero.
fn reader_after_the_owner_let_go(tag: &str) -> (PathBuf, AirDrop) {
    let dir = machine_dir(tag);
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio = start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    start(&second.menu_opened(0), Proc::Browse).expect("the reader browses the owner's radio");
    first.turn_off();
    first.ended(Proc::Radio, radio, 0, 0, &i18n);
    (dir, second)
}

#[test]
fn a_reader_that_takes_the_radio_still_stops_the_browse_it_had() {
    let (dir, mut second) = reader_after_the_owner_let_go("take");
    let acts = second.turn_on("/d", &english());
    assert!(acts.contains(&Act::Stop(Proc::Browse)), "{acts:?}");
    assert!(start(&acts, Proc::Radio).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reader_whose_card_is_not_ready_still_stops_the_browse_it_had() {
    let (dir, mut second) = reader_after_the_owner_let_go("stale");
    second.adapter = Adapter::default();
    let acts = second.turn_on("/d", &english());
    assert!(acts.contains(&Act::Stop(Proc::Browse)), "{acts:?}");
    assert!(
        acts.iter().any(|a| matches!(a, Act::Say(_, true))),
        "{acts:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn receiving_back_on_in_the_reader_before_the_owner_looked_keeps_the_radio() {
    let dir = machine_dir("keep");
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio = start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    second.look();
    second.turn_off();
    assert!(second
        .turn_on("/d", &i18n)
        .iter()
        .all(|a| !matches!(a, Act::Start(..))));
    assert!(second.page("/d", "h").receiving);
    assert_eq!(
        first.tick(0, &i18n),
        (Vec::new(), true),
        "the owner keeps its radio"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reader_whose_menu_is_open_browses_once_the_owners_radio_comes_up() {
    let dir = machine_dir("up");
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio = start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    assert!(start(&second.menu_opened(0), Proc::Browse).is_none());
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    let (acts, going) = second.tick(0, &i18n);
    assert!(start(&acts, Proc::Browse).is_some(), "{acts:?}");
    assert!(going);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_owners_radio_going_down_takes_the_readers_devices_and_browse_with_it() {
    let dir = machine_dir("down");
    let i18n = english();
    let (mut first, mut second) = (window(Some(&dir)), window(Some(&dir)));
    let radio = start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    first.line(Proc::Radio, radio, UP, 0, &i18n);
    let browse = start(&second.menu_opened(0), Proc::Browse).unwrap();
    second.line(
        Proc::Browse,
        browse,
        r#"3a9f   "iPhone" ([fe80::1%7]:8770)"#,
        0,
        &i18n,
    );
    first.line(Proc::Radio, radio, owly::RADIO_EXPIRED, 0, &i18n);
    first.ended(Proc::Radio, radio, 1, 0, &i18n);
    let (acts, going) = second.tick(0, &i18n);
    assert!(acts.contains(&Act::Stop(Proc::Browse)) && acts.contains(&Act::Refresh));
    assert!(!going && second.devices().is_empty() && !second.page("/d", "h").receiving);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_window_that_closes_lets_go_of_the_machines_radio() {
    let dir = machine_dir("close");
    let i18n = english();
    let mut first = window(Some(&dir));
    start(&first.turn_on("/d", &i18n), Proc::Radio).unwrap();
    drop(first);
    let mut second = window(Some(&dir));
    second.look();
    assert!(!second.page("/d", "h").receiving);
    assert!(start(&second.turn_on("/d", &i18n), Proc::Radio).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_runtime_dir_each_window_keeps_its_own_radio_and_watches_nothing() {
    let i18n = english();
    let mut air = window(None);
    assert!(start(&air.turn_on("/d", &i18n), Proc::Radio).is_some());
    assert!(!air.watch());
}

// Radio up and a menu open: the browse it started, at time zero.
fn browsing() -> (AirDrop, u64) {
    let i18n = english();
    let mut air = window(None);
    air.menu_opened(0);
    let radio = start(&air.turn_on("/d", &i18n), Proc::Radio).unwrap();
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
        start(&air.menu_opened(5 + BROWSE_GAP_MS - 1), Proc::Browse).is_none(),
        "a menu opened at once does not start another"
    );
    air.menu_closed();
    assert!(start(&air.menu_opened(5 + BROWSE_GAP_MS), Proc::Browse).is_some());
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

// The receiver is up: its generation, at time zero.
fn receiving() -> (AirDrop, u64) {
    let i18n = english();
    let mut air = window(None);
    let radio = start(&air.turn_on("/d", &i18n), Proc::Radio).unwrap();
    let acts = air.line(Proc::Radio, radio, UP, 0, &i18n);
    (air, start(&acts, Proc::Receive).unwrap())
}

#[test]
fn files_the_receiver_saved_are_said_by_name_then_counted_within_one_transfer() {
    let i18n = english();
    let (mut air, receive) = receiving();
    let mut say = |text: &str, at| air.line(Proc::Receive, receive, text, at, &i18n);
    assert_eq!(
        say("/d/IMG_0001.HEIC", 10),
        [Act::Say(
            "Received IMG_0001.HEIC with AirDrop".into(),
            false
        )]
    );
    assert_eq!(
        say("/d/IMG_0002.HEIC", 20),
        [Act::Say("Received 2 items with AirDrop".into(), false)]
    );
    assert_eq!(
        say("/d/notes.txt", 20 + RECEIVED_GROUP_MS),
        [Act::Say("Received notes.txt with AirDrop".into(), false)],
        "a later transfer starts its own count"
    );
    assert_eq!(
        say(r#""https://example.com/""#, 30_000),
        [Act::Say(
            "Received https://example.com/ with AirDrop".into(),
            false
        )],
        "a link is shown, never opened"
    );
    assert_eq!(
        say("transfer failed: connection reset", 31_000),
        [Act::Say("AirDrop · connection reset".into(), true)]
    );
    assert_eq!(
        say("the request was withdrawn", 32_000),
        [Act::Say(
            "AirDrop could not finish the transfer.".into(),
            true
        )]
    );
    assert!(say("seen by fe80::1%awdl0", 33_000).is_empty());
    assert!(
        air.line(Proc::Receive, receive - 1, "/d/old.txt", 34_000, &i18n)
            .is_empty(),
        "a replaced receiver says nothing"
    );
}
