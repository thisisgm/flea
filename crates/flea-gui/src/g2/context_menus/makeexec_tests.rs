//! The real menu host, backend probe and permission writer cover Make executable end to end.
#[path = "makeexec_fixture.rs"]
mod fixture;
#[path = "flyleaf_tests.rs"]
mod flyleaf;
#[path = "makeexec_menurefuse_tests.rs"]
mod menurefuse;
#[path = "makeexec_review_tests.rs"]
mod review;
#[path = "makeexec_sw03_tests.rs"]
mod sw03;
use crate::g2::context_menus::join_tests::read;
use fixture::{number, update, Fixture, Process};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;
use std::os::unix::fs::PermissionsExt;

#[gpui::test]
fn makeexec_script_row_lands_in_the_same_open_menu_after_the_threaded_probe(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::new(cx, 0o644);
    let request = fixture
        .open(cx, 0, vec![])
        .expect("one regular non-executable cursor target asks");
    assert!(
        !fixture.has_action(cx),
        "the menu must not guess from a script's name"
    );
    fixture.probe(cx, &request);
    assert!(
        fixture.has_action(cx),
        "the producer must fill MenuContext.has_shebang and refresh the already open menu"
    );
    let model = fixture.model(cx);
    let row = model
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("action").and_then(Json::as_str) == Some("makeExecutable"))
        .unwrap();
    assert_eq!(
        row.get("label").and_then(Json::as_str),
        Some("Make Executable")
    );
    assert_eq!(row.get("glyph").and_then(Json::as_str), Some("play"));
    let state = jsondoc::parse(read(cx, fixture.handle, "menuState").stdout.trim()).unwrap();
    assert_eq!(state.get("shebangHas").and_then(Json::as_bool), Some(true));
    assert_eq!(
        state.get("shebangId"),
        state.get("shebangReply").unwrap().get("id")
    );
    assert_eq!(
        state.get("pane"),
        state.get("shebangReply").unwrap().get("pane")
    );
    assert_eq!(state.get("shebangAsked"), request.get("path"));
    assert_eq!(state.get("shebangReply"), state.get("shebangLastReply"));
}

#[gpui::test]
fn makeexec_plain_file_probe_is_observable_and_offers_no_row(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    let request = fixture.open(cx, 1, vec![]).unwrap();
    fixture.probe(cx, &request);
    assert!(!fixture.has_action(cx));
    let state = jsondoc::parse(read(cx, fixture.handle, "menuState").stdout.trim()).unwrap();
    assert_eq!(state.get("shebangHas").and_then(Json::as_bool), Some(false));
    assert_eq!(
        state
            .get("shebangReply")
            .unwrap()
            .get("hasShebang")
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        state.get("shebangReply").unwrap().get("path"),
        request.get("path")
    );
}

#[gpui::test]
fn makeexec_any_execute_bit_needs_neither_probe_nor_row(cx: &mut TestAppContext) {
    for mode in [0o744, 0o654, 0o645] {
        let mut fixture = Fixture::new(cx, mode);
        assert!(fixture.open(cx, 0, vec![]).is_none());
        assert!(!fixture.has_action(cx));
    }
}

#[gpui::test]
fn makeexec_two_files_or_a_single_mark_away_from_cursor_offer_no_row(cx: &mut TestAppContext) {
    for marks in [vec![0, 1], vec![1]] {
        let mut fixture = Fixture::new(cx, 0o644);
        assert!(fixture.open(cx, 0, marks).is_none());
        assert!(!fixture.has_action(cx));
    }
}

#[gpui::test]
fn makeexec_older_id_same_path_and_other_path_never_fill_the_new_menu(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    let old = fixture.open(cx, 0, vec![]).unwrap();
    let current = fixture.open(cx, 0, vec![]).unwrap();
    assert!(number(&current) > number(&old));
    fixture.probe(cx, &old);
    assert!(!fixture.has_action(cx));
    update(cx, fixture.handle, |shell, _, _| {
        assert!(!shell.menu.shebang.has);
        assert!(!shell.menu.context.has_shebang);
        assert!(shell.menu.shebang.reply.is_none());
    });
    fixture.receive(
        cx,
        &format!(
            "{{\"t\":\"shebang\",\"id\":{},\"path\":\"/different\",\"hasShebang\":true}}",
            number(&current)
        ),
    );
    assert!(!fixture.has_action(cx));
    fixture.probe(cx, &current);
    assert!(fixture.has_action(cx));
    let plain = fixture.open(cx, 1, vec![]).unwrap();
    fixture.probe(cx, &current);
    assert!(!fixture.has_action(cx));
    fixture.probe(cx, &plain);
    assert!(!fixture.has_action(cx));
}

#[gpui::test]
fn makeexec_action_uses_the_permissions_batch_sets_only_owner_x_and_refreshes_mode(
    cx: &mut TestAppContext,
) {
    for (before, after) in [(0o644, 0o744), (0o640, 0o740)] {
        let mut fixture = Fixture::new(cx, before);
        let request = fixture.open(cx, 0, vec![]).unwrap();
        fixture.probe(cx, &request);
        let batch = fixture.choose(cx);
        let path = batch.get("paths").unwrap().as_array().unwrap()[0]
            .as_str()
            .unwrap();
        let mode = batch.get("modes").unwrap().as_array().unwrap()[0]
            .as_str()
            .unwrap();
        assert_eq!(mode, "u+x");
        assert!(
            std::path::Path::new(path).is_absolute()
                && std::path::Path::new(path).starts_with(&fixture.root)
        );
        assert!(fixture.root.join(".flea-test-sandbox").is_file());
        let mut backend = Process::new(&fixture.root);
        let reply = backend.request(&batch, "permissions");
        assert_eq!(
            jsondoc::parse(&reply)
                .unwrap()
                .get("ok")
                .and_then(Json::as_bool),
            Some(true),
            "{reply}"
        );
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o7777,
            after
        );
        fixture.receive(cx, &reply);
        assert_eq!(
            read(cx, fixture.handle, "lastMessage").stdout.trim(),
            "Made it executable. · z undoes"
        );
        assert!(
            fixture
                .drain()
                .iter()
                .any(|r| r.get("c").and_then(Json::as_str) == Some("list")),
            "success rereads the mode from the backend"
        );
        update(cx, fixture.handle, |shell, window, cx| {
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Listed {
                    path: fixture.root.to_string_lossy().into(),
                    total: 1,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                    start: 0,
                    rows: vec![flea::protocol::RowFacts {
                        name: "build.sh".into(),
                        mode: 0o100000 | after,
                        ..Default::default()
                    }],
                    listing: 3,
                }),
                window,
                cx,
            );
        });
        assert!(fixture.open(cx, 0, vec![]).is_none());
        assert!(!fixture.has_action(cx));
        update(cx, fixture.handle, |shell, window, cx| {
            shell.retire_menu(window, cx);
            assert!(shell.operation_action("undo", cx));
        });
        let undo = fixture
            .drain()
            .into_iter()
            .find(|r| r.get("c").and_then(Json::as_str) == Some("undo"))
            .unwrap();
        let undone = backend.request(&undo, "undone");
        assert_eq!(
            jsondoc::parse(&undone)
                .unwrap()
                .get("op")
                .and_then(Json::as_str),
            Some("permissions")
        );
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o7777,
            before
        );
        fixture.receive(cx, &undone);
        assert_eq!(
            read(cx, fixture.handle, "lastMessage").stdout.trim(),
            "Undid the permissions"
        );
    }
}

#[gpui::test]
fn makeexec_failure_preserves_backend_error_and_has_the_oracle_fallback(cx: &mut TestAppContext) {
    for (error, expected) in [
        ("Permission denied", "Permission denied"),
        ("", "That file could not be made executable."),
    ] {
        let mut fixture = Fixture::new(cx, 0o644);
        let request = fixture.open(cx, 0, vec![]).unwrap();
        fixture.probe(cx, &request);
        let batch = fixture.choose(cx);
        fixture.receive(cx, &format!("{{\"t\":\"permissions\",\"id\":{},\"op\":\"applyMany\",\"ok\":false,\"error\":\"{error}\"}}", number(&batch) + 1));
        assert!(
            !read(cx, fixture.handle, "lastMessage")
                .stdout
                .contains(expected),
            "another id cannot finish the action"
        );
        fixture.receive(cx, &format!("{{\"t\":\"permissions\",\"id\":{},\"op\":\"applyMany\",\"ok\":false,\"error\":\"{error}\"}}", number(&batch)));
        assert_eq!(
            read(cx, fixture.handle, "lastMessage").stdout.trim(),
            expected
        );
        assert_eq!(
            std::fs::metadata(fixture.root.join("build.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
}

#[gpui::test]
fn makeexec_action_refuses_a_target_that_is_not_the_single_cursor_file(cx: &mut TestAppContext) {
    let mut fixture = Fixture::new(cx, 0o644);
    fixture.open(cx, 0, vec![0, 1]);
    update(cx, fixture.handle, |shell, window, cx| {
        let capture = shell.menu.captured.clone().unwrap();
        shell.make_executable(
            &capture,
            &[fixture.root.join("build.sh").to_string_lossy().into()],
            window,
            cx,
        );
    });
    assert_eq!(
        read(cx, fixture.handle, "lastMessage").stdout.trim(),
        "Make executable works on a script that cannot run yet"
    );
    assert!(!fixture
        .drain()
        .iter()
        .any(|r| r.get("c").and_then(Json::as_str) == Some("permissionsBatch")));
}

#[gpui::test]
fn makeexec_real_batch_failure_keeps_the_file_and_reports_the_backend_sentence(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::new(cx, 0o4644);
    let request = fixture.open(cx, 0, vec![]).unwrap();
    fixture.probe(cx, &request);
    let batch = fixture.choose(cx);
    let reply = Process::new(&fixture.root).request(&batch, "permissions");
    let error = "Special permissions cannot be edited.";
    assert_eq!(
        jsondoc::parse(&reply)
            .unwrap()
            .get("error")
            .and_then(Json::as_str),
        Some(error)
    );
    fixture.receive(cx, &reply);
    assert_eq!(read(cx, fixture.handle, "lastMessage").stdout.trim(), error);
    assert_eq!(
        std::fs::metadata(fixture.root.join("build.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o4644
    );
}

#[gpui::test]
fn makeexec_probe_uses_the_search_listing_base_and_a_closed_menu_never_reopens(
    cx: &mut TestAppContext,
) {
    let mut fixture = Fixture::new(cx, 0o644);
    update(cx, fixture.handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.path = "/previous-folder".into();
        host.search.owner.session.base = fixture.root.to_string_lossy().into();
    });
    let request = fixture.open(cx, 0, vec![]).unwrap();
    assert_eq!(
        request.get("path").and_then(Json::as_str),
        Some(fixture.root.join("build.sh").to_str().unwrap())
    );
    update(cx, fixture.handle, |shell, window, cx| {
        shell.retire_menu(window, cx)
    });
    fixture.probe(cx, &request);
    assert!(!fixture.has_action(cx));
    assert_eq!(
        read(cx, fixture.handle, "contextMenuVisible").stdout,
        "false\n"
    );
}
