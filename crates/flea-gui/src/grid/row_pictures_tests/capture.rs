//! The capture probe's Columns ink is the slot the painter drew a lent thumbnail in, and the mark square otherwise.
use super::*;
use crate::{columns::geometry, visual::geometry::Probe};

fn rect(roles: &[Json], id: &str) -> Vec<f64> {
    let role = roles
        .iter()
        .find(|role| role.get("id").and_then(Json::as_str) == Some(id))
        .unwrap_or_else(|| panic!("no {id} role"));
    let values = role.get("rect").and_then(Json::as_array).unwrap();
    values.iter().map(|value| value.as_f64().unwrap()).collect()
}

#[gpui::test]
fn columns_capture_ink_is_the_slot_the_thumbnail_painted_in(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::Columns, "{}");
    first_settle(cx, handle, &mut lines);
    assert!(deliver(cx, handle, &thumbed(1, PICTURE)));
    cx.run_until_parked();
    step(cx, handle);
    handle
        .update(cx, |shell, window, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let painter = host.columns.as_ref().unwrap().painters.get(Slot::Active);
            let paint = painter.unwrap().borrow();
            assert!(paint.picture(1).is_some() && paint.picture(0).is_none());
            drop(paint);
            let probe = Probe::default();
            shell.visual_view_geometry(&probe, window);
            let data = probe.json(&Json::Null);
            let roles = data.get("roles").and_then(Json::as_array).unwrap();
            for (index, (x, y, side)) in [
                (0, geometry::mark_box(crate::list::pane::ROW_HEIGHT)),
                (1, geometry::picture_box(crate::list::pane::ROW_HEIGHT)),
            ] {
                let row = rect(roles, &format!("columns-0-3-row-{index}"));
                let ink = rect(roles, &format!("columns-0-3-row-{index}-ink"));
                let offsets = [ink[0] - row[0], ink[1] - row[1], ink[2], ink[3]];
                for (got, want) in offsets.into_iter().zip([x, y, side, side]) {
                    assert!(
                        (got - f64::from(want)).abs() < 1e-3,
                        "row {index}: {offsets:?}"
                    );
                }
            }
        })
        .unwrap();
}
