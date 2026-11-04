//! Acceptance pins for the existing agent, captured Actions and shared Jobs routes.
use super::*;
use flea::ui_logic_op1::jobs::{self, Action};

fn paths_json(paths: &[&str]) -> String {
    paths
        .iter()
        .map(|path| format!("\"{}\"", flea::json::escape(path)))
        .collect::<Vec<_>>()
        .join(",")
}

// Sample input: {"paths":["/files/a.jpg","/files/b.jpg"]}.
fn paths(line: &str) -> Vec<String> {
    flea::jsondoc::parse(line)
        .unwrap()
        .get("paths")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

#[gpui::test]
fn m9_g2rows_18_agent_key_and_menu_use_same_captured_folder(cx: &mut TestAppContext) {
    let fixture = open(cx, SHOWN);
    let handle = fixture.handle;
    let launched = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    let log = launched.clone();
    let folder = "/files/photos with spaces";
    update(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().path = folder.into();
        shell.menu.agent = Some(Rc::new(move |folder| {
            log.borrow_mut().push(folder.to_path_buf());
            Ok(())
        }));
        shell.open_menu(0, None, (10., 10.), window, cx);
    });
    let quiet = status(cx, handle);
    fire(cx, handle, "openAgent", "");
    fire(cx, handle, "openAgent", "");
    assert_eq!(
        *launched.borrow(),
        [PathBuf::from(folder)],
        "a duplicate choice cannot launch twice"
    );
    assert_eq!(status(cx, handle), quiet);
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("shift-a");
    assert_eq!(
        *launched.borrow(),
        [PathBuf::from(folder), PathBuf::from(folder)]
    );
    assert_eq!(
        status(cx, handle),
        quiet,
        "successful launches leave status quiet"
    );
    assert_eq!(
        flea::ui_logic_mn2::agent::argv(),
        ["omarchy", "agent", "--pick"]
    );
}

#[gpui::test]
fn m9_g2rows_18_agent_start_failure_arrives_from_the_background_launch(cx: &mut TestAppContext) {
    let fixture = open(cx, SHOWN);
    let handle = fixture.handle;
    // No launcher is injected, so the real argv runs; a device is no working directory, so the child fails before exec.
    update(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().path = "/dev/null".into();
        shell.open_menu(0, None, (10., 10.), window, cx);
    });
    let quiet = status(cx, handle);
    fire(cx, handle, "openAgent", "");
    assert_eq!(
        status(cx, handle),
        quiet,
        "the launch waits for the background executor, so the menu returns before it is said"
    );
    cx.run_until_parked();
    let refused = std::io::Error::from_raw_os_error(20).to_string();
    let sentence = update(cx, handle, |shell, _, _| {
        shell
            .locale
            .catalog
            .tr_args(crate::g2::mn2::model::START_KEY, &[&refused])
    });
    assert_ne!(sentence, quiet);
    assert_eq!(
        status(cx, handle),
        sentence,
        "the background launch reports its start failure"
    );
}

#[gpui::test]
fn m9_g2rows_18_actions_match_and_capture_full_selection(cx: &mut TestAppContext) {
    for (rows, selected) in [
        (vec![2], vec!["/files/a.jpg"]),
        (
            vec![2, 4, 500],
            vec![
                "/files/a.jpg",
                "/files/$(touch should-not-run).jpg",
                "/files/line\nbreak.jpg",
            ],
        ),
    ] {
        let mut fixture = open(cx, SHOWN);
        let handle = fixture.handle;
        update(cx, handle, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().selected = rows.clone();
            shell.open_menu(0, Some(2), (10., 10.), window, cx);
        });
        let snapshot = next(&mut fixture, "snapshot");
        let document = flea::jsondoc::parse(&snapshot).unwrap();
        let captured: Vec<_> = document
            .get("rows")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_f64().unwrap() as usize)
            .collect();
        assert_eq!(
            captured, rows,
            "the capture includes marks beyond the viewport"
        );
        let id = flea::json::field_usize(&snapshot, "id").unwrap();
        receive(
            cx,
            handle,
            &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[]}}"#),
        );
        next(&mut fixture, "validate");
        let selection = paths_json(&selected);
        receive(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"__menuScripts","paths":[{selection}]}}"#
            ),
        );
        assert_eq!(paths(&next(&mut fixture, "list")), selected);
        receive(
            cx,
            handle,
            &format!(r#"{{"t":"actions","id":{id},{LISTING}}}"#),
        );
        next(&mut fixture, "applications");
        receive(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
            ),
        );
        let leaves = update(cx, handle, |shell, _, _| {
            shell
                .menu_entries()
                .into_iter()
                .find(|row| row.id.as_ref() == "actions")
                .unwrap()
                .submenu
                .unwrap()
                .iter()
                .map(|row| row.id.to_string())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            leaves,
            ["action:resize.toml", "script:ocr.sh"],
            "the provider's matches include legacy scripts and no excluded leaf"
        );
        assert!(!ids(cx, handle).iter().any(|id| id == "runScript"));
        fire(cx, handle, "actions", "action:resize.toml");
        let validate = next(&mut fixture, "validate");
        assert!(validate.contains("actions:action:resize.toml"));
        receive(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"actions:action:resize.toml","paths":[{selection}]}}"#
            ),
        );
        let run = next(&mut fixture, "run");
        assert_eq!(
            paths(&run),
            selected,
            "every captured path reaches the existing executor as data"
        );
        assert_eq!(
            flea::jsondoc::parse(&run)
                .unwrap()
                .get("action")
                .unwrap()
                .as_str(),
            Some("action:resize.toml")
        );
    }
}

#[gpui::test]
fn m9_g2rows_18_closed_menu_cannot_run_a_late_validation(cx: &mut TestAppContext) {
    let mut fixture = open(cx, SHOWN);
    let handle = fixture.handle;
    let id = discovered(cx, &mut fixture, None);
    fire(cx, handle, "actions", "action:resize.toml");
    next(&mut fixture, "validate");
    update(cx, handle, |shell, window, cx| {
        shell.retire_menu(window, cx)
    });
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"actions:action:resize.toml","paths":["/files/a.jpg"]}}"#
        ),
    );
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"actions","id":{id},{LISTING}}}"#),
    );
    assert!(!update(cx, handle, |shell, _, _| shell.menu_opened()));
    while let Some(line) = (fixture.requests)() {
        assert!(
            !line.contains("\"op\":\"run\""),
            "a dismissed menu must not execute: {line}"
        );
    }
}

#[gpui::test]
fn m9_g2rows_18_fresh_install_keeps_both_rows_hidden(cx: &mut TestAppContext) {
    let mut fixture = open(cx, "{}");
    let handle = fixture.handle;
    update(cx, handle, |shell, window, cx| {
        shell.open_menu(0, None, (10., 10.), window, cx)
    });
    assert!(!ids(cx, handle).iter().any(|id| id == "openAgent"));
    update(cx, handle, |shell, window, cx| {
        shell.open_menu(0, Some(2), (10., 10.), window, cx);
        shell.menu.actions.listing_requested(99);
        shell
            .menu
            .actions
            .receive(&format!(r#"{{"t":"actions","id":99,{LISTING}}}"#));
    });
    let shown = ids(cx, handle);
    assert!(
        !shown.iter().any(|id| id == "actions" || id == "runScript"),
        "even matching actions stay hidden at defaults: {shown:?}"
    );
    let snapshot = next(&mut fixture, "snapshot");
    let id = flea::json::field_usize(&snapshot, "id").unwrap();
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[]}}"#),
    );
    while let Some(line) = (fixture.requests)() {
        assert!(
            !line.contains("__menuScripts") && !line.contains("\"c\":\"actions\""),
            "hidden Actions does not discover: {line}"
        );
    }
}

#[gpui::test]
fn m9_g2rows_18_action_output_cancel_and_failure_reach_jobs(cx: &mut TestAppContext) {
    let (backend, mut commands) = Backend::test_lines();
    let mut commands = move || loop {
        let line = commands()?;
        match flea::jsondoc::parse(&line)
            .unwrap()
            .get("c")
            .and_then(Json::as_str)
        {
            Some("jobs") => return Some(line),
            Some("fsinfo") => {}
            _ => panic!("unexpected operation command: {line}"),
        }
    };
    let handle = cx.add_window(move |_, cx| Root {
        shell: cx.new(|cx| shell_with(backend, cx)),
    });
    receive(
        cx,
        handle,
        r#"{"t":"actionstarted","requestId":7,"id":41,"ok":true,"error":""}"#,
    );
    receive(
        cx,
        handle,
        r#"{"t":"job","job":{"id":41,"kind":"action","head":"Resize to 1600 px · 1 of 3","file":"a.jpg","done":1,"total":3,"state":"running","output":["first","second","third"]},"unfinished":1}"#,
    );
    update(cx, handle, |shell, _, cx| {
        let view = shell
            .operation_view(0)
            .expect("the action uses the shared Jobs overlay");
        assert!(
            shell.operation_view(1).is_none(),
            "the other pane receives no job"
        );
        view.update(cx, |overlay, cx| {
            let row = &overlay.model.jobs.rows[0];
            assert_eq!((row.id, row.done, row.total), (41, 1, 3));
            let block = jobs::block(&overlay.catalog, row);
            assert_eq!(block.output, ["second", "third"]);
            assert_eq!(block.bar, Some(1. / 3.));
            assert_eq!(block.buttons, [Action::Cancel]);
            overlay.model.job_action(41, Action::Cancel);
            cx.notify();
        });
        shell.flush_operations(0, cx);
    });
    assert_eq!(
        commands().as_deref(),
        Some(r#"{"c":"jobs","op":"cancel","id":41}"#)
    );
    receive(
        cx,
        handle,
        r#"{"t":"job","job":{"id":41,"kind":"action","head":"Resize to 1600 px failed","done":1,"total":3,"state":"failed","reason":"Action exited with 7.","output":[]},"unfinished":0}"#,
    );
    update(cx, handle, |shell, _, cx| {
        let view = shell.operation_view(0).unwrap();
        view.update(cx, |overlay, cx| {
            assert_eq!(
                overlay.model.jobs.rows.len(),
                1,
                "the failed update replaces the same job"
            );
            let block = jobs::block(&overlay.catalog, &overlay.model.jobs.rows[0]);
            assert_eq!(block.file, "Action exited with 7.");
            assert_eq!(block.buttons, [Action::Retry, Action::Dismiss]);
            overlay.model.job_action(41, Action::Cancel);
            assert!(
                overlay.model.outbox.is_empty(),
                "a finished job cannot be cancelled"
            );
            overlay.model.job_action(41, Action::Retry);
            cx.notify();
        });
        shell.flush_operations(0, cx);
    });
    assert_eq!(
        commands().as_deref(),
        Some(r#"{"c":"jobs","op":"retry","id":41}"#)
    );
    assert!(
        commands().is_none(),
        "no second executor or neighbour command"
    );
}
