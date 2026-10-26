use super::*;
use crate::backend::testdir::TestDir;
use crate::jsondoc::{self, Json};
use std::io::{BufRead, Write};
use std::process::{Command, Stdio};

#[path = "nearby_ready_tests.rs"]
mod ready;

#[path = "nearby_adv_tests.rs"]
mod adv;

struct Fixture {
    dir: TestDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = TestDir::new("nearby");
        let output = Command::new("mktemp").args(["-d", "--"]).arg(dir.join("tools.XXXXXX")).output().unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        dir.assert_contains(&root);
        std::fs::write(root.join(".flea-nearby-test"), b"nearby test tools\n").unwrap();
        Self { dir, root }
    }

    fn script(&self, name: &str, body: &str) {
        let file = self.root.join(name);
        self.dir.assert_contains(&file);
        let relative = file.strip_prefix(self.dir.path()).unwrap().to_str().unwrap();
        let guarded = format!("#!/bin/sh\nroot=${{0%/*}}\ncase \"$root\" in /*) ;; *) exit 96 ;; esac\n[ -n \"$root\" ] && [ -f \"$root/.flea-nearby-test\" ] || exit 97\nprintf 'call %s %s\\n' \"$$\" \"$*\" >> \"$root/calls\"\n{body}\n");
        self.dir.script(relative, &guarded);
    }

    fn calls(&self) -> String { std::fs::read_to_string(self.root.join("calls")).unwrap_or_default() }

    fn wait_calls(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.calls().lines().count() < count && Instant::now() < deadline {
            std::thread::sleep(STEP);
        }
        assert!(self.calls().lines().count() >= count, "{}", self.calls());
    }

    fn wait_gone(&self) {
        let pids: Vec<_> = self.calls().lines().filter_map(|line| line.split_whitespace().nth(1)).map(str::to_owned).collect();
        assert!(!pids.is_empty());
        let deadline = Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|pid| PathBuf::from(format!("/proc/{pid}")).exists()) && Instant::now() < deadline {
            std::thread::sleep(STEP);
        }
        for pid in pids { assert!(!PathBuf::from(format!("/proc/{pid}")).exists(), "helper {pid} survived cancellation"); }
    }
}

fn result(rx: &Receiver<Event>) -> (u64, Json) {
    match rx.recv_timeout(Duration::from_secs(6)).expect("discovery must answer within its deadline") {
        Event::Nearby { ticket, line } => (ticket, jsondoc::parse(&line).unwrap()),
        _ => panic!("unexpected event"),
    }
}

fn field<'a>(value: &'a Json, key: &str) -> &'a str { value.get(key).and_then(Json::as_str).unwrap() }

#[test]
fn stubs_receive_exact_argv_and_discovery_is_idle_until_asked() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", r#"[ "$#" = 6 ] && [ "$1 $2 $3 $4 $5" = '-p -r -t -k --' ] || exit 42
case "$6" in _smb._tcp) printf '=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;\n' ;; esac"#);
    fixture.script("tailscale", r#"[ "$#" = 2 ] && [ "$1 $2" = 'status --json' ] || exit 43
printf '%s\n' '{"BackendState":"Running","Peer":{"a":{"Online":true,"DNSName":"pi.tail.ts.net.","HostName":"pi","OS":"linux"}}}'"#);
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    assert!(discovery.worker.is_none());
    assert!(fixture.calls().is_empty());
    let looking = discovery.request_with_path("start", 7, tx, Some(fixture.root.clone()));
    assert_eq!(field(&jsondoc::parse(&looking).unwrap(), "message"), LOOKING);
    assert_eq!(field(&jsondoc::parse(&looking).unwrap(), "state"), "looking");
    let (ticket, reply) = result(&rx);
    assert!(discovery.accept(ticket));
    assert_eq!(field(&reply, "state"), "found");
    let rows = reply.get("rows").and_then(Json::as_array).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(field(&rows[0], "host"), "nas.local");
    assert_eq!(field(&rows[1], "host"), "pi.tail.ts.net");
    assert!(reply.get("errors").and_then(Json::as_array).unwrap().is_empty());
    assert_eq!(fixture.calls().lines().count(), 7);
    for service in SERVICES { assert!(fixture.calls().contains(&format!("-p -r -t -k -- {service}"))); }
    discovery.close();
}

#[test]
fn missing_tools_return_nothing_nearby_and_keep_errors() {
    let fixture = Fixture::new();
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    discovery.request_with_path("start", 3, tx, Some(fixture.root.clone()));
    let (_, reply) = result(&rx);
    assert_eq!(field(&reply, "state"), "empty");
    assert_eq!(field(&reply, "message"), NOTHING);
    assert_eq!(reply.get("errors").and_then(Json::as_array).unwrap().len(), 2);
}

#[test]
fn failure_keeps_stderr_and_does_not_trust_a_failed_tailnet_reply() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", "printf 'stub daemon refused\\n' >&2\nexit 7");
    fixture.script("tailscale", r#"printf '%s\n' '{"BackendState":"Running","Peer":{"a":{"Online":true,"DNSName":"stale.tail.ts.net."}}}'
printf 'stub status refused\n' >&2
exit 7"#);
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    discovery.request_with_path("start", 3, tx, Some(fixture.root.clone()));
    let (_, reply) = result(&rx);
    assert_eq!(field(&reply, "state"), "empty");
    let rendered = jsondoc::render(&reply);
    assert!(rendered.contains("stub daemon refused"));
    assert!(rendered.contains("stub status refused"));
    assert!(!rendered.contains("stale.tail.ts.net"));
}

#[test]
fn shared_three_second_deadline_kills_all_helpers_but_keeps_resolved_mdns() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", r#"case "$6" in _smb._tcp) printf '=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;\n' ;; esac
exec /usr/bin/sleep 30"#);
    fixture.script("tailscale", "exec /usr/bin/sleep 30");
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    let started = Instant::now();
    discovery.request_with_path("start", 3, tx, Some(fixture.root.clone()));
    let (_, reply) = result(&rx);
    let elapsed = started.elapsed();
    assert!(elapsed >= LIMIT && elapsed < Duration::from_secs(6), "{elapsed:?}");
    assert_eq!(field(&reply, "state"), "found");
    assert_eq!(reply.get("rows").and_then(Json::as_array).unwrap().len(), 1);
    assert!(jsondoc::render(&reply).contains("timed out after 3 seconds"));
    fixture.wait_gone();
    eprintln!("nearby stub deadline: {:.3} seconds", elapsed.as_secs_f64());
}

#[test]
fn closing_reopening_and_owner_drop_cancel_the_active_discovery() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", "exec /usr/bin/sleep 30");
    fixture.script("tailscale", "exec /usr/bin/sleep 30");
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    discovery.request_with_path("start", 1, tx.clone(), Some(fixture.root.clone()));
    fixture.wait_calls(7);
    let old = discovery.ticket;
    discovery.request_with_path("start", 2, tx.clone(), Some(fixture.root.clone()));
    assert!(!discovery.accept(old));
    discovery.request("stop", 1, tx.clone());
    assert!(discovery.active.is_some(), "old close cannot cancel new dialog");
    fixture.wait_calls(14);
    discovery.request("stop", 2, tx.clone());
    assert!(!discovery.accept(discovery.ticket));
    fixture.wait_gone();
    assert!(rx.try_recv().is_err(), "cancelled workers must not answer");
    discovery.request_with_path("start", 3, tx, Some(fixture.root.clone()));
    fixture.wait_calls(21);
    drop(discovery);
    fixture.wait_gone();
}

#[test]
fn excess_output_is_bounded_and_empty_results_use_exact_copy() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", "exit 0");
    fixture.script("tailscale", "exec /usr/bin/head -c 1100000 -- /dev/zero");
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    discovery.request_with_path("start", 1, tx, Some(fixture.root.clone()));
    let (_, reply) = result(&rx);
    assert_eq!(field(&reply, "message"), NOTHING);
    assert_eq!(field(&reply, "state"), "empty");
    assert!(jsondoc::render(&reply).contains("output exceeded its limit"));
    fixture.wait_gone();
}

#[test]
fn helper_exit_kills_descendants_that_hold_its_output_pipe() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", r#"/usr/bin/sleep 30 &
printf 'child %s\n' "$!" >> "$root/calls"
exit 0"#);
    fixture.script("tailscale", r#"printf '%s\n' '{"BackendState":"Running","Peer":{}}'"#);
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    discovery.request_with_path("start", 1, tx, Some(fixture.root.clone()));
    let (_, reply) = result(&rx);
    assert_eq!(field(&reply, "state"), "empty");
    for line in fixture.calls().lines().filter(|line| line.starts_with("child ")) {
        let pid = line.split_whitespace().nth(1).unwrap();
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            let state = stat.rsplit_once(") ").unwrap().1.chars().next().unwrap();
            assert!(matches!(state, 'Z' | 'X'), "descendant {pid} is still running: {stat}");
        }
    }
    assert_eq!(fixture.calls().lines().filter(|line| line.starts_with("child ")).count(), 6);
}

#[test]
fn backend_worker_entry() {
    if std::env::var_os("FLEA_NEARBY_REVIEW_EXIT").is_some() {
        std::process::exit(crate::backend::run::run());
    }
    if std::env::var_os("FLEA_NEARBY_BACKEND_TEST").is_some() {
        assert_eq!(crate::backend::run::run(), 0);
    }
}

#[test]
fn actual_backend_protocol_parses_addresses_and_stops_without_discovery_on_launch() {
    let fixture = Fixture::new();
    fixture.script("avahi-browse", r#"[ ! -f "$root/hold" ] || exec /usr/bin/sleep 30
case "$6" in _smb._tcp) printf '=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;\n' ;; esac"#);
    fixture.script("tailscale", r#"[ ! -f "$root/hold" ] || exec /usr/bin/sleep 30
printf '%s\n' '{"BackendState":"Running","Peer":{}}'"#);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "backend::nearby::tests::backend_worker_entry", "--nocapture"])
        .env("FLEA_NEARBY_BACKEND_TEST", "1").env("PATH", &fixture.root).env("HOME", &fixture.root)
        .env("XDG_CONFIG_HOME", &fixture.root).env("XDG_CACHE_HOME", &fixture.root)
        .env("XDG_DATA_HOME", &fixture.root).env("XDG_RUNTIME_DIR", &fixture.root)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = jsondoc::parse(&line) { let _ = tx.send(value); }
        }
    });
    let input = child.stdin.as_mut().unwrap();
    writeln!(input, r#"{{"c":"networkaddress","id":8,"address":"gm@minipc:/srv/media"}}"#).unwrap();
    let reply = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(field(&reply, "uri"), "sftp://gm@minipc/srv/media");
    assert!(fixture.calls().is_empty());
    writeln!(input, r#"{{"c":"nearby","op":"start","id":9}}"#).unwrap();
    let looking = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(field(&looking, "message"), LOOKING);
    assert_eq!(field(&looking, "state"), "looking");
    let found = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(field(&found, "state"), "found");
    assert_eq!(field(&found.get("rows").and_then(Json::as_array).unwrap()[0], "host"), "nas.local");
    fixture.wait_calls(7);
    let hold = fixture.root.join("hold");
    fixture.dir.assert_contains(&hold);
    std::fs::write(hold, b"hold helpers\n").unwrap();
    writeln!(input, r#"{{"c":"nearby","op":"start","id":10}}"#).unwrap();
    assert_eq!(field(&rx.recv_timeout(Duration::from_secs(5)).unwrap(), "state"), "looking");
    fixture.wait_calls(14);
    writeln!(input, r#"{{"c":"nearby","op":"stop","id":10}}"#).unwrap();
    assert_eq!(field(&rx.recv_timeout(Duration::from_secs(5)).unwrap(), "state"), "stopped");
    fixture.wait_gone();
    writeln!(input, r#"{{"c":"quit"}}"#).unwrap();
    child.stdin.take();
    assert!(matches!(super::super::child::wait_for_exit(&child, Instant::now() + Duration::from_secs(5)), super::super::child::Woke::Exited));
    assert!(child.wait().unwrap().success());
}

#[test]
fn reviewer_backend_exit_stops_active_helpers() {
    let mut survivors = Vec::new();
    for eof in [true, false] {
        for attempt in 0..20 {
            let fixture = Fixture::new();
            fixture.script("avahi-browse", "exec /usr/bin/sleep 30");
            fixture.script("tailscale", "exec /usr/bin/sleep 30");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "backend::nearby::tests::backend_worker_entry", "--nocapture"])
                .env("FLEA_NEARBY_REVIEW_EXIT", "1").env("PATH", &fixture.root).env("HOME", &fixture.root)
                .env("XDG_CONFIG_HOME", &fixture.root).env("XDG_CACHE_HOME", &fixture.root)
                .env("XDG_DATA_HOME", &fixture.root).env("XDG_RUNTIME_DIR", &fixture.root)
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
            writeln!(child.stdin.as_mut().unwrap(), r#"{{"c":"nearby","op":"start","id":9}}"#).unwrap();
            fixture.wait_calls(7);
            if !eof { writeln!(child.stdin.as_mut().unwrap(), r#"{{"c":"quit"}}"#).unwrap(); }
            child.stdin.take();
            assert!(matches!(super::super::child::wait_for_exit(&child, Instant::now() + Duration::from_secs(5)), super::super::child::Woke::Exited));
            assert!(child.wait().unwrap().success());
            std::thread::sleep(Duration::from_millis(100));
            let live: Vec<_> = fixture.calls().lines().filter_map(|line| line.split_whitespace().nth(1)).filter_map(|pid| {
                let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
                let state = stat.rsplit_once(") ")?.1.chars().next()?;
                (!matches!(state, 'Z' | 'X')).then(|| pid.to_string())
            }).collect();
            if !live.is_empty() {
                eprintln!("REVIEW active helpers survived backend exit: eof={eof}, attempt={attempt}, pids={live:?}");
                fixture.dir.assert_contains(&fixture.root);
                for pid in &live {
                    assert!(Command::new("/usr/bin/kill").args(["-KILL", "--", pid]).status().unwrap().success());
                }
                survivors.push((eof, attempt, live));
            }
        }
    }
    assert!(survivors.is_empty(), "backend left active discovery helpers running: {survivors:?}");
}

#[test]
fn reviewer_cancelled_tailnet_parse_does_not_consume_next_scan_deadline() {
    let fixture = Fixture::new();
    let status_file = fixture.root.join("status.json");
    fixture.dir.assert_contains(&status_file);
    let mut status = String::from("{\"BackendState\":\"Running\",\"Peer\":{");
    for index in 0..90_000 {
        if index != 0 { status.push(','); }
        use std::fmt::Write;
        write!(status, "\"{index:05}\":{{}}").unwrap();
    }
    status.push_str("}}");
    assert!(status.len() < 1024 * 1024);
    std::fs::write(&status_file, &status).unwrap();
    fixture.script("avahi-browse", "exit 0");
    fixture.script("tailscale", "exec /usr/bin/cat -- \"$root/status.json\"");
    let (tx, rx) = crate::backend::events::channel();
    let mut discovery = Discovery::default();
    let started = Instant::now();
    discovery.request_with_path("start", 1, tx.clone(), Some(fixture.root.clone()));
    fixture.wait_calls(7);
    std::thread::sleep(Duration::from_millis(500));
    std::fs::write(&status_file, r#"{"BackendState":"Running","Peer":{"fresh":{"Online":true,"DNSName":"fresh.tail.ts.net."}}}"#).unwrap();
    discovery.request("stop", 1, tx.clone());
    let reopened = Instant::now();
    discovery.request_with_path("start", 2, tx, Some(fixture.root.clone()));
    loop {
        let (ticket, reply) = result(&rx);
        if !discovery.accept(ticket) { continue; }
        eprintln!("REVIEW reopened scan: total={:?}, reopen={:?}, calls={}, reply={}", started.elapsed(), reopened.elapsed(), fixture.calls().lines().count(), jsondoc::render(&reply).trim());
        fixture.wait_gone();
        assert_eq!(field(&reply, "state"), "found", "the reopened dialog lost its discovery before any fresh helpers could start");
        break;
    }
}
