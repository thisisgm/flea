//! U13 keeps destructive confirmation defaults and asynchronous commands on their producer's channel.
use crate::{
    backend::{Backend, Event},
    state::Shell,
    window::{pane::PaneHost, root::Root},
};
use flea::ui_logic_file_ops::{
    confirm::Source,
    trash::{Review, Scope},
};
use gpui::{AppContext, TestAppContext, Window, WindowHandle};

fn update<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    f: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Root> {
    super::atomic_tests::open_with(cx, backend)
}

fn key(cx: &mut TestAppContext, handle: WindowHandle<Root>, key: &str) {
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes(key);
    visual.run_until_parked();
}

fn review() -> Review {
    Review {
        count: Some(1),
        token: "review-1".into(),
        ..Default::default()
    }
}

#[gpui::test]
fn u13_confirmation_enter_cancels_and_a_timer_uses_the_real_backend(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open(cx, backend);
    update(cx, handle, |shell, window, cx| {
        shell.confirm_operation(0, review(), Scope::Items, Source::Menu(7), window, cx)
    });
    key(cx, handle, "enter");
    let line = received().expect("Cancel closes the actual menu snapshot");
    assert!(line.contains("\"op\":\"close\"") && line.contains("\"id\":7"));
    assert!(received().is_none(), "Cancel sends no deletion");
    update(cx, handle, |shell, window, cx| {
        shell.confirm_operation(0, review(), Scope::Items, Source::Menu(8), window, cx)
    });
    let visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    visual
        .executor()
        .advance_clock(std::time::Duration::from_millis(2500));
    visual.run_until_parked();
    let line = received().expect("the confirmation timer reaches its originating backend");
    assert!(line.contains("\"op\":\"checkDelete\"") && line.contains("\"id\":8"));
    update(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Event::Failed(crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            }),
            window,
            cx,
        )
    });
    update(cx, handle, |shell, _, cx| {
        let view = shell.operation_view(0).unwrap();
        assert!(
            !view.read(cx).model.open(),
            "a lost backend invalidates its confirmation"
        );
        assert!(view.read(cx).model.confirm.is_none());
    });
}

#[gpui::test]
fn u13_replies_and_actions_keep_the_origin_after_pane_focus_moves(cx: &mut TestAppContext) {
    let (first, mut first_lines) = Backend::test_operation_channel();
    let (second, mut second_lines) = Backend::test_operation_channel();
    let handle = open(cx, first);
    update(cx, handle, |shell, window, cx| {
        let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
        host.backend = Some(second);
        host.pane
            .receive(flea::protocol::Reply::Listed {
                path: "/other".into(),
                total: 1,
            })
            .unwrap();
        host.pane
            .receive(flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![flea::protocol::RowFacts::default()],
                listing: 3,
            })
            .unwrap();
        // The ready second pane has settled both independent filesystem consumers.
        host.sizes = crate::list::sizes::State::settled_fixture(
            std::rc::Rc::clone(&host.pane.held),
            host.path.clone(),
            "",
        );
        let facts = crate::backend::status::Filesystem {
            class: String::new(),
            tagable: None,
            path: host.path.clone(),
            name: "testfs".into(),
            free: 1_073_741_824,
        };
        shell.surfaces[1].filesystem =
            crate::window::status::Filesystem::settled_fixture(facts, host.pane.held.listing);
        shell.panes[1] = Some(host);
        shell.lifecycle.attach(1);
        assert!(shell.ask_operation(0, &super::tests::request(), &[], true, window, cx));
        shell.focus_side = 1;
        shell.receive(1, Event::Operations(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"foreign","d":false,"i":""}],"mergeable":false}"#.into()), window, cx);
        assert!(
            !shell.operation_view(0).unwrap().read(cx).model.open(),
            "the other pane's equal id is not our reply"
        );
        shell.receive(0, Event::Operations(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"ours","d":false,"i":""}],"mergeable":false}"#.into()), window, cx);
    });
    assert!(first_lines().unwrap().contains("\"c\":\"collisions\""));
    key(cx, handle, "enter");
    assert!(
        first_lines().unwrap().contains("\"c\":\"transfer\""),
        "the answer returns to pane zero"
    );
    assert!(second_lines().is_none());
    key(cx, handle, "z");
    assert_eq!(
        second_lines().as_deref(),
        Some(r#"{"c":"undo"}"#),
        "global Undo uses the focused backend and shared journal"
    );
}

#[gpui::test]
fn u13_stale_refusal_releases_the_question_for_the_next_ask(cx: &mut TestAppContext) {
    let mut command = std::process::Command::new("python3");
    command.args(["-u", "-c", r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    if request['c']=='quit': break
    if request['c']=='collisions':
        print('{"t":"error","where":"stale","path":"collisions","msg":"Listing changed"}',flush=True)
"#]);
    let backend = Backend::start_command(command, "/files".into(), None, 320).unwrap();
    let events = backend.replies.clone();
    let handle = open(cx, backend);
    update(cx, handle, |shell, window, cx| {
        assert!(shell.ask_operation(0, &super::tests::request(), &[], false, window, cx));
    });
    let event = super::u13_join_tests::next(&events);
    update(cx, handle, |shell, window, cx| {
        shell.receive(0, event, window, cx);
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            "Listing changed"
        );
        assert!(shell.ask_operation(0, &super::tests::request(), &[], false, window, cx));
        assert_eq!(
            shell
                .operation_view(0)
                .unwrap()
                .read(cx)
                .model
                .collide
                .ask_id,
            2
        );
    });
}

#[gpui::test]
fn u13_a_second_panes_waiting_card_is_promoted_after_cancel(cx: &mut TestAppContext) {
    let (first, _first_lines) = Backend::test_operation_channel();
    let (second, _second_lines) = Backend::test_operation_channel();
    let handle = open(cx, first);
    update(cx, handle, |shell, window, cx| {
        let mut host = PaneHost::new("/other".into(), None, 600., &shell.locale);
        host.backend = Some(second);
        shell.panes[1] = Some(host);
        for side in 0..2 {
            assert!(shell.ask_operation(side, &super::tests::request(), &[], false, window, cx));
            shell.receive(side, Event::Operations(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes","d":false,"i":""}],"mergeable":false}"#.into()), window, cx);
        }
        assert_eq!(shell.overlay, shell.operation_view(0));
    });
    key(cx, handle, "escape");
    update(cx, handle, |shell, _, cx| {
        assert_eq!(shell.overlay, shell.operation_view(1));
        assert!(shell.overlay.as_ref().unwrap().read(cx).model.open());
    });
    key(cx, handle, "escape");
    update(cx, handle, |shell, _, cx| {
        assert!(!shell.operation_view(1).unwrap().read(cx).model.open())
    });
}

#[gpui::test]
fn u13_backend_loss_stops_activity_without_claiming_success(cx: &mut TestAppContext) {
    let (backend, _lines) = Backend::test_operation_channel();
    let handle = open(cx, backend);
    update(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Event::Operations(r#"{"t":"transferstarted","id":9,"n":2,"moving":false}"#.into()),
            window,
            cx,
        );
        shell.receive(
            0,
            Event::Operations(
                r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copying","state":"running"}}"#
                    .into(),
            ),
            window,
            cx,
        );
        shell.receive(
            0,
            Event::Failed(crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            }),
            window,
            cx,
        );
        let view = shell.operation_view(0).unwrap();
        let model = &view.read(cx).model;
        assert!(!model.wire.transfer.running);
        assert!(!model.emptying.running);
        assert_eq!(model.due(), None);
        assert!(model.sticky.is_empty());
        assert_eq!(model.jobs.unfinished(), 0);
        assert_eq!(model.jobs.rows[0].state, "failed");
        assert!(!model.jobs.rows[0].reason.is_empty());
        assert_eq!(
            shell.panes[0].as_ref().unwrap().status.as_ref(),
            shell.locale.catalog.tr("window.backend_stopped")
        );
    });
}

#[gpui::test]
fn u13_a_failed_transfer_enqueue_does_not_spend_the_cut(cx: &mut TestAppContext) {
    let (backend, mut lines) = Backend::test_operation_channel();
    let handle = open(cx, backend);
    let entity = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    let spent = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = spent.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(
            &entity,
            move |_, event: &super::join::OperationUpdate, _| {
                seen.set(seen.get() || event.spent_cut);
            },
        )
    });
    update(cx, handle, |shell, window, cx| {
        assert!(shell.ask_operation(0, &super::tests::request(), &[], true, window, cx));
        shell.receive(0, Event::Operations(r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes","d":false,"i":""}],"mergeable":false}"#.into()), window, cx);
        shell.panes[0]
            .as_ref()
            .unwrap()
            .backend
            .as_ref()
            .unwrap()
            .quit();
    });
    assert!(lines().unwrap().contains("collisions"));
    key(cx, handle, "enter");
    assert!(
        !spent.get(),
        "a refused enqueue preserves the real clipboard cut"
    );
    assert!(lines().is_none(), "no transfer was queued");
    update(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().error.is_some())
    });
}

#[gpui::test]
fn u13_jobs_mount_and_pointer_cancel_reaches_the_keyed_backend(cx: &mut TestAppContext) {
    let (backend, mut lines) = Backend::test_operation_channel();
    let handle = open(cx, backend);
    update(cx, handle, |shell, window, cx| {
        shell.receive(0, Event::Operations(r#"{"t":"jobs","jobs":[{"id":13,"kind":"copy","head":"Copying a.bin","state":"running"},{"id":14,"kind":"copy","head":"Copy b.bin","state":"waiting"}]}"#.into()), window, cx);
        shell
            .operation_view(0)
            .unwrap()
            .update(cx, |overlay, _| overlay.pinned = Some(0.));
    });
    super::draw_fixture::settle_progress(&gpui::VisualTestContext::from_window(handle.into(), cx));
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
    let job = visual
        .debug_bounds("op1-job-13")
        .expect("the running job draws in the live root");
    assert!(
        visual.debug_bounds("op1-job-14").is_some(),
        "the second job stays mounted"
    );
    let cancel = gpui::point(job.right() - gpui::px(5.), job.bottom() - gpui::px(10.));
    visual.simulate_mouse_move(cancel, None, gpui::Modifiers::default());
    visual.simulate_mouse_down(cancel, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.simulate_mouse_up(cancel, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    assert_eq!(
        lines().as_deref(),
        Some(r#"{"c":"jobs","op":"cancel","id":13}"#)
    );
    assert!(
        lines().is_none(),
        "the neighbour job receives no cancellation"
    );
}

#[path = "jobs_input_tests.rs"]
mod jobs_input_tests;
