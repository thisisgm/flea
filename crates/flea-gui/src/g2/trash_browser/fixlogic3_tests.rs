//! EY-533 and EY-534: Escape over a finished note closes Trash, and opening Trash drops the selection of the pane under it.
use crate::g2::trash_browser::join_tests::{answer, draw, opened, root, shell};
use crate::{list::bench::real_cx, window::pane::PaneHost};
use flea::protocol::Reply;

// A Restore that lands, so the bar carries a plain note rather than an error.
fn restored(cx: &mut gpui::TestAppContext, window: gpui::WindowHandle<crate::window::root::Root>) {
    shell(cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.menu_action(Some("restoreTrashSelection"), cx)
        });
        shell.flush_trash(window, cx);
    });
    answer(
        cx,
        window,
        "restore",
        r#","done":1,"failed":0,"failures":[]"#,
    );
    draw(cx, window);
}

fn note(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::window::root::Root>,
) -> (String, bool) {
    shell(cx, window, |shell, _, cx| {
        let line = shell.trash.as_ref().unwrap().messages.read(cx);
        let drawn = line.drawn();
        (drawn.primary.to_string(), drawn.error)
    })
}

#[test]
fn fixlogic3_ey533_escape_over_a_finished_note_closes_trash() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    restored(&mut cx, window);
    let (said, error) = note(&mut cx, window);
    assert!(!said.is_empty() && !error, "a plain note stands: {said}");
    cx.simulate_keystrokes(window.into(), "escape");
    draw(&mut cx, window);
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "StatusBar.escapePressed takes only an error, so Escape closes Trash as TrashView.qml:294 does"
    );
}

#[test]
fn fixlogic3_ey533_escape_over_an_error_dismisses_it_and_trash_stays() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.menu_action(Some("restoreTrashSelection"), cx)
        });
        shell.flush_trash(window, cx);
    });
    answer(
        &mut cx,
        window,
        "restore",
        r#","done":0,"failed":1,"failures":[{"uri":"trash:///alpha","name":"alpha.txt","error":"already exists"}]"#,
    );
    draw(&mut cx, window);
    assert!(note(&mut cx, window).1, "a failed restore is an error");
    cx.simulate_keystrokes(window.into(), "escape");
    draw(&mut cx, window);
    assert!(
        shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "the first Escape dismisses the error and Trash stays"
    );
    assert!(note(&mut cx, window).0.is_empty(), "the error is gone");
    cx.simulate_keystrokes(window.into(), "escape");
    draw(&mut cx, window);
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "the next Escape closes Trash"
    );
}

fn mark(cx: &mut gpui::TestAppContext, window: gpui::WindowHandle<crate::window::root::Root>) {
    shell(cx, window, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.input.selection.only(&mut host.selected, 0);
    });
    draw(cx, window);
}

fn marks(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::window::root::Root>,
) -> Vec<usize> {
    shell(cx, window, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    })
}

#[test]
fn fixlogic3_ey534_opening_trash_drops_the_selection_of_the_pane_under_it() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    mark(&mut cx, window);
    assert_eq!(marks(&mut cx, window), [0], "one mark before Trash opens");
    opened(&mut cx, window);
    assert!(
        marks(&mut cx, window).is_empty(),
        "TrashHost.qml open() clears the pane's selection as Trash opens"
    );
    shell(&mut cx, window, |shell, window, cx| {
        shell.drop_trash(0, window, cx)
    });
    draw(&mut cx, window);
    assert!(
        !shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
        "Trash closed"
    );
    assert!(
        marks(&mut cx, window).is_empty(),
        "leaving Trash reads the item count"
    );
}

#[test]
fn fixlogic3_ey534_empty_trash_and_restore_all_open_trash_the_same_way() {
    for action in ["emptyTrash", "restoreAll"] {
        let mut cx = real_cx();
        let window = root(&mut cx);
        mark(&mut cx, window);
        shell(&mut cx, window, |shell, window, cx| {
            shell.mount_trash(window, cx);
            assert!(shell.trash_action(action, window, cx));
        });
        draw(&mut cx, window);
        assert!(
            marks(&mut cx, window).is_empty(),
            "{action} opens Trash through TrashHost.qml open(), which clears the selection"
        );
    }
}

#[test]
fn fixlogic3_ey534_an_action_into_an_open_trash_keeps_the_selection() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    mark(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        assert!(shell.trash_action("openTrash", window, cx));
    });
    draw(&mut cx, window);
    assert_eq!(
        marks(&mut cx, window),
        [0],
        "TrashHost.qml open() returns before clearSelection while Trash is open"
    );
}

#[test]
fn fixlogic3_ey534_a_selection_action_into_a_closed_trash_opens_it_the_same_way() {
    for action in ["restoreTrashSelection", "deleteTrashSelection"] {
        let mut cx = real_cx();
        let window = root(&mut cx);
        mark(&mut cx, window);
        shell(&mut cx, window, |shell, window, cx| {
            shell.mount_trash(window, cx);
            assert!(shell.trash_action(action, window, cx));
        });
        draw(&mut cx, window);
        assert!(
            shell(&mut cx, window, |shell, _, cx| shell.trash_opened(cx)),
            "TrashHost.qml action() opens a closed Trash for {action}"
        );
        assert!(
            marks(&mut cx, window).is_empty(),
            "{action} opens Trash through open(), which clears the selection"
        );
    }
}

#[test]
fn fixlogic3_ey534_trash_over_the_right_pane_leaves_the_left_marks() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    mark(&mut cx, window);
    shell(&mut cx, window, |shell, _, _| {
        let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
        right
            .pane
            .receive(Reply::Listed {
                path: "/right".into(),
                total: 8,
            })
            .unwrap();
        right.input.selection.only(&mut right.selected, 2);
        shell.panes[1] = Some(right);
        shell.focus_side = 1;
    });
    opened(&mut cx, window);
    let right = shell(&mut cx, window, |shell, _, _| {
        shell.panes[1].as_ref().unwrap().selected.clone()
    });
    assert!(right.is_empty(), "the pane under Trash drops its marks");
    assert_eq!(
        marks(&mut cx, window),
        [0],
        "the other pane keeps its marks"
    );
}
