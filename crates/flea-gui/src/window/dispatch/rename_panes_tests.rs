//! The rename editor against the list's viewport, and bulk renames out in both panes at once.
use super::{
    rename_tests::{close, draw, notice, sent, Lines},
    test_shell::{land_on, open, row, set_cursor},
};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
    window::{pane::PaneHost, startup::View, u07_tests::press},
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn editing(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| {
        shell
            .rename
            .edit
            .as_ref()
            .is_some_and(|edit| edit.entry.editing)
    })
}

// A point on the name column, the list's top edge, the row pitch and the field's inset in its row, as last drawn.
fn viewport(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (f32, f32, f32, f32) {
    shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        let bounds = list.drawn.bounds;
        let x = f32::from(bounds.origin.x) + list.drawn.layout.name.x + 4.;
        let metrics = list.drawn.metrics;
        let inset = (metrics.row - metrics.slot) / 2.;
        (x, f32::from(bounds.origin.y), metrics.row, inset)
    })
}

fn scroll(shell: &Entity<Shell>, cx: &mut VisualTestContext, y: f32) {
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().pane.scroll_to(y)
    });
    draw(cx);
}

#[gpui::test]
fn a_row_scrolled_out_drops_its_edit_and_a_partly_shown_one_is_clipped(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    let first = shell.read_with(cx, |shell, _| {
        let pending = shell.panes[0].as_ref().unwrap().pane.swap.pending.as_ref();
        pending.unwrap().first
    });
    let rows = (0..first)
        .map(|at| row(&format!("note{at}.txt"), false))
        .collect();
    land_on(&shell, cx, 0, "/files", rows, first + 50, 1);
    set_cursor(&shell, cx, 2);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut lines: Lines = Box::new(lines);
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert!(editing(&shell, cx), "F2 opens the editor");
    let (x, top, pitch, inset) = viewport(&shell, cx);
    // Row 2's field starts four pixels above the list: the editor stays, and its part above the list takes no press.
    scroll(&shell, cx, pitch * 2. + inset + 4.);
    assert!(editing(&shell, cx), "a partly shown row keeps its editor");
    press(cx, &format!("{x} {}", top - 2.), 1);
    assert!(
        !editing(&shell, cx),
        "a press above the list is outside the clipped editor, so it commits"
    );
    let _ = sent(&mut lines);
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert!(editing(&shell, cx), "F2 opens the editor again");
    cx.simulate_input("renamed");
    draw(cx);
    assert!(
        shell.read_with(cx, |shell, _| {
            let edit = shell.rename.edit.as_ref().unwrap();
            edit.entry.text != edit.name
        }),
        "the draft must differ so an accidental commit sends a rename"
    );
    scroll(&shell, cx, pitch * 3. + 1.);
    assert!(
        shell.read_with(cx, |shell, _| shell.rename.edit.is_none()),
        "a row wholly out of the list drops its edit"
    );
    assert_eq!(sent(&mut lines), Vec::<String>::new(), "with no commit");
    close(&shell, cx);
}

fn receive_on(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, event: BackendEvent) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(side, event, window, cx)));
    cx.run_until_parked();
}

fn bulk_send(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, paths: [&str; 2]) {
    shell.update(cx, |shell, cx| {
        shell.focus_pane(side, cx);
        let host = shell.panes[side].as_mut().unwrap();
        host.selected = vec![1, 2];
        host.input.selection.changed(&host.selected);
    });
    cx.simulate_keystrokes("f2");
    receive_on(
        shell,
        cx,
        side,
        BackendEvent::Paths(paths.iter().map(|path| path.to_string()).collect()),
    );
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "side {side}'s card opened"
    );
    draw(cx);
    cx.simulate_input("e");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("x");
    cx.simulate_keystrokes("enter");
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
}

#[gpui::test]
fn each_pane_keeps_its_own_bulk_rename_done_line(cx: &mut TestAppContext) {
    let (shell, cx) = super::test_shell::open_root(cx, "default");
    let home = || {
        vec![
            row("alpha", true),
            row("beta", true),
            row("notes.txt", false),
        ]
    };
    land_on(&shell, cx, 0, "/files", home(), 3, 1);
    let (left, left_lines) = Backend::test_lines();
    let (right, right_lines) = Backend::test_lines();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.panes[0].as_mut().unwrap().backend = Some(left);
            let mut host = PaneHost::new("/other".into(), None, 620., &shell.locale);
            host.backend = Some(right);
            host.pane.use_executor(cx.background_executor().clone());
            shell.panes[1] = Some(host);
            shell.lifecycle.attach(1);
            shell.set_view(View::Dual, window, cx);
        })
    });
    land_on(&shell, cx, 1, "/other", home(), 3, 3);
    let (mut left_lines, mut right_lines): (Lines, Lines) =
        (Box::new(left_lines), Box::new(right_lines));
    bulk_send(&shell, cx, 0, ["/files/beta", "/files/notes.txt"]);
    assert!(
        sent(&mut left_lines)
            .iter()
            .any(|line| line.starts_with(r#"{"c":"bulkrename""#)),
        "the left request is out"
    );
    bulk_send(&shell, cx, 1, ["/other/beta", "/other/notes.txt"]);
    assert!(
        sent(&mut right_lines)
            .iter()
            .any(|line| line.starts_with(r#"{"c":"bulkrename""#)),
        "the right request is out"
    );
    let done = shell.read_with(cx, |shell, _| {
        flea::ui_logic_op1::lines::bulk_done_line(&shell.locale.catalog, "default", 2)
    });
    let marked = |shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize| {
        shell.read_with(cx, |shell, _| {
            shell.panes[side].as_ref().unwrap().selected.len()
        })
    };
    // The right card's Close never takes the left pane's answer.
    receive_on(
        &shell,
        cx,
        0,
        BackendEvent::Producer(r#"{"t":"bulkrenamed","ok":2,"unchanged":0}"#.into()),
    );
    assert_eq!(notice(&shell, cx), done);
    assert_eq!(marked(&shell, cx, 0), 0, "the left marks clear");
    receive_on(
        &shell,
        cx,
        1,
        BackendEvent::Producer(r#"{"t":"bulkrenamed","ok":2,"unchanged":0}"#.into()),
    );
    assert_eq!(marked(&shell, cx, 1), 0, "the right marks clear");
    close(&shell, cx);
}
