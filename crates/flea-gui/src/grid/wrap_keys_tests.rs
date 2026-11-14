//! Shared horizontal selection keys and filtered navigation contracts.
use super::join_tests::{frame, key};
use super::wrap_tests::configured;
use crate::{grid::Shown, list::pane::page_items, window::startup::View};
use gpui::TestAppContext;
use std::rc::Rc;

#[gpui::test]
fn issue_434_shared_shift_keys_extend_with_row_wrapping_off(cx: &mut TestAppContext) {
    for preset in ["default", "vim", "mac", "windows"] {
        let (handle, _requests) = configured(cx, &format!(r#"{{"keys":"{preset}"}}"#));
        let columns = handle
            .update(cx, |shell, _, _| {
                let host = shell.panes[0].as_mut().unwrap();
                let columns = host
                    .grid
                    .as_ref()
                    .unwrap()
                    .view
                    .layout(Shown::All(host.pane.held.total))
                    .columns;
                assert!(columns > 1);
                host.pane.cursor = columns - 2;
                columns
            })
            .unwrap();
        key(cx, handle, "shift-right");
        handle
            .update(cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(
                    host.pane.cursor,
                    columns - 1,
                    "{preset}: extend inside the row with wrapping off"
                );
                assert_eq!(host.selected, vec![columns - 2, columns - 1]);
            })
            .unwrap();
        key(cx, handle, "shift-right");
        handle
            .update(cx, |shell, _, _| {
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.cursor,
                    columns - 1,
                    "{preset}: stop at the row end"
                );
            })
            .unwrap();
        key(cx, handle, "shift-left");
        handle
            .update(cx, |shell, _, _| {
                let host = shell.panes[0].as_ref().unwrap();
                assert_eq!(host.pane.cursor, columns - 2);
                assert_eq!(host.selected, vec![columns - 2]);
            })
            .unwrap();
    }
}

#[gpui::test]
fn issue_434_shift_never_wraps_at_either_folder_end(cx: &mut TestAppContext) {
    for rows in [false, true] {
        for ends in [false, true] {
            let (handle, _requests) = configured(
                cx,
                &format!(r#"{{"gridWrapRows":{rows},"wrapAtEnds":{ends}}}"#),
            );
            handle
                .update(cx, |shell, _, _| {
                    shell.panes[0].as_mut().unwrap().selected = vec![2];
                })
                .unwrap();
            key(cx, handle, "shift-left");
            let last = handle
                .update(cx, |shell, _, _| {
                    let host = shell.panes[0].as_mut().unwrap();
                    assert_eq!(host.pane.cursor, 0, "rows={rows}, ends={ends}");
                    assert_eq!(host.selected, vec![2]);
                    let last = host.pane.held.total - 1;
                    host.pane.cursor = last;
                    host.selected = vec![last - 2];
                    last
                })
                .unwrap();
            key(cx, handle, "shift-right");
            handle
                .update(cx, |shell, _, _| {
                    let host = shell.panes[0].as_mut().unwrap();
                    assert_eq!(host.pane.cursor, last, "rows={rows}, ends={ends}");
                    assert_eq!(host.selected, vec![last - 2]);
                    host.pane.cursor = 0;
                    host.selected.clear();
                })
                .unwrap();
            let probes = [("shift-right", 1, vec![0, 1]), ("shift-left", 0, vec![0])];
            for (chord, cursor, selected) in probes {
                key(cx, handle, chord);
                handle
                    .update(cx, |shell, _, _| {
                        let host = shell.panes[0].as_ref().unwrap();
                        assert_eq!(
                            host.pane.cursor, cursor,
                            "rows={rows}, ends={ends}: {chord} is bound here"
                        );
                        assert_eq!(
                            host.selected, selected,
                            "rows={rows}, ends={ends}: {chord} extends"
                        );
                    })
                    .unwrap();
            }
        }
    }
}

#[gpui::test]
fn issue_434_horizontal_extension_keeps_list_and_columns_unchanged(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns] {
        let (handle, _requests) = configured(cx, "{}");
        handle
            .update(cx, |shell, window, cx| shell.set_view(view, window, cx))
            .unwrap();
        frame(cx, handle);
        let path = handle
            .update(cx, |shell, _, _| {
                let host = shell.panes[0].as_mut().unwrap();
                host.pane.cursor = 3;
                host.selected = vec![0, 5];
                host.pane.path().to_owned()
            })
            .unwrap();
        for chord in ["shift-right", "shift-left"] {
            key(cx, handle, chord);
            handle
                .update(cx, |shell, _, _| {
                    let host = shell.panes[0].as_ref().unwrap();
                    assert_eq!(host.pane.cursor, 3);
                    assert_eq!(host.selected, vec![0, 5]);
                    assert_eq!(host.pane.path(), path);
                })
                .unwrap();
        }
    }
}

#[gpui::test]
fn issue_434_default_off_filter_keys_follow_visible_order(cx: &mut TestAppContext) {
    for (chord, start) in [
        ("home", 99),
        ("end", 0),
        ("pageup", 99),
        ("pagedown", 0),
        ("left", 2),
        ("right", 1),
    ] {
        let (handle, _requests) = configured(cx, "{}");
        let shown: Rc<[usize]> = (0..100).map(|i| i * 3 + 1).collect::<Vec<_>>().into();
        let target = handle
            .update(cx, |shell, _, _| {
                let host = shell.panes[0].as_mut().unwrap();
                let page = page_items(host.pane.visible_rows());
                host.filter.shown = Some(Rc::clone(&shown));
                host.input.set_shown(Some(Rc::clone(&shown)));
                host.pane.cursor = shown[start];
                match chord {
                    "home" => 0,
                    "end" => 99,
                    "pageup" => 99_usize.saturating_sub(page),
                    "pagedown" => page.min(99),
                    "left" => 1,
                    "right" => 2,
                    _ => unreachable!(),
                }
            })
            .unwrap();
        key(cx, handle, chord);
        handle
            .update(cx, |shell, _, _| {
                assert_eq!(
                    shell.panes[0].as_ref().unwrap().pane.cursor,
                    shown[target],
                    "{chord} uses the visible position"
                );
            })
            .unwrap();
    }
}
