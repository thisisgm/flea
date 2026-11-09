// In-process GPUI tests on the real Picker entity: one answer, the reply on disk, both children, then exit.
use super::process::{Line, PickerChild};
use super::*;
use fixture::{fixture, locale, Fake, Fixture, Root};
use flea::ui_logic_picker_portal::{self as logic, CLOSE_LINE};
use gpui::{TestAppContext, VisualTestContext};
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "browse_tests.rs"]
mod browse_tests;
#[path = "codeicons_tests.rs"]
mod codeicons_tests;
#[path = "find_tests.rs"]
mod find_tests;
#[path = "fixture.rs"]
pub(super) mod fixture;
#[path = "geometry_tests.rs"]
mod geometry_tests;
#[path = "ipc_tests.rs"]
mod ipc_tests;
#[path = "save_tests.rs"]
mod save_tests;

fn leftovers(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into())
        .collect();
    names.sort();
    names
}

#[gpui::test]
fn cancel_writes_the_refusal_then_quits_both_children_then_exits_once(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.quits() == (1, 1));
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
    assert_eq!(
        f.checks.sent(),
        [CLOSE_LINE],
        "the checks child is told the request is over"
    );
    assert_eq!(
        leftovers(&f.root.0),
        [".flea-test-sandbox", "reply.json"],
        "no temporary file is left"
    );
    f.checks.say(Line::Stopped);
    f.cx.run_until_parked();
    assert_eq!(f.exits.get(), 0, "a stalled listing child holds the exit");
    f.listing.say(Line::Stopped);
    f.cx.run_until_parked();
    assert_eq!(f.exits.get(), 1);
    f.update(|picker, cx| picker.cancel(cx));
    f.cx.run_until_parked();
    assert_eq!(
        (f.exits.get(), f.quits(), f.checks.sent().len()),
        (1, (1, 1), 1),
        "one answer, one quit, one exit"
    );
}

#[gpui::test]
fn escape_refuses_from_the_focused_window(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("escape");
    f.settle(|f| f.written().is_some());
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
}

#[gpui::test]
fn a_window_close_refuses_and_waits_for_the_children(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    assert!(
        !f.update(|picker, cx| picker.window_closed(cx)),
        "the close waits for the reply and both children"
    );
    f.settle(|f| f.quits() == (1, 1));
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
    f.checks.say(Line::Stopped);
    f.listing.say(Line::Stopped);
    f.cx.run_until_parked();
    assert_eq!(f.exits.get(), 1);
    assert!(
        f.update(|picker, cx| picker.window_closed(cx)),
        "an exited window may close"
    );
}

#[gpui::test]
fn the_cursor_file_is_checked_then_answered_with_its_uri_and_filter(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"filters":[{"label":"Text","globs":["*.txt"]}]}"#,
        None,
    );
    f.update(|picker, cx| {
        picker.listing.location = "/d".into();
        picker.listing.state = "ready";
        picker.listing.cursor_file = Some("a b.txt".into());
        assert!(picker.can_accept());
        picker.accept(cx);
        assert!(!picker.can_accept(), "a submission refuses a second press");
        picker.accept(cx);
    });
    assert_eq!(
        f.checks.sent(),
        [
            r#"{"op":"mark","path":"/d/a b.txt","directory":false,"multiple":false,"c":"picker","id":1}"#
        ]
    );
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"mark","ok":true,"marks":[{"path":"/d/a b.txt","bytes":3}],"removed":0}"#.into()));
    f.settle(|f| f.written().is_some());
    assert_eq!(
        f.written().as_deref(),
        Some(r#"{"response":0,"uris":["file:///d/a%20b.txt"],"filter":0}"#)
    );
}

#[gpui::test]
fn a_refused_mark_says_why_and_answers_nothing(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    f.update(|picker, cx| {
        picker.listing.cursor_file = Some("gone".into());
        picker.accept(cx);
    });
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"mark","ok":false,"error":"Selected item moved or disappeared."}"#.into()));
    f.settle(|f| f.read(|picker| picker.message.is_some()));
    let message = f.read(|picker| picker.message.clone().unwrap());
    assert_eq!(
        (message.text.as_ref(), message.error),
        ("Selected item moved or disappeared.", true)
    );
    assert!(
        f.read(|picker| !picker.answered() && picker.can_accept()),
        "the window stays answerable"
    );
    // A stale or unknown answer changes nothing.
    f.checks.say(Line::Reply(
        r#"{"t":"picker","id":9,"op":"mark","ok":true,"marks":[]}"#.into(),
    ));
    f.checks.say(Line::Reply("not json".into()));
    f.cx.run_until_parked();
    assert!(f.written().is_none());
}

#[gpui::test]
fn a_mark_that_removed_items_waits_for_the_selection_owner(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"multiple":true}"#, None);
    f.update(|picker, cx| {
        picker.marks = logic::Mark::reviewed(&[], &[("/d/a".into(), 1), ("/d/b".into(), 2)]);
        picker.accept(cx);
    });
    assert_eq!(
        f.checks.sent(),
        [r#"{"op":"validate","c":"picker","id":1}"#]
    );
    f.checks.say(Line::Reply(r#"{"t":"picker","id":1,"op":"validate","ok":true,"marks":[{"path":"/d/a","bytes":1}],"removed":1}"#.into()));
    f.settle(|f| f.read(|picker| picker.selection_notice.is_some()));
    assert!(
        f.written().is_none(),
        "a changed selection is never sent unseen"
    );
    assert_eq!(f.read(|picker| picker.marks.len()), 1);
}

#[gpui::test]
fn a_lost_backend_refuses_answers_and_says_so_once(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    f.update(|picker, _| picker.listing.cursor_file = Some("a".into()));
    f.checks.say(Line::Failed("exited".into()));
    f.settle(|f| f.read(Picker::backend_unavailable));
    let (message, hints, accept) = f.read(|picker| {
        (
            picker.message.clone().unwrap(),
            picker.hints(),
            picker.can_accept(),
        )
    });
    assert_eq!(
        message.text.as_ref(),
        "Flea stopped responding. Cancel and reopen this request."
    );
    assert!(message.error && !accept);
    assert_eq!(hints.as_ref(), "esc cancel");
    f.update(|picker, cx| picker.accept(cx));
    assert!(
        f.checks.sent().is_empty(),
        "nothing is asked of a lost backend"
    );
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.quits() == (1, 1));
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
    assert!(
        f.checks.sent().is_empty(),
        "a lost backend is not told to close"
    );
}

#[gpui::test]
fn a_reply_that_cannot_be_written_still_quits_the_children(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    std::fs::create_dir(f.root.reply()).unwrap();
    std::fs::write(f.root.reply().join("held"), "").unwrap();
    f.update(|picker, cx| picker.cancel(cx));
    f.settle(|f| f.quits() == (1, 1));
    assert!(f.root.reply().is_dir(), "a failed write replaces nothing");
    assert_eq!(leftovers(&f.root.0), [".flea-test-sandbox", "reply.json"]);
}

#[gpui::test]
fn a_notice_fades_and_an_error_stays(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"directory":true}"#, None);
    f.update(|picker, cx| {
        picker.listing.location = logic::RECENT.into();
        picker.accept(cx);
    });
    assert_eq!(
        f.read(|picker| picker.message.clone().unwrap().text.to_string()),
        "Press Space to select a folder first"
    );
    f.cx.executor()
        .advance_clock(Duration::from_millis(logic::MESSAGE_LIFE_MS + 1));
    f.cx.run_until_parked();
    assert!(f.read(|picker| picker.message.is_none()));
    f.update(|picker, cx| picker.say("kept", true, cx));
    f.cx.executor()
        .advance_clock(Duration::from_millis(logic::MESSAGE_LIFE_MS * 2));
    f.cx.run_until_parked();
    assert!(f.read(|picker| picker.message.is_some()));
}

#[gpui::test]
fn the_labels_follow_the_request_and_the_marks(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"accept":"Se%2nd","multiple":true,"app":"org.example.a%1b"}"#,
        None,
    );
    f.update(|picker, _| {
        picker.marks =
            logic::Mark::reviewed(&[], &[("/a".into(), 1), ("/b".into(), 1), ("/c".into(), 1)])
    });
    let (accept, requester, title) = f.read(|picker| {
        (
            picker.answer_label(),
            picker.labels.requester.clone(),
            picker.labels.title.clone(),
        )
    });
    assert_eq!(
        accept.as_ref(),
        "Se%2nd 3",
        "the caller's label is never re-scanned"
    );
    assert_eq!(requester.unwrap().as_ref(), "Requested by org.example.a%1b");
    assert_eq!(title.as_ref(), "Choose file");
    f.update(|picker, _| picker.marks.truncate(1));
    assert_eq!(f.read(Picker::answer_label).as_ref(), "Se%2nd");
}

#[gpui::test]
fn real_stalled_children_are_reaped_after_a_cancel(cx: &mut TestAppContext) {
    const CHILD: &str = "FLEA_PICKER_REAP_TEST_CHILD";
    const TEST: &str = "g2::picker_portal::tests::real_stalled_children_are_reaped_after_a_cancel";
    if std::env::var(CHILD).as_deref() != Ok(TEST) {
        struct ReplyRoot(std::path::PathBuf);
        impl Drop for ReplyRoot {
            fn drop(&mut self) {
                let path = &self.0;
                if path.is_absolute()
                    && !path.as_os_str().is_empty()
                    && path.starts_with("/dev/shm")
                    && path.join(".flea-test-sandbox").is_file()
                {
                    std::fs::remove_dir_all(path).expect("remove the private reply root");
                }
            }
        }
        // Reaping is measured on tmpfs, so the guard never includes a shared disk's reply fsync.
        let made = std::process::Command::new("mktemp")
            .args(["-d", "/dev/shm/flea-picker-reap.XXXXXX"])
            .output()
            .expect("a private tmpfs reply root");
        assert!(made.status.success(), "tmpfs reply root: {:?}", made.status);
        let path = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with("/dev/shm"));
        std::fs::write(path.join(".flea-test-sandbox"), "").unwrap();
        let reply_root = ReplyRoot(path);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, TEST)
            .env("TMPDIR", &reply_root.0)
            .status()
            .expect("the isolated picker reap test");
        assert!(status.success(), "isolated picker reap test: {status}");
        return;
    }

    struct TimedChild {
        child: process::PickerProcess,
        began: std::rc::Rc<std::cell::Cell<Option<Instant>>>,
    }
    impl PickerChild for TimedChild {
        fn send(&self, line: &str) -> bool {
            self.child.send(line)
        }
        fn quit(&self) {
            if self.began.get().is_none() {
                self.began.set(Some(Instant::now()));
            }
            self.child.quit();
        }
        fn lines(&self) -> async_channel::Receiver<Line> {
            self.child.lines()
        }
    }
    let began = std::rc::Rc::new(std::cell::Cell::new(None));
    let stalled = || {
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(r#"printf '{"t":"blocked"}\n'; exec sleep 30"#);
        Box::new(TimedChild {
            child: process::PickerProcess::start(command),
            began: began.clone(),
        }) as Box<dyn PickerChild>
    };
    let mut f = fixture(
        cx,
        "{}",
        Some(Children {
            checks: stalled(),
            listing: stalled(),
        }),
    );
    f.update(|picker, cx| picker.cancel(cx));
    // Record the first quit before synchronous teardown; missing quits must still fail within the guard.
    f.settle(|f| began.get().is_some() || f.exits.get() != 0);
    assert!(
        began.get().is_some(),
        "cancellation exited without quitting either child"
    );
    f.settle(|f| f.exits.get() == 1);
    assert!(
        began.get().unwrap().elapsed() < Duration::from_secs(2),
        "the stalled children held the exit {:?}",
        began.get().unwrap().elapsed()
    );
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
}

#[gpui::test]
fn the_window_opens_with_the_callers_title(cx: &mut TestAppContext) {
    let root = Root::new();
    let launch = launch::PickLaunch::resolve(
        Some(r#"{"title":"Attach files"}"#),
        &root.reply().to_string_lossy(),
        Some("/home/t"),
    )
    .unwrap();
    let children = Children {
        checks: Box::new(Fake::new()),
        listing: Box::new(Fake::new()),
    };
    let window = cx
        .update(|cx| launch::open(launch, locale(), children, cx))
        .expect("the picker window opens");
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(visual.window_title().as_deref(), Some("Attach files"));
    let location = window
        .update(cx, |picker, _, _| picker.listing.location.clone())
        .unwrap();
    assert_eq!(
        location, "/home/t",
        "a request without a folder opens on home"
    );
}

#[test]
fn a_launch_needs_a_request_and_a_reply_path() {
    assert_eq!(
        launch::PickLaunch::resolve(None, "/r", None),
        Err("picker.portal.no_request")
    );
    assert_eq!(
        launch::PickLaunch::resolve(Some(""), "/r", None),
        Err("picker.portal.no_request")
    );
    assert_eq!(
        launch::PickLaunch::resolve(Some("{}"), "", None),
        Err("picker.portal.no_reply")
    );
    let launch = launch::PickLaunch::resolve(Some("not json"), "/r", Some("")).unwrap();
    assert_eq!(
        (launch.req.mode, launch.home.as_str()),
        (logic::Mode::Open, "/")
    );
}

// Review g2-pk1-r1-1 and r1-3: the reviewer's repro tests as given.
#[gpui::test]
fn rev_pk1_r1_a_backend_that_cannot_start_keeps_the_window_answerable(cx: &mut TestAppContext) {
    let missing = || {
        Box::new(process::PickerProcess::start(std::process::Command::new(
            "/nonexistent/flea-rev-pk1",
        ))) as Box<dyn PickerChild>
    };
    let mut f = fixture(
        cx,
        "{}",
        Some(Children {
            checks: missing(),
            listing: missing(),
        }),
    );
    f.settle(|f| f.read(Picker::backend_unavailable));
    f.cx.run_until_parked();
    assert_eq!(
        (f.exits.get(), f.written()),
        (0, None),
        "the window left with no reply before anyone answered"
    );
}

#[gpui::test]
fn rev_pk1_r1_a_refused_answer_button_does_nothing_when_clicked(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"directory":true}"#, None);
    f.update(|picker, cx| {
        picker.listing.location = "/d".into();
        cx.notify();
    });
    f.cx.run_until_parked();
    assert!(
        !f.read(Picker::can_accept),
        "Choose folder is drawn refused while the listing loads"
    );
    let size = f.cx.update(|window, _| window.viewport_size());
    let at = gpui::point(size.width - gpui::px(30.), size.height - gpui::px(57.));
    f.cx.simulate_click(at, gpui::Modifiers::none());
    f.cx.run_until_parked();
    let refused = f.checks.sent();
    assert!(
        refused.is_empty(),
        "a press on the refused button asked the backend {refused:?}"
    );
    f.update(|picker, cx| {
        picker.listing.state = "ready";
        cx.notify();
    });
    f.cx.run_until_parked();
    f.cx.simulate_click(at, gpui::Modifiers::none());
    f.cx.run_until_parked();
    let live = f.checks.sent();
    assert!(
        !live.is_empty(),
        "control: the click must land on the answer button"
    );
}
