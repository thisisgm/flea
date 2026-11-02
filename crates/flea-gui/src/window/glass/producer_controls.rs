//! Product controls inspect painted quads and actual producer samples, never the request alone.
use super::*;
use crate::window::glass::{
    self,
    producers::{self, Producer},
};
use gpui::prelude::*;

struct UnknownOwner;
impl gpui::Render for UnknownOwner {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        let background = cx.global::<crate::palette::Theme>().background;
        gpui::div().size_full().children(
            [
                Producer::Body,
                Producer::Chrome,
                Producer::Status,
                Producer::Listing,
                Producer::Unknown,
            ]
            .map(|p| producers::backdrop(p, background)),
        )
    }
}

#[gpui::test]
fn glass_unknown_paint_cannot_be_reported_as_the_requested_alpha(cx: &mut TestAppContext) {
    let fixture = install(cx, "ground-product-unknown-painted", &[(6, "")]);
    let handle = cx.update(|cx| {
        cx.open_window(
            gpui::WindowOptions {
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                glass::mount(window, gpui::WindowBackgroundAppearance::Transparent, cx);
                cx.new(|_| UnknownOwner)
            },
        )
        .unwrap()
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    handle
        .update(cx, |_, window, cx| {
            assert_eq!(
                producers::observed(window, cx).0[Producer::Unknown as usize],
                Some(255)
            );
            assert_eq!(glass::alpha(window, cx), 1.);
            assert!(window
                .painted_quads()
                .iter()
                .any(|q| q.background.as_solid().is_some_and(|c| c.a == 1.)));
            window.remove_window();
        })
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(glass::producer_window_count(cx), 0));
    fixture.cleanup();
}

#[gpui::test]
fn glass_paint_the_material_and_preserve_independent_floors(cx: &mut TestAppContext) {
    let fixture = install(cx, "ground-product-painted", &[(6, "")]);
    let handle = launch(cx);
    draw(handle, cx);
    let value = glass_value(handle, cx);
    assert_eq!(
        value.get("material").and_then(Json::as_bool),
        Some(true),
        "{value:?}"
    );
    assert!((value.get("alpha").and_then(Json::as_f64).unwrap() - 192. / 255.).abs() < 0.0001);
    let grounds = value.get("grounds").unwrap();
    assert_eq!(grounds.get("body").and_then(Json::as_f64), Some(0.));
    assert_eq!(grounds.get("chrome").and_then(Json::as_f64), Some(192.));
    assert_eq!(grounds.get("status").and_then(Json::as_f64), Some(192.));
    assert_eq!(grounds.get("header"), grounds.get("listing"));
    assert_eq!(
        (value.get("listAlpha").and_then(Json::as_f64).unwrap() * 255.).round(),
        (value
            .get("requestedListAlpha")
            .and_then(Json::as_f64)
            .unwrap()
            * 255.)
            .round()
    );
    handle
        .update(cx, |_, window, _| {
            let width = f32::from(window.viewport_size().width);
            let height = f32::from(window.viewport_size().height);
            assert!(window
                .painted_quads()
                .iter()
                .any(|q| q.bounds.size.width.as_f32() >= width - 1.
                    && q.background
                        .as_solid()
                        .is_some_and(|c| (c.a - 192. / 255.).abs() < 0.0001)));
            assert!(!window
                .painted_quads()
                .iter()
                .any(|q| q.bounds.size.width.as_f32() >= width - 1.
                    && q.bounds.size.height.as_f32() >= height - 1.
                    && q.background.as_solid().is_some_and(|c| c.a == 1.)));
        })
        .unwrap();
    stop(handle, cx);
    fixture.cleanup();
}

#[gpui::test]
fn glass_unknown_and_inherited_grounds_are_opaque(cx: &mut TestAppContext) {
    let fixture = install(cx, "ground-product-unknown", &[(6, "")]);
    let handle = launch(cx);
    handle
        .update(cx, |_, window, cx| {
            let theme = cx.global::<crate::palette::Theme>();
            assert_eq!(
                producers::color(Producer::Unknown, theme.background, window, cx).a,
                1.
            );
            assert_ne!(theme.foreground, theme.background);
            assert_ne!(theme.foreground, theme.surface);
            assert_ne!(theme.foreground, theme.chrome());
            assert_eq!(
                producers::color(Producer::Listing, theme.foreground, window, cx).a,
                1.
            );
            assert_eq!(
                producers::color(Producer::Chrome, theme.foreground, window, cx).a,
                1.
            );
        })
        .unwrap();
    stop(handle, cx);
    fixture.cleanup();
}

#[gpui::test]
fn glass_mode_changes_reset_samples_and_windows_do_not_share_them(cx: &mut TestAppContext) {
    let fixture = install(cx, "ground-product-windows", &[(6, "")]);
    let first = launch(cx);
    let second = launch(cx);
    cx.update(|cx| assert_eq!(glass::producer_window_count(cx), 2));
    draw(first, cx);
    assert_eq!(
        glass_value(first, cx)
            .get("groundReady")
            .and_then(Json::as_bool),
        Some(true)
    );
    first
        .update(cx, |_, window, cx| {
            let mut theme = *cx.global::<crate::palette::Theme>();
            theme.glass.mode = crate::window::glass_tag::Mode::Opaque;
            theme.glass.wanted_alpha = 255;
            cx.set_global(theme);
            assert!(
                !producers::observed(window, cx).1,
                "a previous mode's paint samples cannot certify this mode"
            );
        })
        .unwrap();
    draw(first, cx);
    assert_eq!(
        glass_value(first, cx).get("alpha").and_then(Json::as_f64),
        Some(1.)
    );
    first
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, _| shell.stop_for_app_quit());
            window.remove_window();
        })
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(glass::producer_window_count(cx), 1));
    draw(second, cx);
    assert_eq!(
        glass_value(second, cx)
            .get("groundReady")
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        glass_value(second, cx).get("alpha").and_then(Json::as_f64),
        Some(1.)
    );
    stop(second, cx);
    fixture.cleanup();
}

#[gpui::test]
fn glass_dual_path_paints_once_and_retires_on_view_change(cx: &mut TestAppContext) {
    let fixture = install(cx, "ground-product-dual-path", &[(6, "")]);
    let handle = launch(cx);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.set_view(View::Dual, window, cx);
                let path = shell.panes[1]
                    .as_ref()
                    .expect("requested secondary")
                    .path
                    .clone();
                shell.receive(
                    1,
                    crate::backend::Event::Reply(flea::protocol::Reply::Listed { path, total: 0 }),
                    window,
                    cx,
                );
                shell.receive(
                    1,
                    crate::backend::Event::Reply(flea::protocol::Reply::Rows {
                        start: 0,
                        rows: vec![],
                        listing: 2,
                    }),
                    window,
                    cx,
                );
                assert_eq!(
                    shell.view,
                    View::Dual,
                    "a completed secondary activates the real Dual layout"
                );
                shell.sync_chrome(window, cx);
            });
        })
        .unwrap();
    draw(handle, cx);
    handle
        .update(cx, |root, window, cx| {
            let bounds = root.shell.read_with(cx, |shell, cx| {
                let pane = shell.chrome.panes[0]
                    .as_ref()
                    .expect("the actual Dual pane path")
                    .read(cx);
                pane.path.read(cx).paint.borrow().strips[1].expect("painted path bounds")
            });
            let scale = window.scale_factor();
            let same = |a: f32, b: f32| (a - b * scale).abs() < 1.;
            let painted = window.painted_quads();
            let quads: Vec<_> = painted
                .iter()
                .filter(|q| {
                    same(q.bounds.origin.x.as_f32(), bounds.origin.x.as_f32())
                        && same(q.bounds.origin.y.as_f32(), bounds.origin.y.as_f32())
                        && same(q.bounds.size.width.as_f32(), bounds.size.width.as_f32())
                        && same(q.bounds.size.height.as_f32(), bounds.size.height.as_f32())
                })
                .collect();
            assert_eq!(
                quads.len(),
                1,
                "the path owner must not sit on a duplicate host ground"
            );
            assert!(quads[0]
                .background
                .as_solid()
                .is_some_and(|c| (c.a - 192. / 255.).abs() < 0.0001));
            assert_eq!(
                producers::observed(window, cx).0[Producer::Path as usize],
                Some(192)
            );
            root.shell
                .update(cx, |shell, cx| shell.set_view(View::List, window, cx));
        })
        .unwrap();
    draw(handle, cx);
    handle
        .update(cx, |_, window, cx| {
            assert_eq!(
                producers::observed(window, cx).0[Producer::Path as usize],
                None
            );
        })
        .unwrap();
    stop(handle, cx);
    fixture.cleanup();
}
