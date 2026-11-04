//! Every exit of a bridge wait, timeout, served or dropped, ends its Starting line once.
use super::mount::Effect;
use super::service::{Service, ServiceEvent};
use crate::g2::pl1::process::{Event, Handle, Job, Outcome, Runner};
use gpui::{AppContext, Entity};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// Every `test` check answers at once whether the bridge serves the folder yet.
#[derive(Default)]
struct Bridge {
    served: Cell<bool>,
}
impl Runner for Bridge {
    fn start(&self, job: Job) -> Handle {
        assert_eq!(job.argv[0], "test");
        let (handle, send) = Handle::detached();
        let code = if self.served.get() { 0 } else { 1 };
        send.try_send(Event::Exited(Outcome::code(code))).unwrap();
        handle
    }
}

/// The service's effects in order, one line each.
type Log = Rc<RefCell<Vec<String>>>;

/// A service waiting on an unserved bridge whose Starting line has just shown, and its event log.
fn started(
    cx: &mut gpui::TestAppContext,
) -> (
    Rc<Bridge>,
    Entity<Service>,
    Log,
    &mut gpui::VisualTestContext,
) {
    let runner = Rc::new(Bridge::default());
    let service = cx.new(|_| {
        Service::new(
            runner.clone(),
            "/fixture-run".into(),
            "/fixture-missing-fuse".into(),
        )
    });
    let seen: Log = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.subscribe(&service, move |_, event: &ServiceEvent, _| {
            log.borrow_mut().push(match &event.0 {
                Effect::Starting { note, .. } => format!("starting {}", note.key),
                Effect::Settled { origin } => format!("settled {origin}"),
                Effect::Say { note, .. } => format!("say {}", note.key),
                other => format!("{other:?}"),
            })
        })
        .detach();
        service.update(cx, |service, cx| {
            let now = service.now(cx);
            let ensure = Effect::EnsureBridge {
                generation: 0,
                path: "/fixture-run/gvfs/smb-share:server=fixture,share=data".into(),
                label: "Bridge fail".into(),
                origin: 7,
            };
            service.apply(vec![ensure], now, cx)
        });
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_pl2::bridge::STARTING_MS + 1,
    ));
    cx.run_until_parked();
    assert_eq!(&*seen.borrow(), &["starting network.bridge_starting"]);
    (runner, service, seen, cx)
}

/// Runs the clock past the whole bridge wait, so a late timeout or second settle would land.
fn outlast_the_wait(cx: &mut gpui::VisualTestContext) {
    for _ in 0..(flea::ui_logic_pl2::bridge::ENSURE_MS / 1_000) {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn uish_misc_unserved_bridge_settles_its_starting_line_then_says_the_timeout(
    cx: &mut gpui::TestAppContext,
) {
    let (_, _service, seen, cx) = started(cx);
    outlast_the_wait(cx);
    assert_eq!(
        &*seen.borrow(),
        &[
            "starting network.bridge_starting",
            "settled 7",
            "say network.bridge_failed_timeout"
        ]
    );
}

#[gpui::test]
fn uish_misc_served_bridge_settles_its_starting_line(cx: &mut gpui::TestAppContext) {
    let (runner, _service, seen, cx) = started(cx);
    runner.served.set(true);
    cx.executor().advance_clock(Duration::from_millis(
        flea::ui_logic_pl2::bridge::POLL_MS + 1,
    ));
    cx.run_until_parked();
    outlast_the_wait(cx);
    assert_eq!(
        &*seen.borrow(),
        &["starting network.bridge_starting", "settled 7"]
    );
}

#[gpui::test]
fn uish_misc_dropped_bridge_settles_its_starting_line(cx: &mut gpui::TestAppContext) {
    let (_, service, seen, cx) = started(cx);
    cx.update(|_, cx| {
        service.update(cx, |service, cx| {
            let now = service.now(cx);
            service.apply(vec![Effect::Kill(0)], now, cx)
        })
    });
    cx.run_until_parked();
    outlast_the_wait(cx);
    assert_eq!(
        &*seen.borrow(),
        &["starting network.bridge_starting", "settled 7"]
    );
}
