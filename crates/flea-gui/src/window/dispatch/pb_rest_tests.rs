//! Mounted input regressions for the remaining parity routes.
use super::search_open_tests::answer;
use super::test_shell::{asked, land, open_root, open_root_with, row};
use crate::state::Shell;
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static OPENED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    OPENED.with(|opened| opened.borrow_mut().push(argv[2..].join(" ")));
    (Some(0), String::new())
}

fn prepare_open(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    OPENED.with(|opened| opened.borrow_mut().clear());
    shell.update(cx, |shell, _| shell.opener[0].launch = record);
}

fn choose(cx: &mut VisualTestContext, name: &str) {
    cx.simulate_keystrokes("?");
    cx.run_until_parked();
    let keys = name.chars().map(String::from).collect::<Vec<_>>().join(" ");
    cx.simulate_keystrokes(&keys);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn pb_rest_sheet_search_starts_the_mounted_search(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    choose(cx, "find");
    shell.read_with(cx, |shell, cx| {
        assert!(
            shell.settings.sheet.is_none(),
            "the chosen sheet closed: {:?}",
            shell
                .settings
                .sheet
                .as_ref()
                .map(|view| (&view.read(cx).sheet.query, &view.read(cx).sheet.ranked))
        );
        assert!(
            shell.panes[0].as_ref().unwrap().search.owner.active(),
            "Search entered its input state"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_sheet_reveal_uses_the_search_result(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    cx.simulate_keystrokes("f s");
    answer(&shell, cx, 1, vec![row("sub/notes.txt", false)]);
    cx.simulate_keystrokes("enter");
    choose(cx, "reveal");
    assert_eq!(
        asked(&shell, cx),
        "/files/sub",
        "Reveal opens the result's containing folder"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_open_mode_single_opens_on_release_and_never_arms_rename(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(
        cx,
        r#""view":"list","openMode":"single","clickRename":true"#,
        None,
    );
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    prepare_open(&shell, cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let at = point(
        frame.bounds.left() + px(60.),
        frame.bounds.top() + px(frame.row / 2.),
    );
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(OPENED.with(|opened| opened.take()), ["/files/notes.txt"]);
    shell.read_with(cx, |shell, _| {
        assert!(shell
            .rename
            .slow
            .as_ref()
            .is_none_or(|slow| !slow.pending()))
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_rest_neighbour_double_click_runs_the_file_opener(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    prepare_open(&shell, cx);
    cx.simulate_keystrokes("ctrl-2");
    cx.run_until_parked();
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 2);
    answer_columns(&shell, cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    super::preview_tests::draw(cx);
    let at = shell.read_with(cx, |shell, _| {
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns
            .painters
            .get(crate::columns::geometry::Slot::Parent)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        assert!(
            drawn.view().enabled,
            "the neighbour is settled: {:?}",
            drawn.view()
        );
        assert!(!shell.panes[0].as_ref().unwrap().nav.gate.owned());
        point(drawn.bounds.left() + px(60.), drawn.bounds.top() + px(15.))
    });
    cx.simulate_mouse_move(at, None, Modifiers::none());
    super::preview_tests::draw(cx);
    for click_count in 1..=2 {
        cx.simulate_event(gpui::MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        assert_eq!(
            asked(&shell, cx),
            "/",
            "the first tap lists the clicked file's own directory"
        );
        if click_count == 1 {
            assert!(OPENED.with(|opened| opened.borrow().is_empty()));
            shell.read_with(cx, |shell, _| {
                assert_eq!(
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .nav
                        .pending_select
                        .as_deref(),
                    Some("/outside.txt"),
                    "a neighbour file's first tap selects that file"
                );
            });
        }
    }
    cx.run_until_parked();
    let state = shell.read_with(cx, |shell, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let columns = host.columns.as_ref().unwrap();
        let paint = columns
            .painters
            .get(crate::columns::geometry::Slot::Parent)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        (
            shell.view,
            shell.shown_view(),
            shell.input_blocked(cx),
            host.pane.busy(),
            std::rc::Rc::ptr_eq(&drawn.tracks, columns.area.tracks()),
            drawn.row_at(0),
        )
    });
    assert_eq!(
        OPENED.with(|opened| opened.take()),
        ["/outside.txt"],
        "{state:?}"
    );
    assert_eq!(
        asked(&shell, cx),
        "/",
        "the double click keeps the directory selected by its first tap"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

pub(super) fn answer_columns(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    use flea::jsondoc::Json;
    for request in super::test_shell::sent() {
        let Ok(mut document) = flea::jsondoc::parse(&request) else {
            continue;
        };
        if document.get("c").and_then(Json::as_str) != Some("peek") {
            continue;
        }
        let parent = document.get("path").and_then(Json::as_str) == Some("/");
        let rows = if parent {
            vec![("outside.txt", false), ("files", true)]
        } else {
            Vec::new()
        };
        let Json::Obj(fields) = &mut document else {
            continue;
        };
        fields.push(("t".into(), Json::Str("peeked".into())));
        fields.push(("n".into(), Json::Num(rows.len().to_string())));
        fields.push(("failed".into(), Json::Bool(false)));
        fields.push(("mode".into(), Json::Num("0".into())));
        fields.push((
            "rows".into(),
            Json::Arr(
                rows.into_iter()
                    .map(|(name, dir)| {
                        Json::Obj(vec![
                            ("n".into(), Json::Str(name.into())),
                            ("d".into(), Json::Bool(dir)),
                        ])
                    })
                    .collect(),
            ),
        ));
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.receive(0, crate::backend::Event::Peeked(document), window, cx)
            })
        });
    }
    super::preview_tests::draw(cx);
}
