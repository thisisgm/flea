//! Mounted input regressions for the listing's open and Escape paths.
use super::test_shell::{land_on, open_root, row, sent, status};
use crate::state::Shell;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static LAUNCHED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    LAUNCHED.with(|calls| calls.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

fn launches() -> Vec<Vec<String>> {
    LAUNCHED.with(|calls| std::mem::take(&mut *calls.borrow_mut()))
}

fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                crate::backend::Backend::test_reply_line(line),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
}

fn start(
    cx: &mut TestAppContext,
    directory: bool,
    image: bool,
) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    let first = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .swap
            .pending
            .as_ref()
            .unwrap()
            .first
    });
    let mut rows: Vec<_> = (0..first.min(1000))
        .map(|index| row(&format!("file-{index}.txt"), false))
        .collect();
    rows[0] = row(if image { "z.png" } else { "a.txt" }, directory);
    rows[1] = row(if image { "a.png" } else { "b.txt" }, false);
    if image {
        rows[0].icon = "image-x-generic".into();
        rows[1].icon = "image-x-generic".into();
    }
    land_on(&shell, cx, 0, "/files", rows, 1000, 1);
    shell.update(cx, |shell, _| shell.opener[0].launch = record);
    launches();
    sent();
    (shell, cx)
}

#[gpui::test]
fn pb_rest_2_enter_resolves_selected_rows_outside_the_viewport(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, false, false);
    cx.simulate_keystrokes("v down v");
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected.push(900)
    });
    cx.simulate_keystrokes("enter");
    assert!(
        launches().is_empty(),
        "no partial launch before all selected paths arrive"
    );
    let line = sent()
        .into_iter()
        .find(|line| line.contains("\"op\":\"snapshot\""))
        .expect("Enter asks the backend for every selected row");
    let doc = flea::jsondoc::parse(&line).unwrap();
    let expected = flea::jsondoc::parse("[0,1,900]").unwrap();
    assert_eq!(doc.get("rows"), Some(&expected));
    let id = flea::json::field_usize(&line, "id").unwrap();
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"count":3,"kinds":[33188,33188,33188]}}"#
        ),
    );
    assert!(sent()
        .iter()
        .any(|line| line.contains("\"op\":\"validate\"")));
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"open","id":{id},"ok":true,"paths":["/files/a.txt","/files/b.txt","/files/far.txt"]}}"#
        ),
    );
    let calls = launches();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        &calls[0][1..],
        [
            "--open-selection",
            "/files/a.txt",
            "/files/b.txt",
            "/files/far.txt"
        ]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn s1_pb_rest_2_escape_clears_a_standing_filter_before_the_marks(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, false, false);
    cx.simulate_keystrokes("/ a");
    reply(
        &shell,
        cx,
        r#"{"t":"filtered","id":1,"query":"a","n":1000,"rows":[0,1],"ms":0.01,"listing":1}"#,
    );
    // A cursor key hands the keyboard to the list, whose Ctrl+A marks the matches (EY-115: the typed line keeps it).
    cx.simulate_keystrokes("home ctrl-a escape");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, vec![0, 1], "Focus.js:187 keeps the marks");
        assert!(!host.filter.owner.up(), "Escape closes the standing filter");
        assert!(host.filter.shown.is_none(), "every row is back");
    });
    cx.simulate_keystrokes("escape");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.selected.is_empty(), "the next Escape clears the marks");
        assert!(!host.filter.owner.up());
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_2_enter_refuses_a_folder_in_the_selection_before_launch(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, true, false);
    cx.simulate_keystrokes("v down v enter");
    assert!(launches().is_empty());
    assert_eq!(
        status(&shell, cx),
        "A folder can’t be opened together with files."
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_2_image_open_resolves_the_folder_gallery(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, false, true);
    cx.simulate_keystrokes("down enter");
    assert!(launches().is_empty());
    let requests = sent();
    assert!(
        requests.iter().any(|line| line.contains("\"images\":1")),
        "{requests:?}"
    );
    reply(
        &shell,
        cx,
        r#"{"t":"paths","paths":["/files/a.png","/files/z.png"]}"#,
    );
    let calls = launches();
    assert_eq!(calls.len(), 1);
    assert_eq!(&calls[0][1..], ["--open", "/files/a.png", "/files/z.png"]);
    assert!(!shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .nav
        .gate
        .paths_pending));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_2_stale_selection_reply_never_opens_a_departed_folder(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, false, false);
    cx.simulate_keystrokes("v down v enter");
    let line = sent()
        .into_iter()
        .find(|line| line.contains("\"op\":\"snapshot\""))
        .unwrap();
    let id = flea::json::field_usize(&line, "id").unwrap();
    cx.simulate_keystrokes("backspace");
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"count":2,"kinds":[33188,33188]}}"#
        ),
    );
    assert!(launches().is_empty());
    assert!(!sent()
        .iter()
        .any(|line| line.contains("\"op\":\"validate\"")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_2_standing_filter_ctrl_d_and_ctrl_u_move_half_a_page(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, false, false);
    cx.simulate_keystrokes("/ a");
    let rows = (0..40)
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"filtered","id":1,"query":"a","n":1000,"rows":[{rows}],"ms":0.01,"listing":1}}"#
        ),
    );
    cx.simulate_keystrokes("home");
    let page = shell
        .read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().pane.visible_rows() / 2
        })
        .max(1);
    cx.simulate_keystrokes("ctrl-d");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        page
    );
    cx.simulate_keystrokes("ctrl-u");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        0
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_2_empty_listed_path_keeps_the_requested_directory(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, true, false);
    cx.simulate_keystrokes("enter");
    let requested = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    });
    assert_eq!(requested, "/files/a.txt");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            for answer in [
                flea::protocol::Reply::Listed {
                    path: String::new(),
                    total: 1,
                },
                flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 2,
                    rows: vec![row("child.txt", false)],
                },
            ] {
                shell.receive(0, crate::backend::Event::Reply(answer), window, cx);
            }
        })
    });
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(400));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.pane.busy());
        assert_eq!(host.pane.path(), requested);
        assert_eq!(host.pane.held.row(0).unwrap().name, "child.txt");
        assert_eq!(shell.path_labels[0].as_ref(), requested);
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_2_enter_opens_reverse_order_marks(cx: &mut TestAppContext) {
    let (shell, cx) = start(cx, false, false);
    cx.simulate_keystrokes("down v up v");
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1, 0]
    });
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, vec![1, 0]);
    });
    cx.simulate_keystrokes("enter");
    assert!(launches().is_empty());
    let line = sent()
        .into_iter()
        .find(|line| line.contains("\"op\":\"snapshot\""))
        .expect("Enter asks the backend for every selected row");
    let doc = flea::jsondoc::parse(&line).unwrap();
    let expected = flea::jsondoc::parse("[0,1]").unwrap();
    assert_eq!(doc.get("rows"), Some(&expected));
    let id = flea::json::field_usize(&line, "id").unwrap();
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"count":2,"kinds":[33188,33188]}}"#
        ),
    );
    assert!(
        sent()
            .iter()
            .any(|line| line.contains("\"op\":\"validate\"")),
        "unchanged reverse-order marks must pass the snapshot reply guard"
    );
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"open","id":{id},"ok":true,"paths":["/files/a.txt","/files/b.txt"]}}"#
        ),
    );
    let calls = launches();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        &calls[0][1..],
        ["--open-selection", "/files/a.txt", "/files/b.txt"]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
