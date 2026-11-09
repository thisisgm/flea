use super::*;

fn dir(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("flea-airdrop-shared-{tag}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    root
}

#[test]
fn one_owner_at_a_time_and_every_other_window_reads_its_radio_and_card() {
    let root = dir("own");
    let (mut owner, mut other) = (Shared::at(root.clone()), Shared::at(root.clone()));
    assert_eq!(other.read().0, Radio::Off);
    assert!(owner.claim("Intel AX210"));
    assert!(
        !other.claim("Intel AX210"),
        "a second window cannot take the radio"
    );
    assert_eq!(other.read(), (Radio::Starting, "Intel AX210".to_string()));
    owner.publish(Radio::On, "Intel AX210");
    assert_eq!(other.read().0, Radio::On);
    owner.release();
    assert_eq!(other.read().0, Radio::Off);
    assert!(
        other.claim(""),
        "a released radio is free for the next window"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_radio_going_down_reads_off_at_once_but_stays_owned_until_owly_has_exited() {
    let root = dir("down");
    let (mut owner, mut other) = (Shared::at(root.clone()), Shared::at(root.clone()));
    assert!(owner.claim("Intel AX210"));
    owner.publish(Radio::On, "Intel AX210");
    owner.withdraw();
    assert_eq!(
        other.read().0,
        Radio::Off,
        "a held lock with no state reads Off"
    );
    assert!(
        !other.claim(""),
        "nobody takes the card while owly gives Wi-Fi back"
    );
    owner.release();
    assert!(other.claim(""));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_off_asked_of_the_owner_reads_off_in_every_window_until_it_is_withdrawn() {
    let root = dir("asked");
    let (mut owner, other) = (Shared::at(root.clone()), Shared::at(root.clone()));
    owner.claim("");
    owner.publish(Radio::On, "");
    other.ask_stop();
    assert_eq!(other.read().0, Radio::Off);
    other.keep();
    assert_eq!(other.read().0, Radio::On);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_state_file_left_with_no_owner_alive_reads_off() {
    let root = dir("stale");
    std::fs::write(root.join(STATE), "1-2\non\nIntel AX210\n").unwrap();
    assert_eq!(Shared::at(root.clone()).read().0, Radio::Off);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stop_asked_by_another_window_reaches_only_the_owner_it_named() {
    let root = dir("stop");
    let (mut owner, other) = (Shared::at(root.clone()), Shared::at(root.clone()));
    owner.claim("");
    assert!(!owner.stop_asked());
    other.ask_stop();
    assert!(owner.stop_asked());
    other.keep();
    assert!(
        !owner.stop_asked(),
        "turned back on before the owner looked"
    );
    other.ask_stop();
    owner.release();
    let mut next = Shared::at(root.clone());
    next.claim("");
    assert!(!next.stop_asked(), "an old ask never stops a later owner");
    drop(next);
    assert_eq!(
        other.read().0,
        Radio::Off,
        "an owner that goes away lets go"
    );
    let _ = std::fs::remove_dir_all(&root);
}
