//! EY-496: m over the Locked hero opens its menu at the listing centre, as v0.3.12 Pane.qml openCursorMenu does.
use super::*;

#[gpui::test]
fn fixlogic2_locked_hero_menu_opens_at_the_listing_centre(cx: &mut TestAppContext) {
    let handle = locked(cx, Some("[]"));
    let (centre, rect) = handle
        .update(cx, |shell, window, _| {
            assert!(shell.menu_opened(), "the locked folder's menu opens");
            let pane = shell.geometry(window).rows(
                0,
                shell.view == crate::window::startup::View::Dual,
                shell.slots.strip_height,
            );
            (
                (pane.x + pane.width / 2., pane.y + pane.height / 2.),
                (pane.x, pane.y, pane.width, pane.height),
            )
        })
        .unwrap();
    let frame = read(cx, handle, "contextMenuRect").stdout;
    let card: Vec<f32> = frame
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    let [x, y, _, _] = card[..] else {
        panic!("contextMenuRect answers x y w h: {frame:?}");
    };
    // ContextMenu.qml openLocked places the card at the point it is given; the reader rounds to whole pixels.
    assert!(
        (x - centre.0).abs() <= 1. && (y - centre.1).abs() <= 1.,
        "the menu opens at the listing's centre {centre:?}, pane {rect:?}, card {frame:?}"
    );
}
