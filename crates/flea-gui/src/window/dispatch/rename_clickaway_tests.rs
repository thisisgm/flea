//! A click-away commits on release so a fast reply cannot replace the pressed listing before selection.
use super::super::{
    menus_tests,
    test_shell::{land, row, set_cursor},
};
use super::{close, draw, editing, ready_root, receive, sent};
use crate::{
    backend::Event,
    window::{startup::View, u07_tests::read},
};
use gpui::{point, px, size, Bounds, Modifiers, MouseButton, TestAppContext};
use std::time::Duration;

// The rowCentre reply is two window coordinates, for example "412 287".
fn centre(
    shell: &gpui::Entity<crate::state::Shell>,
    cx: &mut gpui::VisualTestContext,
    row: usize,
) -> gpui::Point<gpui::Pixels> {
    let centre = read(shell, cx, "rowCentre", &[&row.to_string()]);
    let coordinates: Vec<f32> = centre
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    point(px(coordinates[0]), px(coordinates[1]))
}

fn clickaway(cx: &mut TestAppContext, view: View, pending: bool) {
    for menu in [false, true] {
        let (shell, cx, mut lines) = ready_root(cx, "default");
        land(&shell, cx, "/files", menus_tests::home(), 2);
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
        set_cursor(&shell, cx, 2);
        menus_tests::settle(cx);
        if menu {
            cx.simulate_keystrokes("shift-f10");
            menus_tests::answer(&shell, cx, &[0o100644]);
            menus_tests::choose(&shell, cx, "rename");
        } else {
            cx.simulate_keystrokes("f2");
        }
        draw(cx);
        assert_eq!(editing(&shell, cx).unwrap().1, "notes.txt");
        cx.simulate_input("draft");
        draw(cx);
        sent(&mut lines);
        if pending {
            cx.simulate_keystrokes("enter");
            assert_eq!(sent(&mut lines).len(), 1);
            draw(cx);
        }
        let at = centre(&shell, cx, 3);
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.executor().advance_clock(Duration::from_millis(200));
        draw(cx);
        assert!(
            sent(&mut lines)
                .iter()
                .all(|line| flea::json::field_str(line, "c").as_deref() != Some("rename")),
            "{view:?}, menu={menu}: no rename reply may refresh the listing before release"
        );
        if pending {
            receive(
                &shell,
                cx,
                Event::Refused {
                    operation: "rename".into(),
                    path: "/files/notes.txt".into(),
                    message: "file exists".into(),
                    mode: 0,
                },
            );
            draw(cx);
        }
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        let requests = sent(&mut lines);
        if pending {
            assert!(
                requests
                    .iter()
                    .all(|line| flea::json::field_str(line, "c").as_deref() != Some("rename")),
                "{view:?}, menu={menu}: release must not retry an earlier refusal"
            );
            shell.read_with(cx, |shell, _| {
                let edit = shell.rename.edit.as_ref().unwrap();
                assert!(edit.pending.is_none());
                assert!(!edit.error.is_empty());
                assert_eq!(edit.entry.text, "draft.txt");
            });
            close(&shell, cx);
            continue;
        }
        assert_eq!(
            requests
                .iter()
                .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("rename"))
                .count(),
            1
        );
        assert_eq!(
            read(&shell, cx, "cursor", &[]),
            "3",
            "{view:?}, menu={menu}: the tap selects its neighbour"
        );
        receive(
            &shell,
            cx,
            Event::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
        );
        assert!(editing(&shell, cx).is_none());
        land(
            &shell,
            cx,
            "/files",
            vec![
                row("alpha", true),
                row("beta", true),
                row("draft.txt", false),
                row("todo.md", false),
            ],
            3,
        );
        assert_eq!(
            read(&shell, cx, "cursor", &[]),
            "3",
            "{view:?}, menu={menu}: the reply keeps the clicked neighbour"
        );
        assert_eq!(read(&shell, cx, "selectedIndices", &[]), "3");
        close(&shell, cx);
    }
}

#[gpui::test]
fn rename_clickaway_list_keeps_clicked_row(cx: &mut TestAppContext) {
    clickaway(cx, View::List, false);
    clickaway(cx, View::List, true);
}

#[gpui::test]
fn rename_clickaway_columns_keeps_clicked_row(cx: &mut TestAppContext) {
    clickaway(cx, View::Columns, false);
    clickaway(cx, View::Columns, true);
}

#[gpui::test]
fn rename_clickaway_grid_keeps_clicked_row(cx: &mut TestAppContext) {
    clickaway(cx, View::Grid, false);
    clickaway(cx, View::Grid, true);
}

#[gpui::test]
fn rename_clickaway_dual_keeps_clicked_row(cx: &mut TestAppContext) {
    clickaway(cx, View::Dual, false);
    clickaway(cx, View::Dual, true);
}

// The tab strip exception reads the press: a press on the strip never commits, and a release there does not cancel one.
#[gpui::test]
fn rename_clickaway_judges_the_tab_strip_at_the_press(cx: &mut TestAppContext) {
    for strip_pressed in [true, false] {
        let (shell, cx, mut lines) = ready_root(cx, "default");
        land(&shell, cx, "/files", menus_tests::home(), 2);
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.set_view(View::List, window, cx))
        });
        set_cursor(&shell, cx, 2);
        menus_tests::settle(cx);
        cx.simulate_keystrokes("f2");
        draw(cx);
        cx.simulate_input("draft");
        draw(cx);
        sent(&mut lines);
        let press = centre(&shell, cx, 3);
        let release = press + point(px(20.), px(0.));
        let on_strip = if strip_pressed { press } else { release };
        // A one-tab frame clears the strip band, so the band around one end of the gesture is set before each event.
        let strip = Bounds::new(on_strip - point(px(2.), px(2.)), size(px(4.), px(4.)));
        shell.update(cx, |shell, _| shell.strip.bounds.set(Some(strip)));
        cx.simulate_mouse_down(press, MouseButton::Left, Modifiers::none());
        shell.update(cx, |shell, _| shell.strip.bounds.set(Some(strip)));
        cx.simulate_mouse_up(release, MouseButton::Left, Modifiers::none());
        let renames = sent(&mut lines)
            .iter()
            .filter(|line| flea::json::field_str(line, "c").as_deref() == Some("rename"))
            .count();
        assert_eq!(
            renames,
            usize::from(!strip_pressed),
            "strip_pressed={strip_pressed}: only a press off the strip commits"
        );
        close(&shell, cx);
    }
}
