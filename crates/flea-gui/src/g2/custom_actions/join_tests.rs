//! The production Shell with no test subscriber: Open in Agent and an Actions leaf reach their effects from the captured menu, and a stale capture or reply reaches nothing.
use crate::{
    backend::{Backend, Event},
    g2::context_menus::menu::Outcome,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::jsondoc::Json;
use gpui::{AppContext, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

struct Fixture {
    handle: WindowHandle<Root>,
    requests: Box<dyn FnMut() -> Option<String>>,
}

// Every menu row shown and stamped past the 0.4.0 step, so Actions (Run Script's switch) and Open in Agent both draw.
const SHOWN: &str = r#"{"menu":{"hidden":[]},"stateVersion":4}"#;

fn open(cx: &mut TestAppContext, settings: &str) -> Fixture {
    let (backend, mut receive) = Backend::test_lines();
    let receive = move || loop {
        let line = receive()?;
        let document = flea::jsondoc::parse(&line).unwrap();
        if matches!(
            document.get("c").and_then(Json::as_str),
            Some("menuaction" | "actions")
        ) {
            return Some(line);
        }
    };
    let settings = settings.to_string();
    let handle = cx.add_window(move |window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.launch.settings.data = flea::uistate::from_file(&settings);
            shell
        });
        shell.update(cx, |shell, cx| window.focus(&shell.focus, cx));
        Root { shell }
    });
    Fixture {
        handle,
        requests: Box::new(receive),
    }
}

fn update<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    callback: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .update(cx, |shell, cx| callback(shell, window, cx))
        })
        .unwrap()
}

fn receive(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    update(cx, handle, |shell, window, cx| {
        shell.receive(0, Event::Producer(line.into()), window, cx)
    });
}

fn next(fixture: &mut Fixture, op: &str) -> String {
    loop {
        let line = (fixture.requests)().unwrap_or_else(|| panic!("the backend receives {op}"));
        if line.contains(&format!("\"op\":\"{op}\"")) {
            return line;
        }
        assert!(line.contains("\"op\":\"close\""), "{line}");
    }
}

fn ids(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Vec<String> {
    update(cx, handle, |shell, _, _| {
        shell
            .menu_entries()
            .iter()
            .map(|entry| entry.id.as_ref().to_string())
            .collect()
    })
}

fn fire(cx: &mut TestAppContext, handle: WindowHandle<Root>, action: &str, leaf: &str) {
    update(cx, handle, |shell, window, cx| {
        shell.menu_outcome(
            Outcome::Fire {
                action: action.into(),
                leaf: leaf.into(),
            },
            window,
            cx,
        )
    });
}

fn status(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> String {
    update(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().status.to_string()
    })
}

const LISTING: &str = r#""actions":[{"id":"action:resize.toml","name":"Resize to 1600 px","mode":"background","selection":"many","script":false,"mime":["image/*"]},{"id":"script:ocr.sh","name":"ocr","mode":"terminal","selection":"many","script":true}],"errors":[],"error":"""#;

// The file menu up to its discovered Actions row: snapshot, the resolved selection, then the listing under the menu's own id.
fn discovered(cx: &mut TestAppContext, fixture: &mut Fixture, listing_id: Option<u64>) -> u64 {
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let id = flea::json::field_usize(&next(fixture, "snapshot"), "id").unwrap() as u64;
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[]}}"#),
    );
    let validate = next(fixture, "validate");
    assert!(validate.contains("__menuScripts"), "{validate}");
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"__menuScripts","paths":["/files/a.jpg"]}}"#
        ),
    );
    let list = next(fixture, "list");
    assert!(list.contains("\"paths\":[\"/files/a.jpg\"]"), "{list}");
    let answered = listing_id.unwrap_or(id);
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"actions","id":{answered},{LISTING}}}"#),
    );
    if answered == id {
        let apps = next(fixture, "applications");
        assert!(apps.contains(&format!("\"id\":{id}")), "{apps}");
        receive(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
            ),
        );
    }
    id
}

#[gpui::test]
fn m9_integ_agent_and_custom_action_use_captured_menu_origin(cx: &mut TestAppContext) {
    let mut fixture = open(cx, SHOWN);
    let handle = fixture.handle;
    let id = discovered(cx, &mut fixture, None);
    let shown = ids(cx, handle);
    assert!(
        shown.iter().any(|id| id == "actions"),
        "Actions takes Run Script's place: {shown:?}"
    );
    assert!(!shown.iter().any(|id| id == "runScript"), "{shown:?}");
    assert!(
        !shown.iter().any(|id| id == "openAgent"),
        "the file menu never asks for the agent row"
    );
    fire(cx, handle, "actions", "action:resize.toml");
    let validate = next(&mut fixture, "validate");
    assert!(
        validate.contains("\"action\":\"actions:action:resize.toml\""),
        "{validate}"
    );
    let reply = format!(
        r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"actions:action:resize.toml","paths":["/files/a.jpg"]}}"#
    );
    receive(cx, handle, &reply);
    receive(cx, handle, &reply);
    let run = next(&mut fixture, "run");
    assert!(
        run.contains("\"action\":\"action:resize.toml\"")
            && run.contains("\"paths\":[\"/files/a.jpg\"]"),
        "the leaf runs over the validated capture: {run}"
    );
    let mut more = Vec::new();
    while let Some(line) = (fixture.requests)() {
        more.push(line);
    }
    assert!(
        more.iter().all(|line| !line.contains("\"op\":\"run\"")),
        "a duplicate validation runs nothing: {more:?}"
    );
    let request = flea::json::field_usize(&run, "id").unwrap();
    receive(
        cx,
        handle,
        &format!(r#"{{"t":"actionstarted","requestId":{request},"id":40,"ok":true,"error":""}}"#),
    );
    let quiet = status(cx, handle);
    receive(
        cx,
        handle,
        &format!(
            r#"{{"t":"actionstarted","requestId":{request},"ok":false,"error":"Action exited with 3."}}"#
        ),
    );
    let said = status(cx, handle);
    assert_ne!(said, quiet, "a refused run is said on the status bar");
    let expected = update(cx, handle, |shell, _, _| {
        shell
            .locale
            .catalog
            .tr_args(flea::ui_logic_custom_actions::wire::EXITED_KEY, &["3"])
    });
    assert_eq!(said, expected);
}

#[gpui::test]
fn m9_integ_a_stale_listing_or_a_moved_capture_runs_nothing(cx: &mut TestAppContext) {
    let mut fixture = open(cx, SHOWN);
    let handle = fixture.handle;
    let id = discovered(cx, &mut fixture, Some(9_999));
    assert!(
        !ids(cx, handle).iter().any(|id| id == "actions"),
        "a listing under another id is not this menu's"
    );
    update(cx, handle, |shell, window, cx| {
        shell.retire_menu(window, cx)
    });
    let mut fixture2 = fixture;
    let id2 = discovered(cx, &mut fixture2, None);
    assert_ne!(id, id2);
    assert!(ids(cx, handle).iter().any(|id| id == "actions"));
    update(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().path = "/elsewhere".into();
    });
    fire(cx, handle, "actions", "action:resize.toml");
    let moved = update(cx, handle, |shell, _, _| {
        shell.locale.catalog.tr("menu.refusal.moved")
    });
    assert_eq!(
        status(cx, handle),
        moved,
        "the moved capture says the identity refusal"
    );
    let mut sent = Vec::new();
    while let Some(line) = (fixture2.requests)() {
        sent.push(line);
    }
    assert!(
        sent.iter()
            .all(|line| !line.contains("\"op\":\"run\"") && !line.contains("actions:action")),
        "a capture whose folder moved validates and runs nothing: {sent:?}"
    );
}

#[gpui::test]
fn m9_integ_open_in_agent_runs_once_in_the_captured_folder(cx: &mut TestAppContext) {
    let fixture = open(cx, SHOWN);
    let handle = fixture.handle;
    let launched = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    let log = launched.clone();
    update(cx, handle, |shell, window, cx| {
        shell.menu.agent = Some(Rc::new(move |folder: &std::path::Path| {
            log.borrow_mut().push(folder.to_path_buf());
            Ok(())
        }));
        shell.open_menu(0, None, (10., 10.), window, cx);
    });
    let shown = ids(cx, handle);
    let terminal = shown
        .iter()
        .position(|id| id == "openTerminal")
        .expect("Open in Terminal");
    assert_eq!(
        shown.get(terminal + 1).map(String::as_str),
        Some("openAgent"),
        "{shown:?}"
    );
    fire(cx, handle, "openAgent", "");
    fire(cx, handle, "openAgent", "");
    assert_eq!(
        *launched.borrow(),
        [PathBuf::from("/files")],
        "one launch, in the captured folder"
    );
    assert!(!update(cx, handle, |shell, _, _| shell.menu_opened()));
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("shift-a");
    assert_eq!(
        *launched.borrow(),
        [PathBuf::from("/files"), PathBuf::from("/files")],
        "A opens the agent in the folder with no menu"
    );
}

#[gpui::test]
fn m9_integ_open_in_agent_ships_off(cx: &mut TestAppContext) {
    let fixture = open(cx, "{}");
    let handle = fixture.handle;
    update(cx, handle, |shell, window, cx| {
        shell.open_menu(0, None, (10., 10.), window, cx)
    });
    let shown = ids(cx, handle);
    assert!(shown.iter().any(|id| id == "openTerminal"));
    assert!(!shown.iter().any(|id| id == "openAgent"), "{shown:?}");
}

#[path = "m9_g2rows_18_tests.rs"]
mod m9_g2rows_18_tests;
