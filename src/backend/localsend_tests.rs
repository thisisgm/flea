use super::*;

// A substring test read a device named after the event as the event itself, and a CLI that ended is not one that sent.
#[test]
fn only_the_transfer_s_own_event_line_is_a_transfer() {
    let discovered = "D [1] Sent Phone (10.0.0.4)\nD [2] Failed Phone (10.0.0.5)";
    assert!(sent(discovered, "Sent Phone").is_none());
    assert!(sent(discovered, "Failed Phone").is_none());
    assert!(sent("S Sent Phone: Sent 1 file (61 B, took 0s)", "Sent Phone").expect("a verdict").is_ok());
    let refused = sent("S Clean Lemon: Failed to send: connection refused", "Clean Lemon").expect("a verdict");
    assert!(refused.unwrap_err().contains("connection refused"));
    // The peer's own name is what anchors it, so another device's line is not this one's.
    assert!(sent("S Other Box: Sent 1 file (61 B, took 0s)", "Clean Lemon").is_none());
}

#[test]
fn nothing_to_send_is_refused_before_a_terminal_is_opened() {
    assert!(send("Clean Lemon", &[], Duration::from_millis(1)).is_err());
}

// Measured on localsend-cli 1.18.2: a decline is its own event line, then the CLI exits 0.
#[test]
fn a_decline_and_a_failed_upload_each_say_what_happened() {
    let declined = sent("S Clean Lemon: Declined", "Clean Lemon").expect("a verdict");
    assert_eq!(declined.unwrap_err(), "Clean Lemon declined the transfer.");
    let failed = "S Clean Lemon: Failed to upload x.bin: error sending request for url (https://10.0.0.2:53317/api/localsend/v2/upload?sessionId=1&token=2)";
    assert_eq!(sent(failed, "Clean Lemon").expect("a verdict").unwrap_err(),
               "LocalSend could not send to Clean Lemon: Failed to upload x.bin: error sending request.");
    // Measured after a transfer the network cut off: the receiver still holds that session.
    let blocked = sent("S Clean Lemon: Blocked by another session", "Clean Lemon").expect("a verdict");
    assert_eq!(blocked.unwrap_err(), "Clean Lemon is busy with another transfer. Try again when it ends.");
    // Words this bridge has no sentence for are no verdict while the CLI runs, and its reason once it ends.
    assert!(sent("S Clean Lemon: Something new", "Clean Lemon").is_none());
    assert_eq!(said("S Clean Lemon: Something new", "Clean Lemon"), Some("Something new"));
}

#[test]
fn a_progress_line_gives_what_has_moved_and_another_device_gives_nothing() {
    let log = "S Clean Lemon [####----] 241.2 MB / 307.2 MB [12.5 MB/s] [ETA: 5s]\nS Clean Lemon [#####---] 243.3 MB / 307.2 MB [11.2 MB/s] [ETA: 5s]";
    assert_eq!(progress(log, "Clean Lemon").as_deref(), Some("243.3 MB / 307.2 MB"));
    assert_eq!(progress(log, "Clean"), None);
    assert_eq!(progress("S Clean Lemon: Sent 1 file (61 B, took 0s)", "Clean Lemon"), None);
}

// localsend-cli answers a folder with "Not a file"; the send names the folder before any terminal opens.
#[test]
fn a_folder_is_refused_by_name_before_a_terminal_is_opened() {
    let dir = crate::backend::testdir::TestDir::new("localsend-folder");
    let folder = dir.path().join("Holiday photos");
    std::fs::create_dir(&folder).expect("a folder");
    let error = send("Clean Lemon", &[folder.to_string_lossy().into_owned()], Duration::from_millis(1)).unwrap_err();
    assert_eq!(error, "Holiday photos is a folder, and LocalSend sends only files.");
}

// Two runs at once would both bind 53317; the second waits its turn, and a waiting send still cancels.
#[test]
fn a_second_run_waits_for_the_first_and_a_cancel_ends_the_wait() {
    // A lock file of the test's own, so a Flea running on the box neither blocks nor is blocked by it.
    let dir = crate::backend::testdir::TestDir::new("localsend-turn");
    let lock = dir.path().join("localsend.lock");
    let open = || std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock).expect("the lock file");
    let never = AtomicBool::new(false);
    let Turn::Held(Some(first)) = wait_turn(open(), &never, Some(Duration::ZERO)) else { panic!("a free lock was not taken") };
    assert!(matches!(wait_turn(open(), &never, Some(Duration::ZERO)), Turn::Busy));
    assert!(matches!(wait_turn(open(), &AtomicBool::new(true), None), Turn::Cancelled));
    drop(first);
    assert!(matches!(wait_turn(open(), &never, Some(Duration::ZERO)), Turn::Held(Some(_))));
}

// The zero line drawn while the other machine decides is not movement; once bytes move, only a stall ends the send.
#[test]
fn the_accept_clock_stops_once_bytes_move_and_a_stall_takes_over() {
    let (passed, mut watch) = (Instant::now(), Watch::default());
    watch.see(None);
    watch.see(Some("0 B / 209.7 MB".into()));
    watch.see(Some("0 B / 209.7 MB".into()));
    assert_eq!(watch.late("Clean Lemon", passed).as_deref(), Some("Clean Lemon did not accept the transfer."));
    watch.see(Some("12.5 MB / 209.7 MB".into()));
    assert_eq!(watch.late("Clean Lemon", passed), None, "a moving transfer is cut off by the accept clock");
    // The same line read again is no movement, so the stall clock keeps the time the line first changed.
    let long_ago = Instant::now().checked_sub(STALL).expect("an uptime past the stall limit");
    watch.moved = Some(("12.5 MB / 209.7 MB".into(), long_ago));
    watch.see(Some("12.5 MB / 209.7 MB".into()));
    assert_eq!(watch.late("Clean Lemon", passed).as_deref(), Some("Clean Lemon stopped receiving the transfer."));
    watch.see(Some("14.0 MB / 209.7 MB".into()));
    assert_eq!(watch.late("Clean Lemon", passed), None);
}

// A stand-in device panel drawing each move as two writes `gap` apart, the old row plain first, as a pty read can split it.
fn fake_panel(names: &'static [&'static str], start: usize, gap: Duration) -> (OwnedFd, Arc<Mutex<Screen>>, std::thread::JoinHandle<usize>) {
    let draw = move |row: usize, lit: bool| {
        format!("\x1b[{};2H{}  [{}] {} (10.0.0.{})\x1b[0m", row + 2, if lit { "\x1b[48;5;8m" } else { "\x1b[49m" }, row + 1, names[row], row + 1)
    };
    let seen = Arc::new(Mutex::new(Screen::new(ROWS as usize, COLS as usize)));
    let panel: String = (0..names.len()).map(|row| draw(row, row == start)).collect();
    seen.lock().expect("the screen").feed(format!("\x1b[?1049h{}", panel).as_bytes());
    let (ours, mut theirs) = std::os::unix::net::UnixStream::pair().expect("a socket pair");
    let screen = Arc::clone(&seen);
    let drawing = std::thread::spawn(move || {
        let (mut lit, mut key) = (start, [0u8; 3]);
        while theirs.read_exact(&mut key).is_ok() {
            let next = match &key { b"\x1b[B" => (lit + 1).min(names.len() - 1), b"\x1b[A" => lit.saturating_sub(1), _ => lit };
            screen.lock().expect("the screen").feed(draw(lit, false).as_bytes());
            std::thread::sleep(gap);
            lit = next;
            screen.lock().expect("the screen").feed(draw(lit, true).as_bytes());
        }
        lit
    });
    (OwnedFd::from(ours), seen, drawing)
}

// The count-from-the-top fallback took a mid-redraw read as no lit row and would have sent to the fifth device.
#[test]
fn the_lit_row_reaches_the_wanted_device_when_a_read_lands_mid_redraw() {
    let (keys, seen, drawing) = fake_panel(&["Peer A", "Peer B", "Peer C", "Peer D", "Peer E"], 0, Duration::from_millis(150));
    choose(&keys, &seen, "Peer D", Instant::now() + Duration::from_secs(10)).expect("Peer D is chosen");
    drop(keys);
    assert_eq!(drawing.join().expect("the stand-in panel"), 3, "the lit row is not on Peer D");
}

// A redraw slower than REDRAW leaves no row lit after the key's wait, and is still waited out rather than counted from the top.
#[test]
fn a_redraw_slower_than_the_key_wait_is_never_counted_from_the_top() {
    let (keys, seen, drawing) = fake_panel(&["Peer A", "Peer B", "Peer C"], 0, REDRAW + REDRAW / 4);
    choose(&keys, &seen, "Peer B", Instant::now() + Duration::from_secs(10)).expect("Peer B is chosen");
    drop(keys);
    assert_eq!(drawing.join().expect("the stand-in panel"), 1, "the lit row is not on Peer B");
}

#[test]
fn the_lit_row_climbs_to_a_device_above_it_and_a_missing_device_is_gone() {
    let (keys, seen, drawing) = fake_panel(&["Peer A", "Peer B", "Peer C"], 2, Duration::from_millis(150));
    choose(&keys, &seen, "Peer A", Instant::now() + Duration::from_secs(10)).expect("Peer A is chosen");
    let missing = choose(&keys, &seen, "Peer Z", Instant::now() + Duration::from_secs(10)).unwrap_err();
    assert_eq!(missing, "Peer Z is not answering on this network any more.");
    drop(keys);
    assert_eq!(drawing.join().expect("the stand-in panel"), 0, "the lit row is not on Peer A");
}

// A child that inherited the pty master kept the terminal alive after the backend died.
#[test]
fn the_cli_does_not_inherit_the_terminal_master() {
    let (master, slave) = open_pty().expect("a pty");
    let mut child = spawn("/usr/bin/sleep", &["600".to_string()], &slave).expect("a stand-in CLI");
    // O_CLOEXEC closes the descriptor at exec, so the scan waits until the stand-in is itself.
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::fs::read_to_string(format!("/proc/{}/comm", child.id())).map(|c| c.trim() != "sleep").unwrap_or(true) {
        assert!(Instant::now() < deadline, "the stand-in CLI never exec'd");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child.try_wait().expect("try_wait").is_none(), "the stand-in CLI exited before the scan");
    let mut inherited = Vec::new();
    for fd in std::fs::read_dir(format!("/proc/{}/fd", child.id())).expect("the child's descriptor table") {
        let fd = fd.expect("a descriptor entry");
        // sleep closes its locale files right after exec, so a listed descriptor can vanish; an inherited master never would.
        let target = match std::fs::read_link(fd.path()) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            other => other.expect("a descriptor target"),
        };
        if target.to_string_lossy().contains("ptmx") { inherited.push(fd.path()) }
    }
    stop(&mut child);
    drop(master);
    assert!(inherited.is_empty(), "the CLI holds the terminal master at {:?}", inherited);
}
const LIFECYCLE_MARKER: &str = "FLEA_LOCALSEND_LIFECYCLE_PROBE";
const LIFECYCLE_TEST: &str = "backend::localsend::tests::a_child_cli_does_not_outlive_its_parent";

// Field 3 (state) and field 22 (start time) of /proc/pid/stat, split after the last ')' so a comm cannot shift them.
// Sample input: "4242 (localsend) S 4100 4242 4100 0 -1 4194560 812 0 0 0 3 1 0 0 20 0 4 0 9876543 ..." -> ("S", 9876543).
fn proc_stat(pid: i32) -> Option<(String, u64)> {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(ESRCH) => return None,
        other => other.unwrap_or_else(|e| panic!("pid {pid} exists but its stat is unreadable: {e}")),
    };
    let mut fields = stat.rsplit(')').next().expect("a stat tail").split_whitespace();
    let state = fields.next().expect("a stat state").to_string();
    Some((state, fields.nth(18).expect("a stat start time").parse().expect("a numeric start time")))
}

// A reused pid has another start time and a zombie waits to be reaped, so neither is a CLI that survived.
fn child_running(pid: i32, start: u64) -> bool {
    proc_stat(pid).is_some_and(|(state, began)| state != "Z" && began == start)
}

#[test]
fn a_live_pid_with_another_start_time_is_not_the_child() {
    let mut sleeper = std::process::Command::new("sleep").arg("5").spawn().expect("a sleep");
    let pid = sleeper.id() as i32;
    let start = proc_stat(pid).expect("the sleep's stat").1;
    let (other, own) = (child_running(pid, start + 1), child_running(pid, start));
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    assert!(!other, "a live pid with another start time was read as the child");
    assert!(own, "a live pid with its own start time was not read as the child");
}

// The probe ends with a stand-in behind, like a killed backend; only PDEATHSIG can end the HUP-ignoring child.
#[test]
fn a_child_cli_does_not_outlive_its_parent() {
    if let Some(marker) = std::env::var_os(LIFECYCLE_MARKER) {
        let marker = std::path::PathBuf::from(marker);
        let ready = marker.with_extension("ready");
        let (_master, slave) = open_pty().expect("a pty");
        let child = spawn("/bin/sh", &["-c".to_string(), "trap '' HUP; : > \"$1\"; exec sleep 600".to_string(),
            "localsend-life".to_string(), ready.to_string_lossy().into_owned()], &slave)
            .expect("the probe starts a stand-in CLI");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "the stand-in never armed its HUP ignore");
            std::thread::sleep(Duration::from_millis(10));
        }
        let start = proc_stat(child.id() as i32).expect("the stand-in's stat").1;
        std::fs::write(&marker, format!("{} {}", child.id(), start)).expect("the probe writes its child's identity");
        // The stand-in must outlive this probe, so its handle is relinquished rather than reaped.
        std::mem::forget(child);
        return;
    }
    let dir = crate::backend::testdir::TestDir::new("localsend-life");
    let marker = dir.path().join("child.id");
    let out = std::process::Command::new(std::env::current_exe().expect("a test binary knows its own path"))
        .args(["--exact", "--test-threads=1", "--nocapture", LIFECYCLE_TEST])
        .env(LIFECYCLE_MARKER, &marker)
        .output()
        .expect("the test binary re-executes");
    let report = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "the probe child failed: {}", report);
    // Renaming the test would filter the child to nothing and exit 0, so it has to say it ran one.
    assert!(report.contains("1 passed"), "the probe child ran no test: {}", report);
    let identity = std::fs::read_to_string(&marker).expect("the probe wrote its child's identity");
    let (pid, start) = identity.split_once(' ').expect("a pid and a start time");
    let (pid, start): (i32, u64) = (pid.parse().expect("a pid"), start.trim().parse().expect("a start time"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while child_running(pid, start) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    // Only the pid that started at `start` is killed, so a reused pid is never touched.
    if child_running(pid, start) {
        let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
        let killed = Instant::now() + Duration::from_secs(5);
        while child_running(pid, start) && Instant::now() < killed {
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("a CLI survived the backend's death: pid {pid}");
    }
}
