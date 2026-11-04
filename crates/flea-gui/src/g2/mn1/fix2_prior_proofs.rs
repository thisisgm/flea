//! Review proofs for uish-u06 round 1, read through the live window the unit mounts.
use super::join_tests::{open, read, read_args, ready};
use crate::window::root::Root;
use flea::jsondoc::{self, Json};
use gpui::{point, px, AppContext, MouseButton, TestAppContext, VisualTestContext, WindowHandle};

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn num(value: Option<&Json>) -> f64 {
    match value {
        Some(Json::Num(text)) => text.parse().unwrap(),
        Some(Json::Bool(_)) | None => 0.,
        other => panic!("not a number: {other:?}"),
    }
}

#[gpui::test]
fn rev_u06_r1_menu_state_entries_carry_submenu_leaves(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    ready(cx, &mut fixture);
    let state = read(cx, handle, "menuState").stdout;
    println!("REV_MENUSTATE {}", state.trim());
    println!(
        "REV_MODEL {}",
        read(cx, handle, "contextMenuModel").stdout.trim()
    );
    let doc = jsondoc::parse(state.trim()).unwrap();
    let entries = doc.get("entries").and_then(Json::as_array).unwrap();
    let open_with = entries
        .iter()
        .find(|entry| entry.get("action").and_then(Json::as_str) == Some("openWith"))
        .expect("an Open with row");
    assert!(
        open_with.get("submenu").and_then(Json::as_array).is_some(),
        "ui-openwith-design.sh:18 and ui-menus.sh:187 iterate .submenu[]; menuState gives {:?}",
        open_with.get("submenu")
    );
}

#[gpui::test]
fn rev_u06_r1_menu_state_row_parts_sum_to_the_painted_row(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    ready(cx, &mut fixture);
    let state = jsondoc::parse(read(cx, handle, "menuState").stdout.trim()).unwrap();
    let width: f64 = state
        .get("frame")
        .and_then(Json::as_str)
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .parse()
        .unwrap();
    let wanted: Vec<f32> = handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let menu = shell.menu.menu.borrow();
            let card = menu.card.as_ref().unwrap();
            card.built.lines.iter().map(|line| line.wanted).collect()
        })
        .unwrap();
    let rows = state.get("rows").and_then(Json::as_array).unwrap();
    let mut need = 0f64;
    let mut bad = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.get("separator").and_then(Json::as_bool) == Some(true) {
            continue;
        }
        let wrap = row.get("hintWrap").and_then(Json::as_bool) == Some(true);
        // The sum tests/ui-card-layout.sh:228-229 takes, one row at a time.
        let sum = (num(row.get("markLeft"))
            + num(row.get("slot"))
            + num(row.get("labelLeft"))
            + num(row.get("label"))
            + num(row.get("labelRight"))
            + if wrap { 0. } else { num(row.get("hint")) }
            + num(row.get("hintRight"))
            + num(row.get("status"))
            + num(row.get("chevron"))
            + num(row.get("chevronRight")))
        .ceil();
        need = need.max(sum);
        println!(
            "REV_ROW {index} sum={sum} wanted={} parts={row:?}",
            wanted[index]
        );
        if (sum - f64::from(wanted[index])).abs() > 1. {
            bad.push((index, sum, wanted[index]));
        }
    }
    println!("REV_WIDTH frame={width} need={need}");
    assert!(
        bad.is_empty() && need <= width + 1.,
        "menuState.rows parts do not sum to the painted row: {bad:?}, need {need} against frame {width}"
    );
}

#[gpui::test]
fn rev_u06_r1_right_click_at_a_drawn_row_centre_captures_that_row(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    frame(cx, handle);
    for row in [1usize, 4, 9] {
        let centre = read_args(cx, handle, "rowCentre", &[&row.to_string()]).stdout;
        // The drawn list's own geometry, the same terms ipc/rows.rs row_box reads.
        let (x, y) = handle
            .update(cx, |root, _, cx| {
                let shell = root.shell.read(cx);
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                let drawn = &list.drawn;
                (
                    f32::from(drawn.bounds.origin.x) + 40.,
                    f32::from(drawn.bounds.origin.y) + (row as f32 + 0.5) * drawn.metrics.row
                        - drawn.scroll_y,
                )
            })
            .unwrap();
        VisualTestContext::from_window(handle.into(), cx).simulate_mouse_down(
            point(px(x), px(y)),
            MouseButton::Right,
            Default::default(),
        );
        ready(cx, &mut fixture);
        let cursor = read(cx, handle, "cursor").stdout;
        println!(
            "REV_HIT row={row} reader={:?} drawn={x} {y} cursor={}",
            centre.trim(),
            cursor.trim()
        );
        assert_eq!(
            cursor.trim(),
            row.to_string(),
            "right click at rowCentre {row}"
        );
        VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
        frame(cx, handle);
    }
}
