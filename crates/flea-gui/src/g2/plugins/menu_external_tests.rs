//! `flea --action` reaches the window through its IPC reader: built-ins through the key path, plugin actions through the menu row path.
use super::host_tests::{attach, receipt, stop, wait};
use super::*;
use crate::ipc::server::{Answer, Event as IpcEvent};
use flea::ipc::wire::{Reply, Request};
use flea::keys::actions::{UNAVAILABLE, UNKNOWN};
use std::sync::mpsc;

fn call(
    cx: &mut TestAppContext,
    fixture: &Fixture,
    function: &str,
    arguments: &[&str],
) -> mpsc::Receiver<Answer> {
    let (answer, receiver) = mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: function.into(),
        arguments: arguments.iter().map(|a| a.to_string()).collect(),
    };
    update(cx, fixture.handle, |shell, window, cx| {
        shell.ipc_cold = false;
        shell.ipc(IpcEvent::Call { request, answer }, window, cx)
    });
    receiver
}

fn now(receiver: mpsc::Receiver<Answer>) -> Reply {
    receiver
        .try_recv()
        .expect("the window answered at once")
        .reply
}

fn tr(cx: &mut TestAppContext, fixture: &Fixture, key: &str) -> String {
    update(cx, fixture.handle, |shell, _, _| {
        format!("flea: {}\n", shell.locale.catalog.tr(key))
    })
}

#[gpui::test]
fn builtin_actions_run_the_key_path_and_refuse_as_the_window_does(cx: &mut TestAppContext) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    let fixture = open(cx, root, plugin);
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 0
    });

    assert_eq!(
        now(call(cx, &fixture, "runAction", &["cursorDown"])).code,
        0
    );
    update(cx, fixture.handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 1)
    });
    assert_eq!(now(call(cx, &fixture, "runAction", &["selectAll"])).code, 0);
    update(cx, fixture.handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected.len(), 5)
    });

    // A caller with no one to answer, a key binding, runs the same path.
    update(cx, fixture.handle, |shell, window, cx| {
        shell.run_action(
            "cursorDown",
            crate::window::external_action::Waiting::none(),
            window,
            cx,
        )
    });
    update(cx, fixture.handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 2)
    });
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 1
    });

    let unknown = now(call(cx, &fixture, "runAction", &["noSuchAction"]));
    assert_eq!((unknown.code, unknown.stderr.lines().count()), (UNKNOWN, 1));
    assert_eq!(
        now(call(cx, &fixture, "runAction", &["copyArm"])).code,
        UNKNOWN,
        "a pair arm is not an id"
    );

    // A key in the Menu context cannot reach a listing action, so the outside call is refused with the window's own sentence.
    update(cx, fixture.handle, |shell, _, _| {
        shell.keyboard.context = flea::keys::Context::Menu
    });
    let refused = now(call(cx, &fixture, "runAction", &["cursorDown"]));
    assert_eq!(refused.code, UNAVAILABLE);
    assert_eq!(
        refused.stderr,
        tr(cx, &fixture, "feedback.action.unavailable")
    );
    update(cx, fixture.handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 1)
    });

    update(cx, fixture.handle, |shell, _, _| {
        shell.keyboard.context = flea::keys::Context::Listing
    });

    // A key on a busy listing is held and replays later; an outside caller is refused instead, and nothing is held.
    update(cx, fixture.handle, |shell, _, _| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        let path = pane.place().to_owned();
        pane.swap.pending = Some(crate::pane::swap::Pending {
            path,
            total: None,
            first: 0,
            ready: None,
            following: None,
            parked_until: None,
            retained_start: None,
            retained_cursor: None,
            options: Default::default(),
        });
    });
    let busy = now(call(cx, &fixture, "runAction", &["cursorDown"]));
    assert_eq!(
        (busy.code, busy.stderr),
        (UNAVAILABLE, tr(cx, &fixture, "pane.busy"))
    );
    update(cx, fixture.handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(
            host.input.queued.is_none(),
            "the refused action is not held for the re-read"
        );
        assert_eq!(host.pane.cursor, 1);
        host.pane.swap.pending = None;
    });

    let ids = now(call(cx, &fixture, "actionIds", &[]));
    let ids: Vec<&str> = ids.stdout.lines().collect();
    assert_eq!(
        ids,
        flea::keys::actions::builtin_ids(),
        "no plugin is switched on in the window's host"
    );
}

#[gpui::test]
fn a_cold_window_refuses_actions_instead_of_answering_target_not_found(cx: &mut TestAppContext) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    let fixture = open(cx, root, plugin);
    let (answer, receiver) = mpsc::sync_channel(1);
    let request = Request::Call {
        target: "flea".into(),
        function: "runAction".into(),
        arguments: vec!["cursorDown".into()],
    };
    update(cx, fixture.handle, |shell, window, cx| {
        shell.ipc_cold = true;
        shell.ipc(IpcEvent::Call { request, answer }, window, cx)
    });
    let reply = now(receiver);
    assert_eq!(reply.code, UNAVAILABLE);
    assert_eq!(reply.stderr, tr(cx, &fixture, "menu.refusal.unavailable"));
}

#[gpui::test]
fn plugin_actions_take_the_menu_row_path_only_while_their_plugin_is_on(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    drop(runtime);
    let mut fixture = open(cx, root.clone(), plugin.clone());
    let folder = attach(cx, &mut fixture, &root, "checksums", &plugin);
    let id = "example.checksums:sha256";

    let ids = now(call(cx, &fixture, "actionIds", &[]));
    assert!(
        ids.stdout.lines().any(|line| line == id),
        "an enabled plugin's action is listed"
    );
    assert!(ids.stdout.lines().any(|line| line == "selectAll"));

    // The cursor on a folder: the row's `when` excludes folders, so the menu's own refusal comes back.
    update(cx, fixture.handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 4;
        host.selected.clear();
    });
    let pending = call(cx, &fixture, "runAction", &[id]);
    let snapshot = |fixture: &mut Fixture, cx: &mut TestAppContext| {
        let mut snapshot = None;
        wait(cx, |cx| {
            // The readiness poll runs on the test executor's clock, which moves only when told to.
            cx.executor().advance_clock(Duration::from_millis(20));
            snapshot = snapshot.take().or_else(|| {
                std::iter::from_fn(&mut fixture.lines)
                    .map(|line| jsondoc::parse(&line).unwrap())
                    .find(|r| r.get("op").and_then(Json::as_str) == Some("snapshot"))
            });
            snapshot.is_some()
        });
        let request = snapshot.unwrap();
        update(cx, fixture.handle, |shell, _, _| {
            assert!(
                !shell.menu_opened(),
                "an outside action never draws the menu"
            );
        });
        fixture.feed(
            cx,
            Json::Obj(vec![
                ("t".into(), Json::Str("menuaction".into())),
                ("op".into(), Json::Str("snapshot".into())),
                ("id".into(), request.get("id").unwrap().clone()),
                ("ok".into(), Json::Bool(true)),
                ("kinds".into(), Json::Arr(vec![])),
            ]),
        );
    };
    snapshot(&mut fixture, cx);
    let refused = pending.recv_timeout(Duration::from_secs(5)).unwrap().reply;
    assert_eq!(refused.code, UNAVAILABLE);
    assert_eq!(refused.stderr, tr(cx, &fixture, "menu.refusal.unavailable"));
    assert!(receipt(&folder, "run").is_empty());

    // On a text file the same id is the row a click chooses: validated by the backend, then the plugin's input card.
    update(cx, fixture.handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = 0
    });
    let pending = call(cx, &fixture, "runAction", &[id]);
    snapshot(&mut fixture, cx);
    let validate = fixture
        .drain()
        .into_iter()
        .find(|r| r.get("op").and_then(Json::as_str) == Some("validate"))
        .expect("the row is validated like a click");
    assert_eq!(validate.get("action").and_then(Json::as_str), Some(id));
    fixture.validated(cx, &validate, &[root.join("notes.txt")]);
    assert_eq!(
        pending
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .reply
            .code,
        0
    );
    let revalidate = fixture.submit_input(cx, CHECKSUM).unwrap();
    fixture.validated(cx, &revalidate, &[root.join("notes.txt")]);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    std::fs::write(folder.join("release-terminal"), "release\n").unwrap();
    wait(cx, |_| !receipt(&folder, "run").is_empty());
    assert_eq!(
        receipt(&folder, "run")[0].get("paths"),
        Some(&Json::Arr(vec![Json::Str(
            root.join("notes.txt").to_string_lossy().into()
        )]))
    );

    // A row still delivering belongs to its caller: a second outside call is refused at once and leaves it in place.
    update(cx, fixture.handle, |shell, _, _| {
        shell.menu.activating = true
    });
    let second = now(call(cx, &fixture, "runAction", &[id]));
    assert_eq!(
        (second.code, second.stderr),
        (UNAVAILABLE, tr(cx, &fixture, "menu.refusal.unavailable"))
    );
    update(cx, fixture.handle, |shell, _, _| {
        shell.menu.activating = false
    });

    // Switched off, the plugin's id is gone from the list and unknown to runAction.
    update(cx, fixture.handle, |shell, _, cx| {
        let host = shell.plugins.host.clone().unwrap();
        host.update(cx, |host, _| {
            for p in host
                .plugins
                .iter_mut()
                .filter(|p| p.manifest.id == "example.checksums")
            {
                p.enabled = false;
            }
        });
    });
    let ids = now(call(cx, &fixture, "actionIds", &[]));
    assert!(!ids.stdout.lines().any(|line| line == id));
    assert_eq!(now(call(cx, &fixture, "runAction", &[id])).code, UNKNOWN);
    stop(cx, &fixture);
}

#[test]
fn a_dropped_waiting_answers_its_caller_and_a_key_binding_has_no_caller() {
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(crate::window::external_action::Waiting::new(
        sender,
        "flea: moved".into(),
    ));
    let reply = receiver
        .try_recv()
        .expect("a dropped caller is answered")
        .reply;
    assert_eq!(
        (reply.code, reply.stderr.as_str()),
        (UNAVAILABLE, "flea: moved\n")
    );
    drop(crate::window::external_action::Waiting::none());
}

#[gpui::test]
fn plugin_readiness_wait_refuses_a_selection_that_moved(cx: &mut TestAppContext) {
    let root = scratch();
    let (plugin, runtime) = plugin("checksums", &root);
    runtime.shutdown();
    let mut fixture = open(cx, root, plugin.clone());
    update(cx, fixture.handle, |shell, window, cx| {
        shell.follow_plugins(window, cx);
        shell.plugins.host.clone().unwrap().update(cx, |host, _| {
            host.plugins = vec![plugin];
        });
        shell.panes[0].as_mut().unwrap().pane.cursor = 0;
    });

    let pending = call(cx, &fixture, "runAction", &["example.checksums:sha256"]);
    cx.run_until_parked();
    assert!(matches!(pending.try_recv(), Err(mpsc::TryRecvError::Empty)));
    update(cx, fixture.handle, |shell, _, cx| {
        assert!(
            shell.menu.captured.is_none(),
            "the plugin is still starting"
        );
        shell.panes[0].as_mut().unwrap().pane.cursor = 2;
        // Built-in source makes the fixture healthy without an unrelated plugin-host process.
        shell.plugins.host.clone().unwrap().update(cx, |host, _| {
            host.plugins[0].source = Source::Builtin;
        });
    });
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();

    update(cx, fixture.handle, |shell, _, _| {
        assert!(
            shell.menu.captured.is_none(),
            "the changed selection was never captured"
        );
        assert!(shell.menu.pending.is_none());
    });
    let reply = now(pending);
    assert_eq!(
        (reply.code, reply.stderr),
        (UNAVAILABLE, tr(cx, &fixture, "menu.refusal.moved"))
    );
    assert!(!fixture
        .drain()
        .iter()
        .any(|r| r.get("op").and_then(Json::as_str) == Some("snapshot")));
}
