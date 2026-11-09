//! FIX-MENUS EY-416: P over a hidden Paste As opens its flyout alone, and no main row stays lit beside it.
use super::menu::{Face, Outcome};
use super::tests::{file_rows, window, with, WIDE};
use gpui::TestAppContext;

// EY-536: a still pointer's notches step one enabled row each; the scene's pointer-at before every notch is a hover on Copy, which re-lights it first.
#[gpui::test]
fn fixmenus_ey536_a_still_pointer_wheel_steps_one_enabled_row_per_notch(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        let at = |action: &str| {
            card.entries
                .iter()
                .position(|row| !row.separator && row.action == action)
                .unwrap()
        };
        let [cut, copy, duplicate, rename] = ["cut", "copy", "duplicate", "rename"].map(at);
        let (x, y) = (card.frame.x + 30., card.row_screen_top(copy) + 10.);
        // A notch down is -120 raw units and a notch up +120, as the platform wheel reports them.
        let notches = [-120, -120, 120];
        menu.card.as_mut().unwrap().cursor = Some(copy);
        let still: Vec<_> = notches
            .iter()
            .map(|&units| {
                menu.wheel(x, y, units);
                menu.card.as_ref().unwrap().cursor
            })
            .collect();
        assert_eq!(
            still,
            [Some(duplicate), Some(rename), Some(duplicate)],
            "v0.3.12's 6, 9, 10, 9: Paste is disabled and skipped"
        );
        menu.card.as_mut().unwrap().cursor = Some(copy);
        let warped: Vec<_> = notches
            .iter()
            .map(|&units| {
                menu.hover(x, y, env, window);
                menu.wheel(x, y, units);
                menu.card.as_ref().unwrap().cursor
            })
            .collect();
        assert_eq!(
            warped,
            [Some(duplicate), Some(duplicate), Some(cut)],
            "the GPUI capture's 10, 12, 12, 9 is a hover on Copy before each notch"
        );
    });
}

#[gpui::test]
fn fixmenus_ey416_a_lone_flyout_lights_only_its_first_leaf(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.card.as_ref().unwrap().cursor,
            Some(0),
            "a key open lands on the first row"
        );
        assert_eq!(
            menu.open_submenu_for("pasteAs", true, env, window),
            Outcome::Redraw
        );
        let flyout = menu.flyout.as_ref().unwrap();
        assert_eq!(
            flyout.parent, None,
            "Paste As is hidden, so the flyout opens lone"
        );
        assert_eq!(flyout.card.cursor, Some(0), "Link, its first leaf, is lit");
        assert_eq!(
            menu.card.as_ref().unwrap().cursor,
            None,
            "v0.3.12 lights no main row beside a lone flyout"
        );
    });
}
