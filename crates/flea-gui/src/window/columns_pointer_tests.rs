use super::host;
use crate::{backend::Event, columns::geometry::Slot};
use gpui::{point, px, MouseButton, MouseDownEvent};
use std::rc::Rc;

#[test]
fn vf_more_waiting_child_refuses_the_previously_painted_folder() {
    for button in [MouseButton::Left, MouseButton::Right] {
        let mut cx = host::context();
        let (window, _requests) = host::open(&mut cx, "/rows");
        host::change(&mut cx, window, |shell, window, cx| {
            host::peek(shell, "/rows/d01", &[("inside-A", true)], false, window, cx);
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            assert!(shell.panes[0]
                .as_mut()
                .unwrap()
                .pane
                .move_cursor(crate::list::pane::Move::End));
            shell.sync_columns(window, cx);
        });
        host::advance(&mut cx, window, std::time::Duration::from_millis(12));
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            let before = shell.panes[0].as_ref().unwrap().nav.counters.lists;
            click(shell, Slot::Child, 15., button, window, cx);
            let pane = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                pane.pane.place(),
                "/rows",
                "the waiting child must not navigate through its old row"
            );
            assert_eq!(pane.nav.counters.lists, before);
            assert!(pane.nav.menus.background.target.is_empty());
        });
        host::change(&mut cx, window, |shell, window, cx| {
            host::peek(shell, "/rows/d02", &[("inside-B", true)], false, window, cx);
        });
        host::frame(&mut cx, window);
        if button == MouseButton::Left {
            host::change(&mut cx, window, |shell, window, cx| {
                click(shell, Slot::Child, 15., button, window, cx);
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.place(),
                    "/rows/d02/inside-B"
                );
            });
        }
    }
}

fn click(
    shell: &mut crate::state::Shell,
    slot: Slot,
    y: f32,
    button: MouseButton,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::state::Shell>,
) {
    let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
    let paint = columns.painters.get(slot).unwrap().borrow();
    let drawn = paint.drawn.as_ref().unwrap();
    let position = point(
        drawn.bounds.origin.x + drawn.bounds.size.width / 2.,
        drawn.bounds.origin.y + px(y),
    );
    drop(paint);
    shell.columns_pointer(
        0,
        &MouseDownEvent {
            position,
            button,
            click_count: 1,
            ..Default::default()
        },
        window,
        cx,
    );
}

#[test]
fn ey379_neighbour_row_and_background_keep_listing_cursor_marks_and_tracks() {
    for background in [false, true] {
        let mut cx = host::context();
        let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, window, |shell, window, cx| {
            host::peek(
                shell,
                "/srv/a/b",
                &[("c", true), ("other.txt", false)],
                false,
                window,
                cx,
            );
            host::settle(shell, window, cx);
            shell.panes[0].as_mut().unwrap().selected = vec![1, 2];
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            let pane = shell.panes[0].as_ref().unwrap();
            let before = (
                pane.path.clone(),
                pane.pane.cursor,
                pane.selected.clone(),
                pane.pane.held.listing,
            );
            let tracks = pane.columns.as_ref().unwrap().area.tracks().clone();
            click(
                shell,
                Slot::Parent,
                if background { 300. } else { 45. },
                MouseButton::Right,
                window,
                cx,
            );
            let pane = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                (
                    pane.path.clone(),
                    pane.pane.cursor,
                    pane.selected.clone(),
                    pane.pane.held.listing
                ),
                before
            );
            assert_eq!(
                pane.nav.counters.lists, 0,
                "right click must not list another folder"
            );
            assert!(Rc::ptr_eq(
                pane.columns.as_ref().unwrap().area.tracks(),
                &tracks
            ));
            let capture = shell
                .menu
                .captured
                .as_ref()
                .expect("the clicked track owns a menu");
            assert_eq!(capture.path, "/srv/a/b");
            assert_eq!(capture.background, background);
            shell.retire_menu(window, cx);
            let pane = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                (
                    pane.path.clone(),
                    pane.pane.cursor,
                    pane.selected.clone(),
                    pane.pane.held.listing
                ),
                before
            );
        });
    }
}

#[test]
fn ey379_parent_active_and_child_backgrounds_open_at_the_pointer_without_navigation() {
    for (slot, path) in [
        (Slot::Parent, "/srv/a/b"),
        (Slot::Active, "/srv/a/b/c"),
        (Slot::Child, "/srv/a/b/c/d01"),
    ] {
        let mut cx = host::context();
        let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
        host::change(&mut cx, window, |shell, window, cx| {
            host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
            host::peek(shell, "/srv/a/b/c/d01", &[], false, window, cx);
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            click(shell, slot, 300., MouseButton::Right, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.place(), "/srv/a/b/c");
            assert_eq!(host.nav.counters.lists, 0);
            let captured = shell.menu.captured.as_ref().unwrap();
            assert_eq!(captured.path, path);
            assert!(captured.background);
        });
    }
}

#[test]
fn u09_live_columns_failed_background_and_busy_or_blank_clicks_never_open_a_menu() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().nav.gate.editing = true;
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        let host = shell.panes[0].as_mut().unwrap();
        assert!(host.nav.menus.background.target.is_empty());
        assert_eq!(host.nav.counters.lists, 0);
        host.nav.gate.editing = false;
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        assert_eq!(
            shell.panes[0].as_ref().unwrap().nav.counters.lists,
            0,
            "right click must not navigate"
        );
        shell.receive(
            0,
            Event::Refused {
                operation: "read".into(),
                path: "/srv/a/b".into(),
                message: "Permission denied".into(),
                mode: 0,
            },
            window,
            cx,
        );
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.nav.menus.background.target.is_empty());
        assert!(host.nav.background_at.is_none());
    });
    let (root, _requests) = host::open(&mut cx, "/");
    host::change(&mut cx, root, host::settle);
    host::frame(&mut cx, root);
    host::change(&mut cx, root, |shell, window, cx| {
        click(shell, Slot::Parent, 300., MouseButton::Right, window, cx);
        assert_eq!(shell.panes[0].as_ref().unwrap().nav.counters.lists, 0);
    });
}

#[test]
fn ey379_rename_edits_the_neighbour_in_place_and_sends_its_validated_path() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b",
            &[("c", true), ("other.txt", false)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    let (backend, mut requests) = crate::backend::Backend::test_lines();
    let mut captured_id = 0;
    host::change(&mut cx, window, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        click(shell, Slot::Parent, 45., MouseButton::Right, window, cx);
        let id = shell.menu.captured.as_ref().unwrap().id;
        captured_id = id;
        for op in ["snapshot", "applications"] {
            shell.receive(0, Event::Producer(format!(r#"{{"t":"menuaction","op":"{op}","id":{id},"ok":true,"kinds":[32768],"modes":[33188],"applications":[]}}"#)), window, cx);
        }
        shell.menu.pending = Some(("rename".into(), String::new()));
        shell.activate_menu(window, cx);
        shell.receive(0, Event::Producer(format!(r#"{{"t":"menuaction","op":"validate","id":{id},"ok":true,"action":"rename","paths":["/srv/a/b/other.txt"]}}"#)), window, cx);
        let edit = shell
            .rename
            .edit
            .as_mut()
            .expect("Rename owns an inline editor");
        assert_eq!(edit.source, "/srv/a/b/other.txt");
        assert_eq!(edit.menu, id);
        edit.entry.begin("renamed.txt");
        assert_eq!(shell.renaming_row(0), Some(1));
        shell.rename_commit(cx);
        assert_eq!(
            shell.rename.edit.as_ref().unwrap().pending.as_deref(),
            Some("/srv/a/b/renamed.txt")
        );
        let pane = shell.panes[0].as_ref().unwrap();
        assert_eq!(pane.path, "/srv/a/b/c");
        assert_eq!(pane.pane.cursor, 0);
        assert_eq!(pane.nav.counters.lists, 0);
    });
    let sent: Vec<_> = std::iter::from_fn(&mut requests)
        .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("rename"))
        .collect();
    assert_eq!(
        sent.len(),
        1,
        "one validated neighbour rename reaches the backend"
    );
    assert_eq!(
        flea::json::field_str(&sent[0], "path").as_deref(),
        Some("/srv/a/b/other.txt")
    );
    assert_eq!(
        flea::json::field_str(&sent[0], "to").as_deref(),
        Some("renamed.txt")
    );
    assert_eq!(
        flea::json::field_usize(&sent[0], "menuId"),
        Some(captured_id as usize)
    );
}

#[test]
fn ey379_renaming_the_active_folders_ancestor_follows_its_new_path() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        click(shell, Slot::Parent, 15., MouseButton::Right, window, cx);
        let id = shell.menu.captured.as_ref().unwrap().id;
        for op in ["snapshot", "applications"] {
            shell.receive(0, Event::Producer(format!(r#"{{"t":"menuaction","op":"{op}","id":{id},"ok":true,"kinds":[16384],"modes":[16877],"applications":[]}}"#)), window, cx);
        }
        shell.menu.pending = Some(("rename".into(), String::new()));
        shell.activate_menu(window, cx);
        shell.receive(0, Event::Producer(format!(r#"{{"t":"menuaction","op":"validate","id":{id},"ok":true,"action":"rename","paths":["/srv/a/b/c"]}}"#)), window, cx);
        shell.rename.edit.as_mut().unwrap().entry.begin("renamed-c");
        shell.rename_commit(cx);
        shell.receive(
            0,
            Event::Producer(r#"{"t":"renamed","ok":true,"path":"/srv/a/b/renamed-c"}"#.into()),
            window,
            cx,
        );
        let pane = shell.panes[0].as_ref().unwrap();
        assert_eq!(pane.pane.place(), "/srv/a/b/renamed-c");
        assert_eq!(
            pane.nav.pending_select.as_deref(),
            Some("/srv/a/b/renamed-c/d01")
        );
        assert!(shell.rename.edit.is_none());
    });
}

// EY-347: Pane.qml focusPress, a press on a neighbour column takes the keyboard back from the rail as a list press does.
#[test]
fn ey347_a_parent_column_press_takes_the_keyboard_back_from_the_rail() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(shell, "/srv/a/b/c/d01", &[], false, window, cx);
    });
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        window.focus(&shell.dual_focus.rail, cx);
        assert!(!shell.focus.is_focused(window));
        click(shell, Slot::Parent, 10., MouseButton::Left, window, cx);
        assert!(
            shell.focus.is_focused(window),
            "the parent press left the keyboard on the rail"
        );
        assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
    });
}
