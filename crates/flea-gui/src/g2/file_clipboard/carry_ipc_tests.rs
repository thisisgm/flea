//! Two real socket endpoints in isolated processes, driving the production Shell IPC and landing path.
use super::m9_g2rows_25_rig::{answer, edit, open, reply};
use crate::ipc::server::{Answer, Event, Server};
use flea::ipc::{
    client,
    wire::{Reply, Request},
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};

const CHILD: &str = "g2::file_clipboard::carry_ipc_tests::carry_ipc_fixture_child";

#[path = "carry_lifetime_tests.rs"]
mod lifetime;

fn request(function: &str, arguments: Vec<String>) -> Request {
    Request::Call {
        target: "flea".into(),
        function: function.into(),
        arguments,
    }
}

#[test]
fn carry_ipc_fixture_child() {
    let Ok(role) = std::env::var("FLEA_CARRY_FIXTURE") else {
        return;
    };
    let runtime = flea::ipc::endpoint::runtime().unwrap();
    assert!(runtime.join(".flea-test-sandbox").is_file());
    let folder = runtime.join(if role == "source" { "source" } else { "dest" });
    let server = Server::bind(&runtime).unwrap();
    let mut cx = crate::list::bench::real_cx();
    let names: &[&str] = if role == "source" {
        &["a.txt"]
    } else {
        &["folder"]
    };
    let (window, mut lines) = open(&mut cx, folder.to_str().unwrap(), names);
    if role == "source" {
        edit(&mut cx, window, |shell, window, cx| {
            shell.carry_lift(window, cx)
        });
        answer(
            &mut cx,
            window,
            &[runtime.join("source/a.txt").to_str().unwrap()],
        );
    }
    lines();
    while let Ok(event) = server.events.recv_blocking() {
        let Event::Call {
            request: call,
            answer,
        } = event
        else {
            break;
        };
        let function = match &call {
            Request::Call { function, .. } => function.as_str(),
            _ => "",
        };
        let reply = match function {
            "fixtureQuit" => {
                answer
                    .send(Answer {
                        reply: Reply::value("true"),
                        quit: false,
                    })
                    .unwrap();
                break;
            }
            "fixtureActive" => {
                if role != "source" {
                    cx.run_until_parked();
                }
                let active = edit(&mut cx, window, |shell, _, cx| {
                    shell.carry_riding_or_pending(cx)
                });
                Some(Reply::value(active.to_string()))
            }
            "fixtureAdvance" => {
                let Request::Call { arguments, .. } = &call else {
                    unreachable!()
                };
                let seconds = arguments[0].parse::<u64>().unwrap();
                // Queued work runs at the old time before wall and executor time move together.
                cx.run_until_parked();
                super::carry_wire::TEST_CLOCK_SKEW_MS
                    .fetch_add(seconds * 1000, std::sync::atomic::Ordering::Relaxed);
                cx.executor().advance_clock(Duration::from_secs(seconds));
                cx.run_until_parked();
                Some(Reply::value("true"))
            }
            "fixtureLand" => {
                edit(&mut cx, window, |shell, window, cx| {
                    shell.carry_land(
                        0,
                        runtime.join("dest/folder").to_str().unwrap(),
                        false,
                        false,
                        window,
                        cx,
                    )
                });
                cx.run_until_parked();
                let mut sent = lines();
                if sent.iter().any(|line| line.contains(r#""c":"collisions""#)) {
                    reply(
                        &mut cx,
                        window,
                        r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#,
                    );
                    cx.run_until_parked();
                    sent.extend(lines());
                }
                Some(Reply::value(sent.join("\n")))
            }
            "fixtureEscape" => {
                edit(&mut cx, window, |shell, window, cx| {
                    shell.carry_escape(window, cx)
                });
                if role != "source" {
                    cx.run_until_parked();
                }
                Some(Reply::value("true"))
            }
            _ => None,
        };
        if let Some(reply) = reply {
            answer.send(Answer { reply, quit: false }).unwrap();
        } else {
            edit(&mut cx, window, |shell, window, cx| {
                shell.ipc(
                    Event::Call {
                        request: call,
                        answer,
                    },
                    window,
                    cx,
                )
            });
            // The source's synchronous IPC grants stay independent of the destination's background socket calls.
            if role != "source" {
                cx.run_until_parked();
            }
        }
    }
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let output = Command::new("mktemp")
            .args(["-d", "/tmp/flea-carry.XXXXXX"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join(".flea-test-sandbox"), "carry IPC regression\n").unwrap();
        fs::create_dir(root.join("source")).unwrap();
        fs::create_dir_all(root.join("dest/folder")).unwrap();
        fs::write(root.join("source/a.txt"), "captured carry\n").unwrap();
        Self { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.root.is_absolute()
                && self.root.parent() == Some(Path::new("/tmp"))
                && self.root.join(".flea-test-sandbox").is_file()
        );
        fs::remove_dir_all(&self.root).unwrap();
    }
}

struct Endpoint {
    child: Child,
    path: PathBuf,
}

impl Endpoint {
    fn new(root: &Path, role: &str) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env("FLEA_CARRY_FIXTURE", role)
            .env("XDG_RUNTIME_DIR", root)
            .stdout(fs::File::create(root.join(format!("{role}.log"))).unwrap())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let path = root.join(format!("flea-ipc/{}.sock", child.id()));
        let mut endpoint = Self { child, path };
        let until = Instant::now() + Duration::from_secs(15);
        while !endpoint.path.exists() {
            assert!(
                endpoint.child.try_wait().unwrap().is_none(),
                "fixture exited: {}",
                fs::read_to_string(root.join(format!("{role}.log"))).unwrap()
            );
            assert!(Instant::now() < until, "fixture endpoint did not bind");
            std::thread::sleep(Duration::from_millis(10));
        }
        endpoint
    }

    fn call(&self, function: &str, arguments: Vec<String>) -> String {
        let reply = client::exchange(&self.path, &request(function, arguments)).unwrap();
        assert_eq!(reply.code, 0, "{}", reply.stderr);
        reply.stdout.trim().to_owned()
    }

    fn offer_to(&self, dest: &Endpoint) -> String {
        let offer = self.call("carryPeek", vec![String::new()]);
        let parsed =
            flea::jsondoc::parse(&offer).expect("SOURCE_MUST_EXPOSE_CAPTURED_CARRY_OVER_IPC");
        let token = parsed.get("token").unwrap().as_str().unwrap().to_owned();
        assert_eq!(
            dest.call(
                "carryOffer",
                vec![self.child.id().to_string(), token.clone()]
            ),
            "true"
        );
        assert_eq!(
            dest.call("fixtureActive", vec![]),
            "true",
            "PEER_MUST_SEE_CARRY"
        );
        token
    }

    // The process dies without a goodbye, as a crashed or killed source does.
    fn lose(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        std::mem::forget(self);
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = client::exchange(&self.path, &request("fixtureQuit", vec![]));
        if std::thread::panicking() {
            let _ = self.child.kill();
        }
        let status = self.child.wait().unwrap();
        if !std::thread::panicking() {
            assert!(status.success());
        }
    }
}

#[test]
fn carry_ipc_two_endpoints_land_once_and_clear_the_source() {
    let fixture = Fixture::new();
    let source = Endpoint::new(&fixture.root, "source");
    let dest = Endpoint::new(&fixture.root, "dest");
    let token = source.offer_to(&dest);
    let sent = dest.call("fixtureLand", vec![]);
    assert!(
        sent.contains(r#""c":"collisions""#)
            && sent.contains(r#""op":"move""#)
            && sent.contains(fixture.root.join("source/a.txt").to_str().unwrap())
            && sent.contains(fixture.root.join("dest/folder").to_str().unwrap()),
        "IPC_CARRY_MUST_USE_OP1: {sent}"
    );
    assert_eq!(
        source.call("fixtureActive", vec![]),
        "false",
        "SOURCE_CARRY_MUST_CLEAR"
    );
    assert_eq!(
        source.call("carryClaim", vec![token]),
        "false",
        "A_SECOND_LANDING_MUST_BE_REFUSED"
    );
    assert!(!dest
        .call("fixtureLand", vec![])
        .contains(r#""c":"collisions""#));
}

#[test]
fn carry_ipc_peer_escape_cancels_the_source_and_its_offer() {
    let fixture = Fixture::new();
    let source = Endpoint::new(&fixture.root, "source");
    let dest = Endpoint::new(&fixture.root, "dest");
    let token = source.offer_to(&dest);
    assert_eq!(dest.call("fixtureEscape", vec![]), "true");
    assert_eq!(source.call("fixtureActive", vec![]), "false");
    assert_eq!(
        source.call("carryPeek", vec![token.clone()]),
        "",
        "CANCELLED_SOURCE_MUST_STOP_SERVING_ITS_OFFER"
    );
    dest.call(
        "carryOffer",
        vec![source.child.id().to_string(), token.clone()],
    );
    assert_eq!(dest.call("fixtureActive", vec![]), "false");
    assert_eq!(source.call("carryClaim", vec![token]), "false");
    assert!(!dest
        .call("fixtureLand", vec![])
        .contains(r#""c":"collisions""#));
}

#[test]
fn carry_ipc_source_grants_exactly_one_claim() {
    let fixture = Fixture::new();
    let source = Endpoint::new(&fixture.root, "source");
    let offer = source.call("carryPeek", vec![String::new()]);
    let token = flea::jsondoc::parse(&offer)
        .unwrap()
        .get("token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(source.call("carryClaim", vec![token.clone()]), "true");
    assert_eq!(source.call("carryClaim", vec![token]), "false");
    assert_eq!(source.call("fixtureActive", vec![]), "false");
}

#[test]
fn carry_ipc_landing_without_the_source_grant_moves_nothing() {
    let fixture = Fixture::new();
    let source = Endpoint::new(&fixture.root, "source");
    let dest = Endpoint::new(&fixture.root, "dest");
    source.offer_to(&dest);
    source.lose();
    assert_eq!(
        dest.call("fixtureActive", vec![]),
        "true",
        "PEER_COPY_MUST_RIDE_UNTIL_THE_CLAIM"
    );
    let sent = dest.call("fixtureLand", vec![]);
    assert!(
        !sent.contains(r#""c":"collisions""#) && !sent.contains(r#""op":"move""#),
        "UNGRANTED_LANDING_MUST_NOT_REACH_OP1: {sent}"
    );
    assert_eq!(dest.call("fixtureActive", vec![]), "false");
}
