//! Carry clicks use actual drawn List/Grid/Columns targets and the captured source rows.
use super::m9_g2rows_25_rig::{answer, edit, frame, mark, open, riding};
use super::m9_g2rows_25_target_rig::{down, target, view, Fixture};
use crate::{columns::geometry::Slot, window::startup::View};

#[test]
fn m9_g2rows_25_mounted_list_folder_uses_foreign_source_identity() {
    lands(View::List, Some(0), Slot::Active, "dest/folder");
}

#[test]
fn m9_g2rows_25_mounted_grid_folder_uses_foreign_source_identity() {
    lands(View::Grid, Some(0), Slot::Active, "dest/folder");
}

#[test]
fn m9_g2rows_25_mounted_grid_background_keeps_target_selection() {
    lands(View::Grid, None, Slot::Active, "dest");
}

#[test]
fn m9_g2rows_25_mounted_columns_active_and_peek_and_background() {
    for (row, slot, dest) in [
        (Some(0), Slot::Active, "dest/folder"),
        (None, Slot::Active, "dest"),
        (Some(0), Slot::Child, "dest/folder/nested"),
        (None, Slot::Child, "dest/folder"),
        (None, Slot::Parent, ""),
    ] {
        lands(View::Columns, row, slot, dest);
    }
}

fn lands(mode: View, row: Option<usize>, slot: Slot, expected: &str) {
    let fixture = Fixture::new();
    let mut cx = crate::list::bench::real_cx();
    let (from, mut from_lines) = open(&mut cx, &fixture.path("src"), &["folder", "a.txt"]);
    let (to, mut to_lines) = open(&mut cx, &fixture.path("dest"), &["folder", "a.txt"]);
    view(&mut cx, to, mode);
    from_lines();
    to_lines();
    // Source and target deliberately share row numbers and listing id, but not window identity.
    mark(&mut cx, from, vec![0]);
    edit(&mut cx, from, |shell, window, cx| {
        shell.carry_lift(window, cx)
    });
    assert_eq!(from_lines(), [r#"{"c":"paths","rows":[0]}"#]);
    answer(&mut cx, from, &[&fixture.path("src/folder")]);
    mark(&mut cx, from, vec![1]);
    mark(&mut cx, to, vec![1]);
    let at = target(&mut cx, to, row, slot);
    edit(&mut cx, to, |shell, window, cx| down(shell, at, window, cx));
    cx.run_until_parked();
    let sent = to_lines();
    let dest = fixture.path(expected);
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"collisions""#)
            && line.contains(&format!(r#""dest":"{dest}""#))
            && line.contains(&fixture.path("src/folder"))),
        "MOUNTED_CARRY_TARGET {mode:?} {slot:?}: {sent:?}"
    );
    assert!(riding(&mut cx).is_none());
    assert_eq!(
        edit(&mut cx, to, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        vec![1]
    );
    assert!(from_lines().is_empty());
}

#[test]
fn m9_g2rows_25_mounted_pending_clicks_never_select_or_navigate() {
    for mode in [View::List, View::Grid, View::Columns] {
        let fixture = Fixture::new();
        let mut cx = crate::list::bench::real_cx();
        let (handle, mut lines) = open(&mut cx, &fixture.path("dest"), &["folder", "a.txt"]);
        view(&mut cx, handle, mode);
        lines();
        mark(&mut cx, handle, vec![1]);
        edit(&mut cx, handle, |shell, window, cx| {
            shell.carry_lift(window, cx)
        });
        assert_eq!(lines(), [r#"{"c":"paths","rows":[1]}"#]);
        let at = target(&mut cx, handle, Some(0), Slot::Active);
        edit(&mut cx, handle, |shell, window, cx| {
            down(shell, at, window, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.input.press.is_none(),
                "PENDING_CLICK_MUST_CLEAR_PRESS {mode:?}"
            );
            assert_eq!(
                host.selected,
                vec![1],
                "PENDING_CLICK_MUST_KEEP_SELECTION {mode:?}"
            );
            assert_eq!(host.nav.counters.lists, 0);
        });
        assert!(lines().is_empty());
    }
}

#[test]
fn m9_g2rows_25_mounted_source_row_exclusion_ignores_later_selection() {
    for mode in [View::List, View::Grid, View::Columns] {
        let fixture = Fixture::new();
        let mut cx = crate::list::bench::real_cx();
        let (handle, mut lines) = open(&mut cx, &fixture.path("dest"), &["folder", "a.txt"]);
        view(&mut cx, handle, mode);
        lines();
        mark(&mut cx, handle, vec![0]);
        edit(&mut cx, handle, |shell, window, cx| {
            shell.carry_lift(window, cx)
        });
        lines();
        answer(&mut cx, handle, &[&fixture.path("dest/folder")]);
        mark(&mut cx, handle, vec![1]);
        let at = target(&mut cx, handle, Some(0), Slot::Active);
        edit(&mut cx, handle, |shell, window, cx| {
            down(shell, at, window, cx)
        });
        cx.run_until_parked();
        assert!(
            lines().is_empty(),
            "CARRIED_FOLDER_MUST_NOT_LAND_IN_ITSELF {mode:?}"
        );
        assert!(riding(&mut cx).is_some());
        frame(&mut cx, handle);
    }
}
