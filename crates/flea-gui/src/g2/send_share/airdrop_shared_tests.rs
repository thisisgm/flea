use super::super::airdrop_pids::SIGTERM;
use super::*;

fn dir(tag: &str) -> PathBuf {
    let output = std::process::Command::new("mktemp")
        .args(["-d", &format!("/tmp/flea-airdrop-shared-{tag}.XXXXXXXX")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
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

// Runs only as the child of the next two tests: a reader's Off whose stop is still being written when SIGTERM comes.
#[test]
fn sigterm_stop_child() {
    let Some(root) = std::env::var_os("FLEA_AIRDROP_SIGTERM_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.is_absolute() && root.starts_with("/tmp"));
    assert!(root.join(".flea-test-sandbox").is_file());
    let delay = std::fs::read_to_string(root.join("delay-ms")).unwrap();
    let delay = Duration::from_millis(delay.trim().parse().unwrap());
    // The process's own count, the one a SIGTERM waits on.
    let mut reader = Shared::queued(root.clone());
    reader.unwritten = &UNWRITTEN;
    let (probe, held) = (reader.probe(), reader.hold());
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        probe.ask_stop();
        drop(held);
    });
    std::fs::write(root.join("asked"), "").unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(10));
    }
}

// The owner's state in a marked root, a child Flea that asks it to stop, and SIGTERM sent while the stop is pending.
fn sigterm_while_a_stop_is_pending(tag: &str, delay_ms: u64) -> (PathBuf, Option<i32>, Duration) {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    let root = dir(tag);
    std::fs::write(root.join(STATE), "4242-1\non\nIntel AX210\n").unwrap();
    std::fs::write(root.join("delay-ms"), delay_ms.to_string()).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "g2::send_share::airdrop_shared::tests::sigterm_stop_child",
            "--test-threads=1",
        ])
        .env("FLEA_AIRDROP_SIGTERM_ROOT", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !root.join("asked").exists() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(5));
    }
    let sent = Instant::now();
    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let status = child.wait().unwrap();
    (root, status.signal(), sent.elapsed())
}

#[test]
fn flea_killed_by_sigterm_writes_a_pending_stop_first_then_dies_by_it() {
    let (root, signal, took) = sigterm_while_a_stop_is_pending("termstop", 300);
    let asked = std::fs::read_to_string(root.join(STOP)).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(signal, Some(SIGTERM));
    assert_eq!(asked, "4242-1", "the owner was never asked to stop");
    assert!(took < SETTLE + Duration::from_secs(2), "{took:?}");
}

#[test]
fn a_stop_never_written_holds_a_sigterm_only_until_the_bound() {
    let (root, signal, took) = sigterm_while_a_stop_is_pending("termbound", 30_000);
    let asked = root.join(STOP).exists();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(signal, Some(SIGTERM));
    assert!(!asked);
    assert!(
        took >= SETTLE && took < SETTLE + Duration::from_secs(2),
        "{took:?}"
    );
}
