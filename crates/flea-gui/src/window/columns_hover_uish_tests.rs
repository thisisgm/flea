//! Native overlays must read the active Columns frame's actual hover state.
use super::columns_tests::host;
use gpui::{point, px, Modifiers, VisualTestContext};

#[test]
fn uishlist_native_columns_hover_reader_follows_the_painted_frame() {
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("seed.txt", false)],
            false,
            window,
            cx,
        );
    });
    host::frame(&mut cx, window);
    let mut at = point(px(0.), px(0.));
    host::change(&mut cx, window, |shell, window, cx| {
        // Sample input: rowCentre prints "646 74" (window x y).
        let centre: Vec<f32> = host::call(shell, "rowCentre", &["1"], window, cx)
            .stdout
            .split_whitespace()
            .map(|number| number.parse().unwrap())
            .collect();
        assert_eq!(centre.len(), 2);
        at = point(px(centre[0]), px(centre[1]));
    });
    {
        let mut vcx = VisualTestContext::from_window(window.into(), &cx);
        vcx.simulate_mouse_move(at, None, Modifiers::none());
        vcx.run_until_parked();
    }
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "rowHovered", &["1"], window, cx)
                .stdout
                .trim(),
            "true"
        );
        assert_eq!(
            host::call(shell, "rowHovered", &["99"], window, cx)
                .stdout
                .trim(),
            "false"
        );
    });
    {
        let mut vcx = VisualTestContext::from_window(window.into(), &cx);
        vcx.simulate_keystrokes(",");
        vcx.run_until_parked();
    }
    host::frame(&mut cx, window);
    host::change(&mut cx, window, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "settingsOpen", &[], window, cx)
                .stdout
                .trim(),
            "true"
        );
        assert_eq!(
            host::call(shell, "rowHovered", &["1"], window, cx)
                .stdout
                .trim(),
            "false"
        );
    });
}
