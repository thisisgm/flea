//! Bare-server listing uses the service's real timer and kills the real wedged child.
use super::mount::{Effect, Mounts, Phase, Request};
use super::service::{Service, ServiceEvent};
use crate::g2::device_rail::bfx_net_tests::{answer, Root};
use crate::g2::device_rail::process::{Event, Handle, Job, Outcome, Runner, ThreadRunner};
use crate::g2::device_rail::secret::Secret;
use gpui::AppContext;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn request(id: u64) -> Request {
    Request {
        id,
        uri: "smb://fixture/".into(),
        label: "Fixture".into(),
        origin: 7,
        mounted: false,
        authenticated: false,
        password: Secret::new(),
    }
}

/// Each finished job's argv and outcome, in exit order.
type Exits = Arc<Mutex<Vec<(Vec<String>, Outcome)>>>;

/// Runner events not yet delivered on the test thread, by job index; None ends that job's stream.
type Pending = Arc<Mutex<Vec<(usize, Option<Event>)>>>;

struct Gio {
    script: String,
    root: String,
    jobs: RefCell<Vec<Vec<String>>>,
    exits: Exits,
    senders: RefCell<Vec<Option<async_channel::Sender<Event>>>>,
    pending: Pending,
}

impl Gio {
    // Delivers runner events on the test thread; the test scheduler rejects wakes from other threads.
    fn pump(&self) {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        let mut senders = self.senders.borrow_mut();
        for (job, event) in pending {
            match event {
                Some(event) => drop(senders[job].as_ref().unwrap().try_send(event)),
                None => senders[job] = None,
            }
        }
    }
}

impl Runner for Gio {
    fn start(&self, mut job: Job) -> Handle {
        let argv = job.argv.clone();
        assert_eq!(argv[0], "gio");
        self.jobs.borrow_mut().push(argv.clone());
        job.argv[0] = self.script.clone();
        job.env.push(("BFX_NET_ROOT".into(), self.root.clone()));
        let mut handle = ThreadRunner.start(job);
        let (send, receive) = async_channel::unbounded();
        let events = std::mem::replace(&mut handle.events, receive);
        let exits = self.exits.clone();
        let pending = self.pending.clone();
        let job = self.senders.borrow().len();
        self.senders.borrow_mut().push(Some(send));
        std::thread::spawn(move || {
            while let Ok(event) = events.recv_blocking() {
                if let Event::Exited(out) = &event {
                    exits.lock().unwrap().push((argv.clone(), out.clone()));
                }
                pending.lock().unwrap().push((job, Some(event)));
            }
            pending.lock().unwrap().push((job, None));
        });
        handle
    }
}

#[gpui::test]
fn bfx_net_bare_server_list_has_a_wall_deadline_and_reaps_the_child(cx: &mut gpui::TestAppContext) {
    let root = Root::new();
    let script = root.script(
        "gio",
        "#!/bin/sh\ncase \"$1\" in\n  mount|info) exit 0 ;;\n  list) printf '%s\\n' \"$$\" > \"$BFX_NET_ROOT/list.pid\"; sleep 60 & wait ;;\n  *) exit 64 ;;\nesac\n",
    );
    let runner = Rc::new(Gio {
        script,
        root: root.0.to_str().unwrap().into(),
        jobs: RefCell::default(),
        exits: Arc::default(),
        senders: RefCell::default(),
        pending: Arc::default(),
    });
    let service = cx.new(|_| Service::new(runner.clone(), String::new(), String::new()));
    let completions = Rc::new(RefCell::new(Vec::new()));
    let seen = completions.clone();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.subscribe(&service, move |_, event: &ServiceEvent, _| {
            if let Effect::Completed { id, error, .. } = &event.0 {
                seen.borrow_mut().push((*id, *error));
            }
        })
        .detach();
        service.update(cx, |service, cx| service.open_share(request(41), 0, cx));
    });
    let start = Instant::now();
    let mut elapsed = Duration::ZERO;
    // Advance the test executor by elapsed wall time, so the 15-second deadline is exercised in real time.
    while start.elapsed() < Duration::from_secs(18) {
        runner.pump();
        cx.run_until_parked();
        if !completions.borrow().is_empty() && runner.exits.lock().unwrap().len() == 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
        let now = start.elapsed();
        cx.executor().advance_clock(now - elapsed);
        elapsed = now;
    }
    let took = start.elapsed();
    let busy = cx.update(|_, cx| service.read(cx).mounts.busy());
    let shutdown = cx.update(|_, cx| service.update(cx, |service, cx| service.shutdown(cx)));
    let cleanup = Instant::now();
    while !shutdown.is_ready() && cleanup.elapsed() < Duration::from_secs(2) {
        runner.pump();
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
    let drained = shutdown.is_ready();
    if !drained {
        let pid: u32 = std::fs::read_to_string(root.0.join("list.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(pid > 1 && root.0.join(".flea-test-sandbox").is_file());
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .status();
        let cleanup = Instant::now();
        while !shutdown.is_ready() && cleanup.elapsed() < Duration::from_secs(2) {
            runner.pump();
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(
        drained,
        "the deadline must cancel inherited pipes before cleanup"
    );
    let jobs = runner.jobs.borrow();
    assert_eq!(jobs.len(), 3);
    assert_eq!(
        jobs[0],
        ["gio", "mount", "--anonymous", "--", "smb://fixture/"]
    );
    assert_eq!(jobs[1], ["gio", "info", "--", "smb://fixture/"]);
    assert_eq!(jobs[2], ["gio", "list", "--", "smb://fixture/"]);
    assert!(
        root.0.join("list.pid").is_file(),
        "the bare-server list really started"
    );
    assert_eq!(&*completions.borrow(), &[(41, Some("network.no_answer"))]);
    assert!(!busy, "timeout must release single-flight ownership");
    let exits = runner.exits.lock().unwrap();
    assert_eq!(exits.len(), 3);
    assert_eq!(
        exits[2].1.code, -1,
        "the deadline killed and reaped the wedged list leader"
    );
    assert!(
        took >= Duration::from_secs(15) && took < Duration::from_secs(18),
        "wall deadline: {took:?}"
    );
    println!("bare-server list wall deadline {took:?}; child reaped; shutdown drained");
}

fn run(effects: Vec<Effect>, mounts: &mut Mounts, stdout: &str, now: u64) -> Vec<Effect> {
    let Effect::Run {
        generation,
        phase,
        job,
    } = effects.into_iter().next().unwrap()
    else {
        panic!("expected a helper process");
    };
    mounts.exited(generation, phase, &answer(job, stdout, ""), now)
}

#[test]
fn bfx_net_quiet_share_list_cannot_reopen_previous_shares() {
    let mut mounts = Mounts::default();
    for (id, names) in [(1, "docs\nmedia\n"), (2, "")] {
        let effects = mounts.open_share(request(id), 0);
        let effects = run(effects, &mut mounts, "", 1);
        let effects = run(effects, &mut mounts, "", 2);
        assert!(matches!(
            &effects[0],
            Effect::Run {
                phase: Phase::Shares,
                ..
            }
        ));
        let effects = run(effects, &mut mounts, names, 3);
        if names.is_empty() {
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::Completed {
                    id: 2,
                    error: Some(super::mount::GUEST_EMPTY),
                    ..
                }
            )));
            assert!(!effects
                .iter()
                .any(|effect| matches!(effect, Effect::Shares { .. })));
        } else {
            assert!(effects.iter().any(|effect| matches!(effect, Effect::Shares { names, .. } if names == &["docs", "media"])));
        }
    }
}

#[test]
fn bfx_net_quiet_info_cannot_reopen_previous_mount_path() {
    let mut mounts = Mounts::default();
    for (id, path) in [(1, "local path: /fixture/share\n"), (2, "")] {
        let mut request = request(id);
        request.uri = "sftp://fixture/share".into();
        let effects = mounts.open_share(request, 0);
        let effects = run(effects, &mut mounts, "", 1);
        let effects = run(effects, &mut mounts, path, 2);
        if path.is_empty() {
            assert!(effects.iter().any(|effect| matches!(
                effect,
                Effect::Completed {
                    id: 2,
                    error: Some("network.no_folder"),
                    ..
                }
            )));
            assert!(!effects
                .iter()
                .any(|effect| matches!(effect, Effect::EnsureBridge { .. } | Effect::Open { .. })));
        } else {
            assert!(effects.iter().any(|effect| matches!(effect, Effect::EnsureBridge { path, .. } if path == "/fixture/share")));
        }
    }
}
