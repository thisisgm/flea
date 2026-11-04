//! EY-402 in the file chooser: the current view's mark paints in the foreground and the other in the muted frame.
use super::chrome_view::DRAWN;
use super::process::Line;
use super::tests::fixture::fixture;
use crate::palette::Theme;
use crate::window::fixchrome_tests::{assert_view_inks, LATTE, SUNSET};
use gpui::TestAppContext;

/// The chooser chrome's order in chrome_view.rs: Back, Up, List, Grid.
const LIST: usize = 2;
const GRID: usize = 3;

#[gpui::test]
fn fixchrome_ey402_chooser_view_marks_follow_the_current_view(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":0,"v":1,"w":true}"#.into(),
    ));
    f.cx.run_until_parked();
    for (view, current, other) in [("grid", GRID, LIST), ("list", LIST, GRID)] {
        f.update(|picker, cx| picker.user_set_view(view, cx));
        for (name, theme) in [
            ("current", Theme::default()),
            ("catppuccin-latte", Theme::from_text(LATTE)),
            ("sunset", Theme::from_text(SUNSET)),
        ] {
            f.cx.update(|window, cx| {
                cx.set_global(theme);
                window.refresh();
                window.draw(cx).clear(cx);
            });
            f.cx.run_until_parked();
            let frame = DRAWN.take().expect("the chooser chrome drew");
            let [current, other] = [current, other].map(|index| {
                frame.borrow().controls[index]
                    .mark_ink
                    .expect("mark painted")
            });
            assert_view_inks(&format!("{view} {name}"), &theme, current, &[other]);
        }
    }
}
