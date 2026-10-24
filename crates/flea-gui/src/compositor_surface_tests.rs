//! Every rounded surface is painted with the same compositor value at zero and eight.
use gpui::{Quad, TestAppContext, VisualTestContext};

mod chrome;
mod dialogs;
mod settings;

fn painted(cx: &mut VisualTestContext) -> (f32, Vec<Quad>) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.scale_factor(), window.painted_quads())
    })
}

fn frames(name: &str, cx: &mut VisualTestContext, corner: f32, checks: &[f32]) {
    let (scale, quads) = painted(cx);
    assert!(!quads.is_empty(), "{name} must paint");
    let mut count = 0;
    for quad in &quads {
        let radii = [
            quad.corner_radii.top_left,
            quad.corner_radii.top_right,
            quad.corner_radii.bottom_left,
            quad.corner_radii.bottom_right,
        ];
        if corner == 0. {
            assert!(
                radii.iter().all(|radius| radius.0 == 0.),
                "{name} must paint square: {quad:?}"
            );
        }
        let widths = [
            quad.border_widths.top,
            quad.border_widths.right,
            quad.border_widths.bottom,
            quad.border_widths.left,
        ];
        if !widths.iter().all(|width| width.0 > 0.) {
            continue;
        }
        let w = quad.bounds.size.width.0 / scale;
        let h = quad.bounds.size.height.0 / scale;
        let check = checks
            .iter()
            .any(|side| (w - side).abs() <= 0.5 && (h - side).abs() <= 0.5);
        let expected = if check || h < 24. {
            corner / 2.
        } else {
            corner
        };
        assert!(
            radii
                .iter()
                .all(|radius| (radius.0 / scale - expected).abs() < 0.01),
            "{name} frame {w}x{h} must paint radius {expected}: {quad:?}"
        );
        count += 1;
    }
    assert!(count > 0, "{name} must paint a complete frame");
    println!("M9_CORNER surface={name} corner={corner} frames={count} PASS");
}

fn all(cx: &mut TestAppContext, corner: f32) {
    cx.update(|cx| cx.set_global(crate::compositor::Corner(corner)));
    settings::check(cx, corner);
    dialogs::check(cx, corner);
    chrome::check(cx, corner);
}

#[gpui::test]
fn m9_corner_all_surfaces_square(cx: &mut TestAppContext) {
    all(cx, 0.);
}

#[gpui::test]
fn m9_corner_all_surfaces_rounded(cx: &mut TestAppContext) {
    all(cx, 8.);
}
