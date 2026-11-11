//! Rename regressions use the real mounted field and backend operation channel.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use gpui::{
    AppContext, Context, Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window,
};
use std::rc::Rc;
use std::time::Duration;

#[path = "i50_rename_mozc_tests.rs"]
mod mozc;
#[path = "i50_rename_rail_tests.rs"]
mod rail;

type Lines = Box<dyn FnMut() -> Option<String>>;
const VIEWS: [(View, usize); 5] = [
    (View::List, 0),
    (View::Columns, 0),
    (View::Grid, 0),
    (View::Dual, 0),
    (View::Dual, 1),
];

fn ready(
    cx: &mut TestAppContext,
    view: View,
    side: usize,
) -> (Entity<Shell>, &mut VisualTestContext, [Lines; 2]) {
    let (left, left_lines) = Backend::test_operation_channel();
    let (right, right_lines) = Backend::test_operation_channel();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(left, cx);
            let mut second = shell_with(right, cx);
            shell.panes[1] = second.panes[0].take();
            for host in shell.panes.iter_mut().flatten() {
                let held = Rc::get_mut(&mut host.pane.held).unwrap();
                held.rows[0].name = "notes.txt".into();
                held.rows[0].mode = 0o100644;
            }
            shell.view = view;
            shell.focus_side = side;
            shell
        });
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        draw(cx);
    }
    input(cx, &shell, |s, w, cx| {
        assert!(s.rename_open(side, 9, w, cx));
    });
    draw(cx);
    let mut lines: [Lines; 2] = [Box::new(left_lines), Box::new(right_lines)];
    let _ = sent(&mut lines);
    (shell, cx, lines)
}

fn input<R>(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    cx.update(|window, cx| shell.update(cx, |s, cx| f(s, window, cx)))
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn close(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |s, _| s.stop_for_app_quit());
    cx.update(|w, _| w.remove_window());
    cx.run_until_parked();
}

fn mark(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    input(cx, shell, |s, w, cx| {
        s.replace_and_mark_text_in_range(None, "メモ", Some(2..2), w, cx);
    });
    draw(cx);
}

fn sent(lines: &mut [Lines; 2]) -> [Vec<String>; 2] {
    lines
        .each_mut()
        .map(|lines| std::iter::from_fn(lines).collect())
}

fn attachment(cx: &mut TestAppContext, view: View, side: usize) {
    let (shell, cx, _lines) = ready(cx, view, side);
    shell.read_with(cx, |s, _| {
        let edit = s.rename.edit.as_ref().expect("rename host exists");
        assert_eq!(edit.side, side);
        assert!(edit.line.frame.is_some(), "{view:?}/{side}: field painted");
        assert_eq!(edit.line.line.registered, edit.entry.session());
    });
    cx.simulate_input("添付");
    assert_eq!(
        shell.read_with(cx, |s, _| s
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .text
            .clone()),
        "添付.txt",
        "{view:?}/{side}: platform text reached the painted handler"
    );
    close(&shell, cx);
}

#[gpui::test]
fn i50_rename_per_view_handler_attachment_list(cx: &mut TestAppContext) {
    attachment(cx, View::List, 0);
}

#[gpui::test]
fn i50_rename_per_view_handler_attachment_columns(cx: &mut TestAppContext) {
    attachment(cx, View::Columns, 0);
}

#[gpui::test]
fn i50_rename_per_view_handler_attachment_grid(cx: &mut TestAppContext) {
    attachment(cx, View::Grid, 0);
}

#[gpui::test]
fn i50_rename_per_view_handler_attachment_dual_left(cx: &mut TestAppContext) {
    attachment(cx, View::Dual, 0);
}

#[gpui::test]
fn i50_rename_per_view_handler_attachment_dual_right(cx: &mut TestAppContext) {
    attachment(cx, View::Dual, 1);
}

#[gpui::test]
fn i50_rename_preedit_stem(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, mut lines) = ready(cx, view, side);
        input(cx, &shell, |s, w, cx| {
            assert_eq!(s.selected_text_range(false, w, cx).unwrap().range, 0..5);
        });
        mark(&shell, cx);
        input(cx, &shell, |s, w, cx| {
            let edit = s.rename.edit.as_ref().unwrap();
            assert_eq!(edit.entry.text, "メモ.txt");
            assert_eq!(edit.entry.marked, Some(0..6));
            let field = edit.line.frame.unwrap();
            assert_eq!(s.marked_text_range(w, cx), Some(0..2));
            let caret = s.bounds_for_range(2..2, field, w, cx).unwrap();
            assert!(caret.left() >= field.left() && caret.right() <= field.right());
            assert_eq!(
                (caret.top(), caret.bottom()),
                (field.top() - gpui::px(3.), field.bottom() + gpui::px(3.))
            );
        });
        assert_eq!(sent(&mut lines), [Vec::<String>::new(), Vec::new()]);
        close(&shell, cx);
    }
}

#[gpui::test]
fn i50_rename_candidate_enter_no_write(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, mut lines) = ready(cx, view, side);
        mark(&shell, cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(
            sent(&mut lines),
            [Vec::<String>::new(), Vec::new()],
            "{view:?}/{side}: candidate Enter must not send rename"
        );
        shell.read_with(cx, |s, _| {
            assert_eq!(s.rename.edit.as_ref().unwrap().entry.marked, Some(0..6))
        });
        close(&shell, cx);
    }
}

#[gpui::test]
fn i50_rename_commit_once(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, mut lines) = ready(cx, view, side);
        mark(&shell, cx);
        cx.simulate_input("覚書");
        cx.simulate_keystrokes("enter enter");
        let mut expected: [Vec<String>; 2] = [Vec::new(), Vec::new()];
        expected[side]
            .push(r#"{"c":"rename","path":"/files/notes.txt","to":"覚書.txt","menuId":9}"#.into());
        assert_eq!(
            sent(&mut lines),
            expected,
            "{view:?}/{side}: exactly one rename on the owning backend"
        );
        close(&shell, cx);
    }
}

#[gpui::test]
fn i50_rename_cancel_preserves_name(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, mut lines) = ready(cx, view, side);
        mark(&shell, cx);
        cx.simulate_keystrokes("escape");
        shell.read_with(cx, |s, _| {
            assert!(s.rename.edit.is_some(), "first Escape cancels composition")
        });
        cx.simulate_keystrokes("escape");
        shell.read_with(cx, |s, _| {
            assert!(s.rename.edit.is_none());
            assert_eq!(
                s.panes[side]
                    .as_ref()
                    .unwrap()
                    .pane
                    .held
                    .row(0)
                    .unwrap()
                    .name,
                "notes.txt"
            );
        });
        let mut expected = [Vec::<String>::new(), Vec::new()];
        expected[side].push(r#"{"c":"menuaction","op":"close","id":9}"#.into());
        assert_eq!(
            sent(&mut lines),
            expected,
            "cancel closes only the owning snapshot"
        );
        close(&shell, cx);
    }
}

#[gpui::test]
fn i50_rename_outside_commit_waits_for_preedit(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, View::List, 0);
    mark(&shell, cx);
    shell.update(cx, |s, cx| s.rename_commit(cx));
    assert_eq!(
        sent(&mut lines),
        [Vec::<String>::new(), Vec::new()],
        "outside-click commit must not write preedit"
    );
    close(&shell, cx);
}

#[gpui::test]
fn i50_rename_focus_and_session_reject_stale_preedit(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, mut lines) = ready(cx, view, side);
        mark(&shell, cx);
        let other = cx.update(|window, cx| {
            let other = cx.focus_handle();
            window.focus(&other, cx);
            other
        });
        input(cx, &shell, |s, w, cx| {
            s.replace_and_mark_text_in_range(None, "stale", None, w, cx);
            assert!(s.marked_text_range(w, cx).is_none());
            assert_eq!(s.rename.edit.as_ref().unwrap().entry.text, "メモ.txt");
            let focus = s.focus.clone();
            w.focus(&focus, cx);
            s.rename_abandon(cx);
            s.rename_open(side, 9, w, cx);
            s.replace_and_mark_text_in_range(None, "old-session", None, w, cx);
            assert_eq!(s.rename.edit.as_ref().unwrap().entry.text, "notes.txt");
        });
        drop(other);
        draw(cx);
        cx.simulate_input("current");
        shell.read_with(cx, |s, _| {
            assert_eq!(s.rename.edit.as_ref().unwrap().entry.text, "current.txt")
        });
        let mut expected = [Vec::<String>::new(), Vec::new()];
        expected[side].push(r#"{"c":"menuaction","op":"close","id":9}"#.into());
        assert_eq!(
            sent(&mut lines),
            expected,
            "retired input closes only its old snapshot"
        );
        close(&shell, cx);
    }
}

#[gpui::test]
fn i50_rename_error_keeps_draft_and_accepts_correction(cx: &mut TestAppContext) {
    for (view, side) in VIEWS {
        let (shell, cx, mut lines) = ready(cx, view, side);
        cx.simulate_input("bad/name");
        cx.simulate_keystrokes("enter");
        shell.read_with(cx, |s, _| {
            let edit = s.rename.edit.as_ref().unwrap();
            assert!(!edit.error.is_empty());
            assert_eq!(edit.entry.text, "bad/name.txt");
            assert_eq!(edit.pending, None);
        });
        assert_eq!(sent(&mut lines), [Vec::<String>::new(), Vec::new()]);
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("修正.txt");
        cx.simulate_keystrokes("enter");
        let mut expected: [Vec<String>; 2] = [Vec::new(), Vec::new()];
        expected[side]
            .push(r#"{"c":"rename","path":"/files/notes.txt","to":"修正.txt","menuId":9}"#.into());
        assert_eq!(sent(&mut lines), expected);
        close(&shell, cx);
    }
}
