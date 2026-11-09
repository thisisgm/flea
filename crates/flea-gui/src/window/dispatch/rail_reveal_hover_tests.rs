//! S10V-5: a revealed auto-hide rail holds the pointer, so the rows it covers draw no hover wash in List or Dual.
use super::{jsondoc, open, read, settle, Entity, Shell, TestAppContext, VisualTestContext};
use crate::window::startup::View;
use gpui::{point, px, Modifiers};

fn hovered(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| shell.read(cx).list_states(0, window, cx).hovered)
}

fn cover(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<gpui::Bounds<gpui::Pixels>> {
    shell.read_with(cx, |shell, cx| shell.rail.as_ref()?.host.read(cx).cover())
}

fn hover_through(cx: &mut TestAppContext, view: View) {
    let (shell, cx) = open(cx, false);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let mut state = shell.launch.settings.data.clone();
            crate::g2::settings_pages::seams::merge(
                &mut state,
                &jsondoc::parse(r#"{"places":{"autoHide":true}}"#).unwrap(),
            );
            shell.apply_settings(state, window, cx);
            shell.set_view(view, window, cx);
        })
    });
    settle(cx);
    assert_eq!(shell.read_with(cx, |shell, _| shell.view), view);
    // rowCentre answers "x y" in window pixels, for example "412.5 118"; the second field is y.
    let centre = read(&shell, cx, "rowCentre", &["1"]);
    let y: f32 = centre
        .split_whitespace()
        .nth(1)
        .and_then(|y| y.parse().ok())
        .unwrap_or_else(|| panic!("row 1 is drawn, centre {centre:?}"));
    // The pointer on the window's left edge reveals the rail over the left pane.
    cx.simulate_mouse_move(point(px(1.), px(y)), None, Modifiers::none());
    settle(cx);
    // The reveal slides in; read the cover once two settled frames agree.
    let mut drawn = cover(&shell, cx);
    for _ in 0..10 {
        settle(cx);
        let now = cover(&shell, cx);
        if now.is_some() && now == drawn {
            break;
        }
        drawn = now;
    }
    let cover = drawn.expect("the overlay rail is revealed");
    assert!(
        cover.size.width > px(100.),
        "the rail stands open: {cover:?}"
    );
    // The pointer that revealed the rail rests on its edge over row 1 and moves no more, as in the S10V-5 capture.
    assert_eq!(
        hovered(&shell, cx),
        None,
        "{view:?}: the row under the pointer that revealed the rail lit its hover wash"
    );
    let inside = point(cover.right() - px(12.), px(y));
    cx.simulate_mouse_move(inside, None, Modifiers::none());
    settle(cx);
    assert!(inside.x > cover.left() && inside.x < cover.right());
    assert_eq!(
        hovered(&shell, cx),
        None,
        "{view:?}: the row under the revealed rail lit its hover wash"
    );
    // The same row beside the rail still lights, so the wash is withheld only under it.
    cx.simulate_mouse_move(
        point(cover.right() + px(40.), px(y)),
        None,
        Modifiers::none(),
    );
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        hovered(&shell, cx),
        Some(1),
        "{view:?}: a row beside the rail"
    );
}

#[gpui::test]
fn the_revealed_rail_withholds_the_hover_wash_from_the_list_rows_under_it(cx: &mut TestAppContext) {
    hover_through(cx, View::List);
}

#[gpui::test]
fn the_revealed_rail_withholds_the_hover_wash_from_the_dual_left_pane(cx: &mut TestAppContext) {
    hover_through(cx, View::Dual);
}
