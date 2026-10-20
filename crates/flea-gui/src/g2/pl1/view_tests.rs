//! The real UnlockView's name poller against a recording runner, through its public calls only.
use super::devices::LIST_TIMEOUT_MS;
use super::process::{Event, Handle, Job, KillProbe, Outcome, Runner};
use super::view::UnlockView;
use async_channel::Sender;
use flea::i18n::{I18n, Paths};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const ROW: &str = r#"{"blockdevices":[{"name":"cap-loop9","path":"/dev/loop9","label":null,"mountpoints":[null],"rm":true,"tran":null,"size":33554432,"type":"loop","model":null,"fstype":"crypto_LUKS","fsver":"2"}]}"#;

#[derive(Default)]
struct Recorder {
    argv: RefCell<Vec<Vec<String>>>,
    answers: RefCell<Vec<Sender<Event>>>,
    probes: RefCell<Vec<KillProbe>>,
}

impl Runner for Recorder {
    fn start(&self, job: Job) -> Handle {
        let (handle, send, probe) = Handle::probed();
        self.argv.borrow_mut().push(job.argv.clone());
        self.answers.borrow_mut().push(send);
        self.probes.borrow_mut().push(probe);
        handle
    }
}

impl Recorder {
    fn lists(&self) -> usize {
        self.argv
            .borrow()
            .iter()
            .filter(|a| a.first().is_some_and(|p| p == "lsblk"))
            .count()
    }

    fn answer(&self, job: usize, outcome: Outcome) {
        self.answers.borrow()[job]
            .try_send(Event::Exited(outcome))
            .unwrap();
    }
}

fn listed(body: &str) -> Outcome {
    Outcome {
        code: 0,
        stdout: body.into(),
        stderr: String::new(),
    }
}

fn view(cx: &mut TestAppContext) -> (Rc<Recorder>, Entity<UnlockView>, &mut VisualTestContext) {
    let recorder = Rc::new(Recorder::default());
    let runner: Rc<dyn Runner> = recorder.clone();
    let catalog = Arc::new(I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: Vec::new(),
    }));
    let cx = cx.add_empty_window();
    let view = cx.update(|_, cx| cx.new(|cx| UnlockView::new(catalog, runner, cx)));
    (recorder, view, cx)
}

#[gpui::test]
fn a_killed_name_listings_late_exit_never_answers_the_reopened_card(cx: &mut TestAppContext) {
    let (recorder, view, cx) = view(cx);
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_for_device("/dev/loop9", window, cx)));
    assert_eq!(recorder.lists(), 1, "flea --unlock with no rail lists once");
    cx.update(|_, cx| view.update(cx, |v, cx| v.close(cx)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_for_device("/dev/loop9", window, cx)));
    assert_eq!(recorder.lists(), 2, "the reopened card lists again");
    recorder.answer(0, Outcome::code(-1));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let host = &view.read(cx).host;
        assert_eq!(
            host.unnamed_device, "/dev/loop9",
            "the killed listing's exit is not the reopened card's answer"
        );
        assert!(
            host.name_polling(),
            "the reopened card still waits on its own listing"
        );
    });
    recorder.answer(1, listed(ROW));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).host.card.as_ref().unwrap().label,
            "cap-loop9",
            "its own listing names it"
        )
    });
}

#[gpui::test]
fn a_timed_out_name_listing_lists_again_once_its_killed_lsblk_exits(cx: &mut TestAppContext) {
    let (recorder, view, cx) = view(cx);
    cx.update(|window, cx| view.update(cx, |v, cx| v.open_for_device("/dev/loop9", window, cx)));
    assert_eq!(recorder.lists(), 1);
    assert!(
        !recorder.probes.borrow()[0].killed(),
        "the hung lsblk runs until the bound"
    );
    cx.update(|_, cx| view.update(cx, |v, cx| v.rail_changed(None, cx)));
    assert!(
        !recorder.probes.borrow()[0].killed(),
        "a host turn before the bound does not kill the hung lsblk"
    );
    // The view's clock is wall time, so the bound passes for real; any host turn after it runs the poller's tick.
    std::thread::sleep(Duration::from_millis(LIST_TIMEOUT_MS + 200));
    cx.update(|_, cx| view.update(cx, |v, cx| v.rail_changed(None, cx)));
    cx.update(|_, cx| {
        assert!(
            view.read(cx).host.name_polling(),
            "the card still waits on its name"
        )
    });
    assert_eq!(
        recorder.lists(),
        1,
        "the bound does not list again until the hung lsblk exits"
    );
    assert!(
        recorder.probes.borrow()[0].killed(),
        "the bound kills the hung lsblk, not only its wait"
    );
    recorder.answer(0, Outcome::code(-1));
    cx.run_until_parked();
    assert_eq!(recorder.lists(), 2, "the killed lsblk's exit lists again at once, as host_tests timed_out_listing_lists_again_at_the_bound pins for the host");
    recorder.answer(1, listed(ROW));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).host.card.as_ref().unwrap().label,
            "cap-loop9",
            "the retried listing names the card"
        )
    });
}
