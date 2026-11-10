use super::*;
use std::time::{Duration, Instant};

// A window whose changes to the machine's state wait for its chain, as the Shell's do.
fn queued(dir: &std::path::Path) -> AirDrop {
    let mut air = window(None);
    air.shared = Shared::queued(dir.to_path_buf());
    air
}

fn written(air: &mut AirDrop) {
    if let Some((dir, writes)) = air.shared.writes() {
        writes.into_iter().for_each(|write| write.run(&dir));
    }
}

#[test]
fn claim_publish_and_withdraw_change_nothing_until_the_windows_chain_writes_them() {
    let dir = machine_dir("queue");
    let i18n = english();
    let (mut owner, mut reader) = (queued(&dir), window(Some(&dir)));
    let claim = owner.claims();
    let look = claimed(&mut owner, claim);
    let radio = start(&owner.turn_on("/d", &i18n, &look), Proc::Radio).unwrap();
    assert_eq!(
        seen(&reader).radio,
        Radio::Off,
        "the claim alone shows no radio"
    );
    assert!(!reader.shared.claim(""), "the look's claim holds the lock");
    written(&mut owner);
    assert_eq!(seen(&reader).radio, Radio::Starting);
    owner.line(Proc::Radio, radio, UP, 0, &i18n);
    assert_eq!(
        seen(&reader).radio,
        Radio::Starting,
        "On waits for the chain"
    );
    written(&mut owner);
    assert_eq!(seen(&reader).radio, Radio::On);
    owner.turn_off();
    assert_eq!(seen(&reader).radio, Radio::On, "the withdrawal waits too");
    written(&mut owner);
    assert_eq!(seen(&reader).radio, Radio::Off);
    owner.ended(Proc::Radio, radio, 0, 0, &i18n);
    assert!(
        !reader.shared.claim(""),
        "the lock goes back after the state"
    );
    written(&mut owner);
    assert!(reader.shared.claim(""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_lock_the_look_took_for_an_on_overtaken_meanwhile_goes_back_with_the_next_writes() {
    let dir = machine_dir("spare");
    let (mut owner, mut other) = (queued(&dir), window(Some(&dir)));
    claimed(&mut owner, true);
    assert!(!other.shared.claim(""));
    written(&mut owner);
    assert!(other.shared.claim(""), "a lock no On took is given back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unusable_shared_directory_keeps_airdrop_local_instead_of_waiting_forever() {
    let dir = machine_dir("unusable");
    let blocked = dir.join("not-a-directory");
    std::fs::write(&blocked, "").unwrap();
    let mut owner = queued(&blocked);
    let claim = owner.claims();
    let look = claimed(&mut owner, claim);
    let acts = owner.turn_on("/d", &english(), &look);
    assert!(start(&acts, Proc::Radio).is_some(), "{acts:?}");
    assert!(!owner.waits());
    written(&mut owner);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_closing_window_lets_go_of_the_radio_after_its_last_writes_off_its_thread() {
    let dir = machine_dir("closeq");
    let i18n = english();
    let (mut owner, mut reader) = (queued(&dir), window(Some(&dir)));
    let claim = owner.claims();
    let look = claimed(&mut owner, claim);
    let radio = start(&owner.turn_on("/d", &i18n, &look), Proc::Radio).unwrap();
    owner.line(Proc::Radio, radio, UP, 0, &i18n);
    drop(owner);
    let started = Instant::now();
    while !reader.shared.claim("") && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the radio was never let go"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Only Receiving On by hand withdraws a stop; an automatic retry must preserve a later Off.
#[test]
fn receiving_off_in_a_third_window_is_not_undone_by_an_on_still_waiting_in_another() {
    let dir = machine_dir("third");
    let i18n = english();
    let (mut waiting, mut owner, mut third) =
        (window(Some(&dir)), window(Some(&dir)), window(Some(&dir)));
    let (waiting, owner, third) = (&mut waiting, &mut owner, &mut third);
    let radio = start(&on(owner, &i18n), Proc::Radio).unwrap();
    owner.line(Proc::Radio, radio, UP, 0, &i18n);
    off(owner);
    assert!(on(waiting, &i18n)
        .iter()
        .all(|a| !matches!(a, Act::Start(..))));
    assert!(waiting.waits(), "the On waits while owly gives Wi-Fi back");
    on(owner, &i18n);
    let acts = owner.ended(Proc::Radio, radio, 0, 0, &i18n);
    let again = start(&acts, Proc::Radio).expect("the owner's own On takes its lock straight over");
    owner.line(Proc::Radio, again, UP, 0, &i18n);
    looked(third);
    assert!(off(third).contains(&Act::AskStop));
    tick(waiting, &i18n);
    let (acts, _) = tick(owner, &i18n);
    assert!(acts.contains(&Act::Stop(Proc::Radio)), "{acts:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
