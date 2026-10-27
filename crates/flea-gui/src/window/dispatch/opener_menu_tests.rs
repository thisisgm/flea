//! The menu's Open uses its validated paths through the production Shell opener.
use super::opener_tests::{home, launched, quit, shell};
use super::test_shell::{asked, asked_on, land, land_on, row, set_cursor};
use super::Order;
use crate::g2::mn1::join::{Captured, Chosen};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::rc::Rc;

// The menu's Open chosen on the left pane's cursor row with these marked paths; answers whether the opener claimed it.
fn menu_open(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    cursor: usize,
    paths: &[&str],
) -> bool {
    menu_open_on(shell, cx, 0, cursor, paths, None)
}

fn menu_open_on(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    cursor: usize,
    paths: &[&str],
    rows: Option<&[usize]>,
) -> bool {
    let claimed = Rc::new(std::cell::Cell::new(false));
    let chosen = Chosen {
        captured: Captured {
            side,
            listing: shell.read_with(cx, |shell, _| {
                shell.panes[side].as_ref().unwrap().pane.held.listing
            }),
            rows: rows
                .map(<[usize]>::to_vec)
                .unwrap_or_else(|| (cursor..cursor + paths.len()).collect()),
            cursor,
            path: "/files".into(),
            identity: String::new(),
            id: 7,
            ready: true,
            kinds: Vec::new(),
            background: false,
        },
        action: "open".into(),
        leaf: String::new(),
        paths: paths.iter().map(|path| path.to_string()).collect(),
        claimed: claimed.clone(),
    };
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.menu.context.row_is_file = true;
            shell.dispatch(Order::Menu(chosen), window, cx);
        })
    });
    cx.run_until_parked();
    claimed.get()
}

#[gpui::test]
fn the_menu_open_on_a_file_runs_the_opener(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    assert!(
        menu_open(&shell, cx, 1, &["/files/notes.txt"]),
        "the opener claims the choice, so no refusal follows"
    );
    assert_eq!(launched(), ["/files/notes.txt"]);
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_on_an_archive_hands_it_to_quick_look(cx: &mut TestAppContext) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 2);
    assert!(menu_open(&shell, cx, 2, &["/files/backup.tar.zst"]));
    assert_eq!(
        launched(),
        Vec::<String>::new(),
        "an archive never reaches flea --open"
    );
    assert_eq!(actions.borrow().as_slice(), ["preview"]);
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_menu_open_uses_every_captured_path_including_offscreen_marks(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("f5");
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (0..count)
            .map(|i| row(&format!("filler-{i}.txt"), false))
            .collect(),
        1000,
        2,
    );
    assert!(shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        !host.pane.busy() && host.pane.held.row(900).is_none() && host.pane.held.row(901).is_none()
    }));
    assert!(menu_open_on(
        &shell,
        cx,
        0,
        1,
        &[
            "/files/one.png",
            "/files/two.png",
            "/files/three.png",
            "/files/notes.txt"
        ],
        Some(&[1, 2, 900, 901])
    ));
    assert_eq!(
        launched(),
        ["/files/one.png /files/two.png /files/three.png /files/notes.txt"]
    );
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_in_a_busy_pane_launches_nothing(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("f5");
    assert!(shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()));
    assert!(menu_open(&shell, cx, 1, &["/files/notes.txt"]));
    assert_eq!(
        launched(),
        Vec::<String>::new(),
        "a busy pane hands nothing to flea --open"
    );
    land(&shell, cx, "/files", home(), 2);
    quit(&shell, cx);
}

#[gpui::test]
fn the_menu_open_on_the_unfocused_dual_pane_opens_that_panes_row(cx: &mut TestAppContext) {
    let (shell, cx, _, _keep) = shell(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.set_view(crate::window::startup::View::Dual, window, cx)
        })
    });
    let right = asked_on(&shell, cx, 1);
    land_on(
        &shell,
        cx,
        1,
        &right,
        vec![row("beta", true), row("report.pdf", false)],
        2,
        1,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.focus_side),
        0,
        "the right pane is unfocused when its menu opens"
    );
    // A right click on the right pane's file row; the left cursor stays on folder alpha, which a left-pane Open would enter.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_menu(1, Some(1), (10., 10.), window, cx)
        })
    });
    let file = format!("{}/report.pdf", right.trim_end_matches('/'));
    assert!(menu_open_on(&shell, cx, 1, 1, &[&file], None));
    assert_eq!(launched(), [file], "Open ran on the menu's pane");
    assert_eq!(asked(&shell, cx), "/files", "the left pane did not move");
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_menu_open_keeps_its_single_validated_target_when_cursor_is_an_archive(
    cx: &mut TestAppContext,
) {
    let (shell, cx, (actions, _), _keep) = shell(cx);
    set_cursor(&shell, cx, 2);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    assert!(menu_open_on(
        &shell,
        cx,
        0,
        2,
        &["/files/notes.txt"],
        Some(&[1])
    ));
    assert_eq!(launched(), ["/files/notes.txt"]);
    assert!(
        !actions.borrow().contains(&"preview"),
        "the unrelated cursor archive cannot replace the validated target"
    );
    quit(&shell, cx);
}
