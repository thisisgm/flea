//! Real entity handoff proves cancellation, late replies and the timer without mounting anything.
use super::{
    mount::{Effect, Request},
    service::{Service, ServiceEvent},
};
use crate::g2::pl1::{
    process::{Event, Handle, Job, KillProbe, Outcome, Runner},
    secret::Secret,
};
use gpui::AppContext;
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Default)]
struct Recorder {
    jobs: RefCell<Vec<Job>>,
    answers: RefCell<Vec<async_channel::Sender<Event>>>,
    probes: RefCell<Vec<KillProbe>>,
}
impl Runner for Recorder {
    fn start(&self, job: Job) -> Handle {
        let (handle, send, probe) = Handle::probed();
        self.jobs.borrow_mut().push(job);
        self.answers.borrow_mut().push(send);
        self.probes.borrow_mut().push(probe);
        handle
    }
}
fn request(id: u64) -> Request {
    Request {
        id,
        uri: "sftp://user@host/folder".into(),
        label: "Saved name".into(),
        origin: 23,
        mounted: false,
        authenticated: false,
        password: Secret::new(),
    }
}

#[gpui::test]
fn pl2_service_cancellation_late_reply_timer_and_shutdown(cx: &mut gpui::TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let service = cx.new(|_| {
        Service::new(
            recorder.clone(),
            "/fixture/runtime".into(),
            "/usr/bin/true".into(),
        )
    });
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = opened.clone();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.subscribe(&service, move |_, event: &ServiceEvent, _| {
            if let Effect::Open { path, origin, .. } = &event.0 {
                seen.borrow_mut().push((path.clone(), *origin));
            }
        })
        .detach()
    });
    assert!(
        recorder.jobs.borrow().is_empty(),
        "construction starts nothing"
    );
    cx.update(|_, cx| service.update(cx, |v, cx| v.open_share(request(1), 0, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        service.update(cx, |v, cx| {
            v.cancel(1, 0, cx);
            v.open_share(request(2), 0, cx);
        })
    });
    assert!(recorder.probes.borrow()[0].killed());
    recorder.answers.borrow()[0]
        .try_send(Event::Exited(Outcome::code(0)))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        recorder.jobs.borrow().len(),
        2,
        "late cancelled mount cannot start info"
    );
    recorder.answers.borrow()[1]
        .try_send(Event::Exited(Outcome::code(0)))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(recorder.jobs.borrow()[2].argv[1], "info");
    recorder.answers.borrow()[2]
        .try_send(Event::Exited(Outcome {
            code: 0,
            stdout: "local path: /fixture/served\n".into(),
            stderr: String::new(),
        }))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(&*opened.borrow(), &[("/fixture/served".into(), 23)]);
    cx.update(|_, cx| {
        service.update(cx, |v, cx| {
            let now = v.now(cx);
            v.open_share(request(3), now, cx);
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(15_001));
    cx.run_until_parked();
    assert!(
        recorder.probes.borrow()[3].killed(),
        "deadline kills the hung process"
    );
    cx.update(|_, cx| assert!(!service.read(cx).mounts.busy()));
    let shutdown = cx.update(|_, cx| service.update(cx, |v, cx| v.shutdown(cx)));
    for answer in recorder.answers.borrow().iter() {
        answer.close();
    }
    cx.run_until_parked();
    assert!(
        shutdown.is_ready(),
        "shutdown waits for every retired reader"
    );
}

#[gpui::test]
fn pl2_retiring_an_older_bridge_cannot_kill_another_panes_current_mount(
    cx: &mut gpui::TestAppContext,
) {
    let recorder = Rc::new(Recorder::default());
    let service = cx.new(|_| {
        Service::new(
            recorder.clone(),
            "/fixture/runtime".into(),
            "/usr/bin/true".into(),
        )
    });
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = opened.clone();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.subscribe(&service, move |_, event: &ServiceEvent, _| {
            if let Effect::Open { origin, .. } = &event.0 {
                seen.borrow_mut().push(*origin);
            }
        })
        .detach()
    });
    let answer = |job: usize, stdout: &str| {
        recorder.answers.borrow()[job]
            .try_send(Event::Exited(Outcome {
                code: 0,
                stdout: stdout.into(),
                stderr: String::new(),
            }))
            .unwrap()
    };
    cx.update(|_, cx| service.update(cx, |v, cx| v.open_share(request(1), 0, cx)));
    answer(0, "");
    cx.run_until_parked();
    answer(1, "local path: /fixture/runtime/gvfs/first\n");
    cx.run_until_parked();
    assert_eq!(recorder.jobs.borrow()[2].argv[0], "test");
    cx.update(|_, cx| {
        service.update(cx, |v, cx| {
            let mut second = request(2);
            second.origin = 99;
            second.uri = "sftp://other/second".into();
            v.open_share(second, 0, cx);
            v.cancel_origin(23, cx);
        })
    });
    assert!(recorder.probes.borrow()[2].killed());
    assert!(!recorder.probes.borrow()[3].killed());
    answer(2, "");
    answer(3, "");
    cx.run_until_parked();
    answer(4, "local path: /fixture/second\n");
    cx.run_until_parked();
    assert_eq!(&*opened.borrow(), &[99]);
    let shutdown = cx.update(|_, cx| service.update(cx, |v, cx| v.shutdown(cx)));
    for answer in recorder.answers.borrow().iter() {
        answer.close();
    }
    cx.run_until_parked();
    assert!(shutdown.is_ready());
}

#[gpui::test]
fn pl2_provider_probe_retires_cancelled_results_and_its_hung_helper(cx: &mut gpui::TestAppContext) {
    let recorder = Rc::new(Recorder::default());
    let service = cx.new(|_| Service::new(recorder.clone(), String::new(), String::new()));
    let found = Rc::new(RefCell::new(Vec::new()));
    let seen = found.clone();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.subscribe(
            &service,
            move |_, event: &super::provider::ProviderStatus, _| {
                seen.borrow_mut().push((event.id, event.installed))
            },
        )
        .detach()
    });
    assert!(recorder.jobs.borrow().is_empty());
    cx.update(|_, cx| {
        service.update(cx, |v, cx| {
            v.dialog_effect(super::dialog::Effect::CheckDropbox(1), cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(recorder.jobs.borrow()[0].argv, ["which", "dropbox-cli"]);
    cx.update(|_, cx| {
        service.update(cx, |v, cx| {
            v.dialog_effect(super::dialog::Effect::CancelDropbox(1), cx);
            v.dialog_effect(super::dialog::Effect::CheckDropbox(2), cx);
        })
    });
    recorder.answers.borrow()[0]
        .try_send(Event::Exited(Outcome::code(0)))
        .unwrap();
    recorder.answers.borrow()[1]
        .try_send(Event::Exited(Outcome::code(1)))
        .unwrap();
    cx.run_until_parked();
    assert!(recorder.probes.borrow()[0].killed());
    assert_eq!(&*found.borrow(), &[(2, false)]);
    cx.update(|_, cx| {
        service.update(cx, |v, cx| {
            v.dialog_effect(super::dialog::Effect::CheckDropbox(3), cx);
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(3_001));
    cx.run_until_parked();
    assert!(recorder.probes.borrow()[2].killed());
    assert_eq!(&*found.borrow(), &[(2, false), (3, false)]);
    let shutdown = cx.update(|_, cx| service.update(cx, |v, cx| v.shutdown(cx)));
    for answer in recorder.answers.borrow().iter() {
        answer.close();
    }
    cx.run_until_parked();
    assert!(shutdown.is_ready());
}
