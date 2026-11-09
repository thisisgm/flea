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
    assert_eq!(other.probe().seen().radio, Radio::Off);
    assert!(owner.claim("Intel AX210"));
    assert!(
        !other.claim("Intel AX210"),
        "a second window cannot take the radio"
    );
    let seen = other.probe().seen();
    assert_eq!(
        (seen.radio, seen.card.as_str()),
        (Radio::Starting, "Intel AX210")
    );
    owner.publish(Radio::On, "Intel AX210");
    assert_eq!(other.probe().seen().radio, Radio::On);
    owner.release();
    assert_eq!(other.probe().seen().radio, Radio::Off);
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
        other.probe().seen().radio,
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
    other.probe().ask_stop();
    assert_eq!(other.probe().seen().radio, Radio::Off);
    other.probe().keep();
    assert_eq!(other.probe().seen().radio, Radio::On);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_state_file_left_with_no_owner_alive_reads_off() {
    let root = dir("stale");
    std::fs::write(root.join(STATE), "1-2\non\nIntel AX210\n").unwrap();
    assert_eq!(Shared::at(root.clone()).probe().seen().radio, Radio::Off);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stop_asked_by_another_window_reaches_only_the_owner_it_named() {
    let root = dir("stop");
    let (mut owner, other) = (Shared::at(root.clone()), Shared::at(root.clone()));
    owner.claim("");
    assert!(!owner.stop_asked(&owner.probe().seen()));
    other.probe().ask_stop();
    assert!(owner.stop_asked(&owner.probe().seen()));
    other.probe().keep();
    assert!(
        !owner.stop_asked(&owner.probe().seen()),
        "turned back on before the owner looked"
    );
    other.probe().ask_stop();
    owner.release();
    let mut next = Shared::at(root.clone());
    next.claim("");
    assert!(
        !next.stop_asked(&next.probe().seen()),
        "an old ask never stops a later owner"
    );
    drop(next);
    assert_eq!(
        other.probe().seen().radio,
        Radio::Off,
        "an owner that goes away lets go"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_look_taken_before_the_owner_claimed_again_never_stops_its_new_radio() {
    let root = dir("again");
    let (mut owner, other) = (Shared::at(root.clone()), Shared::at(root.clone()));
    owner.claim("");
    other.probe().ask_stop();
    let late = owner.probe().seen();
    assert!(owner.stop_asked(&late));
    owner.release();
    owner.claim("");
    assert!(
        !owner.stop_asked(&late),
        "a look that ran for the last radio lands on this one"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_quit_waits_for_a_stop_still_being_written_and_never_past_its_bound() {
    let unwritten = std::sync::Arc::new(AtomicUsize::new(1));
    let writer = unwritten.clone();
    let started = Instant::now();
    let write = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        writer.fetch_sub(1, Ordering::SeqCst);
    });
    wait_written(&unwritten, Duration::from_secs(5));
    let waited = started.elapsed();
    write.join().unwrap();
    assert!(waited >= Duration::from_millis(50) && waited < Duration::from_secs(5));
    unwritten.fetch_add(1, Ordering::SeqCst);
    let started = Instant::now();
    wait_written(&unwritten, Duration::from_millis(30));
    let held = started.elapsed();
    assert!(
        held >= Duration::from_millis(30) && held < Duration::from_secs(1),
        "a stop never written holds the quit only until the bound"
    );
}
