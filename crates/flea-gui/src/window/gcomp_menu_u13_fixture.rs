//! Ready operation-menu fixture, retaining all effective hidden defaults except two actions.
use super::*;
use flea::jsondoc::Json;
use std::rc::Rc;

fn settle(shell: &mut crate::state::Shell) {
    // This ready fixture has received both independent filesystem answers.
    for side in 0..2 {
        let host = shell.panes[side].as_mut().unwrap();
        host.sizes = crate::list::sizes::State::settled_fixture(
            Rc::clone(&host.pane.held),
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
        shell.surfaces[side].filesystem =
            crate::window::status::Filesystem::settled_fixture(facts, host.pane.held.listing);
    }
    // Enable only these two shipped-hidden actions for the real menu path.
    let stored = shell
        .launch
        .settings
        .data
        .get("menu")
        .and_then(|menu| menu.get("hidden"))
        .and_then(Json::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    let shelf = shell
        .launch
        .settings
        .data
        .get("shelf")
        .and_then(|shelf| shelf.get("enabled"))
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let hidden =
        flea::ui_logic_context_menus::inventory::effective_hidden(stored.as_deref(), shelf)
            .into_iter()
            .filter(|id| id != "copyto" && id != "moveto")
            .map(Json::Str)
            .collect();
    let Json::Obj(settings) = &mut shell.launch.settings.data else {
        panic!("settings object")
    };
    if !settings.iter().any(|(key, _)| key == "menu") {
        settings.push(("menu".into(), Json::Obj(Vec::new())));
    }
    let Json::Obj(menu) = &mut settings
        .iter_mut()
        .find(|(key, _)| key == "menu")
        .unwrap()
        .1
    else {
        panic!("menu object")
    };
    if let Some((_, value)) = menu.iter_mut().find(|(key, _)| key == "hidden") {
        *value = Json::Arr(hidden);
    } else {
        menu.push(("hidden".into(), Json::Arr(hidden)));
    }
}

#[test]
fn u13_join_real_u06_copy_move_choice_keeps_enqueue_origin_and_claims_once() {
    for action in ["copyTo", "moveTo"] {
        let mut cx = crate::list::bench::real_cx();
        let handle = open(&mut cx);
        let (backend, mut requests) = Backend::test_lines();
        let (other, mut other_requests) = Backend::test_lines();
        edit(&mut cx, handle, |shell, _, _| {
            settle(shell);
            shell.panes[0].as_mut().unwrap().backend = Some(backend);
            shell.panes[1].as_mut().unwrap().backend = Some(other);
        });
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("m");
        ready(&mut cx, handle, &mut requests);
        frame(&mut cx, handle);
        let model = json(&mut cx, handle, "contextMenuModel");
        let rows = model.as_array().unwrap();
        let target = rows
            .iter()
            .position(|row| row.get("action").and_then(Json::as_str) == Some(action))
            .unwrap();
        for _ in 0..rows.len() {
            let cursor: usize = call(&mut cx, handle, "contextMenuCursor")
                .stdout
                .trim()
                .parse()
                .unwrap();
            if cursor == target {
                break;
            }
            VisualTestContext::from_window(handle.into(), &cx)
                .simulate_keystrokes(if cursor < target { "down" } else { "up" });
        }
        let captured = edit(&mut cx, handle, |shell, _, _| {
            shell.menu.captured.clone().unwrap()
        });
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("enter");
        assert_eq!(menu_request(&mut requests, "validate"), captured.id);
        // The successful answer arrives after focus moved. No origin is reconstructed here.
        edit(&mut cx, handle, |shell, _, _| shell.focus_side = 1);
        let answer = format!(
            r#"{{"t":"menuaction","id":{},"op":"validate","ok":true,"action":"{action}","paths":["/files/file-0.txt"]}}"#,
            captured.id
        );
        receive(&mut cx, handle, &answer);
        frame(&mut cx, handle);
        let (view, chosen) = edit(&mut cx, handle, |shell, _, _| {
            let origin = shell.operations.origins[0]
                .as_ref()
                .expect("actual U06 Chosen reached U13");
            assert_eq!(origin.captured.side, captured.side);
            assert_eq!(origin.captured.id, captured.id);
            assert_eq!(origin.captured.listing, captured.listing);
            assert_eq!(origin.captured.rows, captured.rows);
            assert_eq!(origin.captured.cursor, captured.cursor);
            assert_eq!(origin.captured.path, captured.path);
            assert_eq!(origin.captured.identity, captured.identity);
            assert_eq!(origin.paths, ["/files/file-0.txt"]);
            assert_eq!(origin.action, action);
            assert!(shell.operations.origins[1].is_none());
            (shell.operation_view(0).unwrap(), origin.clone())
        });
        VisualTestContext::from_window(handle.into(), &cx).simulate_input("/chosen-destination");
        receive(&mut cx, handle, &answer);
        frame(&mut cx, handle);
        view.read_with(&cx, |view, _| {
            assert!(matches!(&view.model.dialog, Some(crate::g2::file_ops::model::Dialog::CopyTo {menu, moving, path, ..})
                if *menu == captured.id && *moving == (action == "moveTo") && path == "/chosen-destination"));
        });
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("enter");
        let sent = requests().expect("the captured source backend receives destination validation");
        assert_eq!(
            sent,
            format!(
                r#"{{"c":"menuaction","op":"validate","id":{},"action":"{action}","dest":"/chosen-destination"}}"#,
                captured.id
            ),
            "the complete captured operation wire request must match"
        );
        assert!(
            requests().is_none(),
            "a duplicate answer cannot submit twice"
        );
        assert!(
            other_requests().is_none(),
            "later focus must never own this operation"
        );
        assert!(chosen.claimed.get(), "the actual U06 choice was consumed");
        VisualTestContext::from_window(handle.into(), &cx).simulate_keystrokes("escape");
        frame(&mut cx, handle);
        view.read_with(&cx, |view, _| assert!(view.model.dialog.is_none()));
        assert_eq!(
            requests().as_deref(),
            Some(format!(r#"{{"c":"menuaction","op":"close","id":{}}}"#, captured.id).as_str()),
            "retiring the actual card closes its captured menu session"
        );
        // Replay the exact already-claimed producer event after its card has retired.
        edit(&mut cx, handle, |_, _, cx| cx.emit(chosen.clone()));
        frame(&mut cx, handle);
        view.read_with(&cx, |view, _| {
            assert!(
                view.model.dialog.is_none(),
                "an already-claimed choice cannot reopen a card"
            );
        });
        assert!(
            requests().is_none(),
            "an already-claimed choice cannot queue another operation"
        );
        assert!(
            other_requests().is_none(),
            "replay cannot change the captured backend"
        );
    }
}
