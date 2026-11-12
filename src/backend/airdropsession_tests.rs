use super::super::airdropsend::{run, Send};
use super::*;
use crate::backend::testdir::TestDir;
use flea::ui_logic_send_share::omdrop::{Stack, Tool};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

// Runs `sh -c <loop>` as the leader of a process group of its own, as omdrop's worker is.
fn own_group(script: &str) -> String {
    format!("python3 -c 'import os, sys; os.setpgid(0, 0); os.execvp(\"sh\", [\"sh\", \"-c\", sys.argv[1]])' '{script}'")
}

struct StandIn {
    tool: Option<Tool>,
    root: TestDir,
}

impl StandIn {
    // An omdrop that, before any TERM handling of its own, starts `sender` in a group of its own and a process in a
    // session of its own, records both pids, then says it is ready.
    fn new(on_term: &str, sender: &str) -> Self {
        let root = TestDir::new("airdrop-session");
        let at = |name: &str| root.join(name).display().to_string();
        let body = format!(
            "{on_term}\n{} &\necho $! > \"{}\"\nsetsid sh -c 'echo $$ > \"{}\"; exec sleep 30' &\nwhile [ ! -s \"{}\" ]; do sleep 0.01; done\ntouch \"{}\"\nwhile :; do sleep 1; done",
            own_group(sender),
            at("sender.pid"),
            at("unrelated.pid"),
            at("unrelated.pid"),
            at("ready"),
        );
        let path = root
            .script("omdrop", &format!("#!/bin/sh\n{body}\n"))
            .to_string_lossy()
            .into_owned();
        Self {
            tool: Some(Tool {
                stack: Stack::Omdrop,
                path,
            }),
            root,
        }
    }

    fn pid(&self, name: &str) -> i32 {
        std::fs::read_to_string(self.root.join(name))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        if let Ok(text) = std::fs::read_to_string(self.root.join("unrelated.pid")) {
            if let Ok(pid) = text.trim().parse::<i32>() {
                let _ = crate::procsignal::signal_process(pid, libc::SIGKILL);
            }
        }
    }
}

// Waits for the stand-in's ready file, at most 10 s; false when it never came.
fn ready(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

// Gone, or a zombie its new parent has not reaped yet: either way it runs nothing more.
fn ended(pid: i32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let state = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next());
        if matches!(state, None | Some("Z")) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn alive(pid: i32) -> bool {
    !ended_now(pid)
}

fn ended_now(pid: i32) -> bool {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    matches!(
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next()),
        None | Some("Z")
    )
}

fn send() -> Send {
    Send {
        peer: "02:00:00:00:00:01".into(),
        paths: vec!["/x".into()],
        request: 1,
        items: Vec::new(),
    }
}

// The send as run() makes it, cancelled once the stand-in is ready.
fn cancel_through_run(stand_in: &StandIn) -> String {
    let (tx, _rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let ready_file: PathBuf = stand_in.root.join("ready");
    let stopper = std::thread::spawn(move || {
        let came = ready(&ready_file);
        flag.store(true, Ordering::Relaxed);
        came
    });
    let (code, _) = run(stand_in.tool.clone(), 3, &send(), &cancel, &tx);
    assert!(
        stopper.join().unwrap(),
        "the stand-in never started its sender"
    );
    code
}

#[test]
fn a_cancel_before_omdrop_handles_term_stops_its_sender_and_nothing_outside_its_session() {
    // Default TERM: omdrop, the session's leader, exits at once, and its sender would run on.
    let stand_in = StandIn::new("", "exec sleep 30");
    assert_eq!(cancel_through_run(&stand_in), "cancelled");
    assert!(
        ended(stand_in.pid("sender.pid")),
        "the sender outlived the cancel"
    );
    assert!(
        alive(stand_in.pid("unrelated.pid")),
        "a process in another session was killed"
    );
}

#[test]
fn a_cancel_that_omdrop_ignores_still_ends_it_and_its_sender_after_the_grace() {
    let stand_in = StandIn::new("trap '' TERM", "exec sleep 30");
    let script = stand_in.tool.clone().unwrap().path;
    // As run() starts it: the leader of a session of its own.
    let mut child = std::process::Command::new("setsid")
        .args(["-w", &script])
        .spawn()
        .unwrap();
    let came = ready(&stand_in.root.join("ready"));
    cancel(&mut child, Duration::from_millis(300), || {});
    assert!(came, "the stand-in never started its sender");
    assert!(child.try_wait().unwrap().is_some(), "omdrop was reaped");
    assert!(
        ended(stand_in.pid("sender.pid")),
        "the sender outlived the cancel"
    );
}

#[test]
fn a_sender_that_keeps_forking_leaves_no_live_process_in_the_session() {
    let stand_in = StandIn::new("", "while :; do sleep 30 & sleep 0.002; done");
    let script = stand_in.tool.clone().unwrap().path;
    let mut child = std::process::Command::new("setsid")
        .args(["-w", &script])
        .spawn()
        .unwrap();
    let sid = i32::try_from(child.id()).unwrap();
    let came = ready(&stand_in.root.join("ready"));
    cancel(&mut child, Duration::from_millis(300), || {});
    assert!(came, "the stand-in never started its sender");
    assert_eq!(
        members(sid, &proc_stat),
        Vec::<i32>::new(),
        "live processes left in the session"
    );
}

#[test]
fn the_signal_numbers_are_linuxs() {
    assert_eq!((SIGTERM, SIGKILL), (libc::SIGTERM, libc::SIGKILL));
}

#[test]
fn a_stat_line_counts_only_a_live_process_of_that_session() {
    assert!(live_in("123 (a) b) S 1 120 777 0 -1", 777));
    assert!(
        !live_in("123 (a) b) Z 1 120 777 0 -1", 777),
        "a zombie runs nothing"
    );
    assert!(!live_in("123 (sleep) S 1 120 778 0 -1", 777));
    assert!(!live_in("garbage", 777));
}

#[test]
fn a_cancel_with_proc_unreadable_still_kills_and_reaps_omdrop() {
    let stand_in = StandIn::new("trap '' TERM", "exec sleep 30");
    let script = stand_in.tool.clone().unwrap().path;
    let mut child = std::process::Command::new("setsid")
        .args(["-w", &script])
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let came = ready(&stand_in.root.join("ready"));
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        cancel_reading(&mut child, Duration::from_millis(300), || {}, &|_| String::new());
        let _ = tx.send(child.try_wait().ok().flatten().is_some());
    });
    let reaped = rx.recv_timeout(Duration::from_secs(5));
    if reaped.is_err() {
        let _ = crate::procsignal::signal_process(pid, libc::SIGKILL);
    }
    let _ = crate::procsignal::signal_process(stand_in.pid("sender.pid"), libc::SIGKILL);
    assert!(came, "the stand-in never started");
    assert_eq!(reaped, Ok(true), "cancel returned with omdrop killed and reaped");
}
