//! The capture probe reads the Columns painter's own frame: its cursor, its guards, its chevrons and its sizes.
use super::host;
use crate::{
    columns::{geometry::Slot, paint::Focus},
    list::pane::Held,
    visual::geometry::Probe,
};
use flea::jsondoc::Json;
use std::rc::Rc;

/// Rows the pane holds in the held-but-unpainted case, well past the one screen the 700 px test window paints.
const HELD_ROWS: usize = 60;
/// A cursor row inside those held rows that no frame of the two-row fixture painted.
const UNPAINTED: usize = 50;

#[test]
fn columns_capture_cursor_is_the_row_the_painter_drew_never_the_model() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, host::settle);
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        assert_eq!(shell.visual_drawn_cursor(), Some(0));
        shell.panes[0].as_mut().unwrap().pane.cursor = 1;
        assert_eq!(
            shell.visual_drawn_cursor(),
            Some(0),
            "no frame painted row 1 yet"
        );
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, _, _| {
        assert_eq!(shell.visual_drawn_cursor(), Some(1));
        let painter = {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            Rc::clone(columns.painters.get(Slot::Active).unwrap())
        };
        let painted = painter.borrow().drawn.as_ref().map(|drawn| {
            let held = drawn.held.clone();
            (Rc::clone(&drawn.focus), Rc::clone(&drawn.tracks), held)
        });
        let (focus, tracks, held) = painted.unwrap();
        let edit = |change: &dyn Fn(&mut crate::columns::paint::Drawn)| {
            change(painter.borrow_mut().drawn.as_mut().unwrap());
        };
        let rows = (0..HELD_ROWS)
            .map(|at| host::row(&format!("d{at:03}"), true))
            .collect();
        let tall = Rc::new(Held {
            total: HELD_ROWS,
            rows,
            ..Held::default()
        });
        let model = std::mem::replace(
            &mut shell.panes[0].as_mut().unwrap().pane.held,
            Rc::clone(&tall),
        );
        edit(&|drawn| {
            drawn.held = Some(Rc::clone(&tall));
            drawn.focus = Rc::new(Focus {
                cursor: Some(UNPAINTED),
                ..(*focus).clone()
            });
        });
        assert!(tall.row(UNPAINTED).is_some() && painter.borrow().prepared(UNPAINTED).is_none());
        assert_eq!(
            shell.visual_drawn_cursor(),
            None,
            "a held cursor row the frame did not paint"
        );
        shell.panes[0].as_mut().unwrap().pane.held = model;
        edit(&|drawn| {
            drawn.held = held.clone();
            drawn.focus = Rc::clone(&focus);
        });
        assert_eq!(shell.visual_drawn_cursor(), Some(1));
        edit(&|drawn| {
            let mut disabled = tracks.to_vec();
            disabled[drawn.at].enabled = false;
            drawn.tracks = disabled.into();
        });
        assert_eq!(shell.visual_drawn_cursor(), None, "a disabled track");
        edit(&|drawn| drawn.tracks = Rc::clone(&tracks));
        edit(&|drawn| drawn.held = Some(Rc::new(Held::default())));
        assert_eq!(
            shell.visual_drawn_cursor(),
            None,
            "rows the pane no longer holds"
        );
    });
}

#[test]
fn columns_capture_labels_follow_the_painted_chevrons_and_only_the_active_track_sizes() {
    let lifted = format!("{}-lifted", "c".repeat(120));
    let sibling = format!("{}-plain", "c".repeat(120));
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, &format!("/srv/a/b/{lifted}"));
    host::change(&mut cx, handle, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b",
            &[(&lifted, true), (&sibling, true)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(&mut cx, handle);
    host::change(&mut cx, handle, |shell, window, _| {
        let probe = Probe::default();
        shell.visual_view_geometry(&probe, window);
        let data = probe.json(&Json::Null);
        let roles = data.get("roles").and_then(Json::as_array).unwrap();
        let text = |role: &Json, key: &str| {
            role.get(key)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        assert!(roles
            .iter()
            .all(|role| !(text(role, "role") == "value"
                && text(role, "id").starts_with("columns-0-2"))));
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        let paint = columns.painters.get(Slot::Parent).unwrap().borrow();
        for (index, chevron) in [(0, true), (1, false)] {
            let prepared = paint.prepared(index).unwrap();
            let shown = f64::from(f32::from(prepared.name_for(chevron, false).line().width));
            let other = f64::from(f32::from(prepared.name_for(!chevron, false).line().width));
            assert!(
                (shown - other).abs() > 1.,
                "row {index}: the chevron changes the drawn name"
            );
            let id = format!("columns-0-2-row-{index}-label");
            let label = roles.iter().find(|role| text(role, "id") == id).unwrap();
            let width = label.get("rect").and_then(Json::as_array).unwrap()[2]
                .as_f64()
                .unwrap();
            assert!(
                (width - shown).abs() < 1e-3,
                "row {index}: {width} is not the painted {shown}"
            );
        }
    });
}
