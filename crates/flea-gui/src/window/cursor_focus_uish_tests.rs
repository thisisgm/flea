//! The mounted Grid and Columns cursor must lose its accent while another control owns input.
use super::{columns_tests::host, startup::View};
use crate::columns::geometry::Slot;
use gpui::{
    point, px, size, Bounds, Hsla, Pixels, TestAppContext, VisualTestContext, WindowHandle,
};

fn cursor_paint(
    cx: &mut TestAppContext,
    handle: WindowHandle<crate::state::Shell>,
    view: View,
    focused: bool,
) {
    host::frame(cx, handle);
    host::change(cx, handle, |shell, window, cx| {
        assert_eq!(shell.list_states(0, window, cx).focused, focused);
        let host = shell.panes[0].as_ref().unwrap();
        let (bounds, foreground, accent, edge) = match view {
            View::Grid => {
                let grid = host.grid.as_ref().unwrap().state.borrow();
                let cursor = grid.drawn().iter().find(|tile| tile.cursor).unwrap();
                let rect = grid.layout.unwrap().frame(cursor.cell);
                let bounds = Bounds::new(
                    grid.bounds.origin + point(px(rect.x), px(rect.y)),
                    size(px(rect.width), px(rect.height)),
                );
                let ink = grid.ink.unwrap();
                (bounds, ink.foreground, ink.accent, ink.accent_frame)
            }
            View::Columns => {
                let columns = host.columns.as_ref().unwrap();
                let paint = columns.painters.get(Slot::Active).unwrap().borrow();
                let drawn = paint.drawn.as_ref().unwrap();
                assert_eq!(host.pane.cursor, 0);
                assert_eq!(drawn.scroll, 0.);
                let bounds = Bounds::new(
                    drawn.bounds.origin,
                    size(drawn.bounds.size.width, px(drawn.row_height)),
                );
                let ink = paint.painted.unwrap();
                (bounds, ink.foreground, ink.accent, ink.edge())
            }
            _ => unreachable!(),
        };
        let scale = window.scale_factor();
        assert_ne!(
            foreground, accent,
            "the plate oracle needs distinct colour roles"
        );
        let quads = window.painted_quads();
        let matches = |quad: &gpui::Quad, bounds: Bounds<Pixels>| {
            let near =
                |actual: f32, expected: Pixels| (actual / scale - f32::from(expected)).abs() < 0.01;
            near(quad.bounds.origin.x.0, bounds.origin.x)
                && near(quad.bounds.origin.y.0, bounds.origin.y)
                && near(quad.bounds.size.width.0, bounds.size.width)
                && near(quad.bounds.size.height.0, bounds.size.height)
        };
        let fill: Hsla = if focused { accent } else { foreground }.opacity(0.18);
        assert!(
            quads
                .iter()
                .any(|quad| matches(quad, bounds) && quad.background.as_solid() == Some(fill)),
            "{view:?} cursor must paint its focused={focused} plate"
        );
        let has_edge = if view == View::Grid {
            quads.iter().any(|quad| {
                matches(quad, bounds) && quad.border_color == edge && quad.border_widths.top.0 > 0.
            })
        } else {
            let edge_bounds = Bounds::new(bounds.origin, size(px(2.), bounds.size.height));
            quads
                .iter()
                .any(|quad| matches(quad, edge_bounds) && quad.background.as_solid() == Some(edge))
        };
        assert_eq!(has_edge, focused, "{view:?} cursor accent edge");
    });
}

fn focus_handoffs(view: View) {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, handle, |shell, window, cx| {
        let accent = crate::palette::Theme::from_text("background='#00aa66'").background;
        cx.set_global(crate::palette::Theme {
            accent,
            accent_frame: accent,
            ..Default::default()
        });
        host::settle(shell, window, cx);
        shell.mount_rail(cx);
        shell.set_view(view, window, cx);
        window.focus(&shell.focus, cx);
        // The cursor draws the accent only in the active window; a TestWindow starts inactive.
        window.activate_window();
    });
    cx.run_until_parked();
    cursor_paint(&mut cx, handle, view, true);
    for stroke in ["m", ",", "tab"] {
        {
            let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
            vcx.simulate_keystrokes(stroke);
            vcx.run_until_parked();
        }
        host::change(&mut cx, handle, |shell, window, cx| match stroke {
            "m" => assert!(shell.menu.presentation.at.is_some()),
            "," => assert!(shell.settings.opened),
            "tab" => {
                assert_eq!(
                    host::call(shell, "focusView", &[], window, cx)
                        .stdout
                        .trim(),
                    "rail"
                );
                assert!(shell.menu.presentation.at.is_none());
                assert!(!shell.settings.opened);
            }
            _ => unreachable!(),
        });
        cursor_paint(&mut cx, handle, view, false);
        {
            let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
            vcx.simulate_keystrokes("escape");
            vcx.run_until_parked();
        }
        cursor_paint(&mut cx, handle, view, true);
    }
}

#[test]
fn uishlist_cursor_focus_grid_paints_menu_settings_and_rail_handoffs() {
    focus_handoffs(View::Grid);
}

#[test]
fn uishlist_cursor_focus_columns_paints_menu_settings_and_rail_handoffs() {
    focus_handoffs(View::Columns);
}
