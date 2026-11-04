//! The Grid floor under glass, read from the mounted window as the List and Columns floors are.
use super::*;

// EY-214: with glass on, the Grid floor lets the wallpaper through as the List and Columns floors do (v0.3.12 Pane.qml listBackground).
#[gpui::test]
fn fix_views_2_ey214_the_grid_floor_draws_the_user_alpha(cx: &mut TestAppContext) {
    let fixture = install(
        cx,
        "fv2-glass-grid",
        &[(2, "[flea]\nbackground-alpha=0.34")],
    );
    let handle = launch(cx);
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                let host = shell.panes[0].as_mut().unwrap();
                let options = crate::pane::swap::Options {
                    supersede: true,
                    ..Default::default()
                };
                assert!(host.pane.begin("/glass".into(), options, host.pane.now()));
                let rows = ["a.txt", "b.txt", "c.txt"].map(|name| flea::protocol::RowFacts {
                    name: name.into(),
                    ..Default::default()
                });
                for reply in [
                    flea::protocol::Reply::Listed {
                        path: "/glass".into(),
                        total: 3,
                    },
                    flea::protocol::Reply::Rows {
                        start: 0,
                        listing: 50,
                        rows: rows.into(),
                    },
                ] {
                    shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
                }
                shell.set_view(View::Grid, window, cx);
            });
        })
        .unwrap();
    draw(handle, cx);
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, _| {
                let host = shell.panes[0].as_ref().expect("the grid pane");
                assert_eq!(host.drawn_view, Some(View::Grid));
                assert_eq!(host.pane.state.kind, crate::pane::Kind::Ready);
            });
            assert!(crate::window::glass::alpha(window, cx) < 1.);
            let background = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default()
                .background;
            let (opaque, translucent) = background_grounds(window, background);
            assert_eq!(opaque, 0, "the Grid floor must not paint an opaque ground");
            assert!(
                translucent > 0,
                "the Grid floor paints the translucent list ground"
            );
        })
        .unwrap();
    stop(handle, cx);
    fixture.cleanup();
}
