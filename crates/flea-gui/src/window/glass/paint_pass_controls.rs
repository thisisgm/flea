//! Actual paint-pass regressions: surviving quads, removed owners and opaque roots.
use super::*;
use crate::ipc::Adapter;
use crate::window::glass::{
    self,
    producers::{self, Producer},
};
use gpui::{prelude::*, Hsla, WindowBackgroundAppearance, WindowOptions};

#[path = "cached_owner_controls.rs"]
mod cached_owner_controls;

#[derive(Default)]
struct Scene {
    custom: bool,
    unknown: bool,
    rail: bool,
    opaque_body: bool,
}

fn area(owner: Producer, color: Hsla, x: f32, y: f32, w: f32, h: f32) -> gpui::Div {
    gpui::div()
        .absolute()
        .left(gpui::relative(x))
        .top(gpui::relative(y))
        .w(gpui::relative(w))
        .h(gpui::relative(h))
        .child(producers::backdrop(owner, color))
}

impl gpui::Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        let theme = *cx.global::<crate::palette::Theme>();
        let custom = if self.custom {
            theme.foreground
        } else {
            theme.background
        };
        let body = if self.opaque_body {
            theme.foreground
        } else {
            theme.background
        };
        gpui::div()
            .relative()
            .size_full()
            .child(producers::backdrop(Producer::Body, body))
            .child(area(Producer::Chrome, custom, 0., 0., 0.5, 0.2))
            .child(area(Producer::Chrome, theme.background, 0.5, 0., 0.5, 0.2))
            .child(area(Producer::Listing, custom, 0., 0.2, 0.5, 0.7))
            .child(area(
                Producer::Listing,
                theme.background,
                0.5,
                0.2,
                0.5,
                0.7,
            ))
            .child(area(Producer::Status, theme.surface, 0., 0.9, 1., 0.1))
            .when(self.unknown, |div| {
                div.child(area(
                    Producer::Unknown,
                    theme.foreground,
                    0.,
                    0.3,
                    0.25,
                    0.2,
                ))
            })
            .when(self.rail, |div| {
                div.child(area(Producer::Rail, theme.background, 0.75, 0.3, 0.25, 0.2))
            })
    }
}

fn open(scene: Scene, cx: &mut TestAppContext) -> WindowHandle<Scene> {
    cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                glass::mount(window, WindowBackgroundAppearance::Transparent, cx);
                cx.new(|_| scene)
            },
        )
        .unwrap()
    })
}

fn draw_scene<T: gpui::Render>(handle: WindowHandle<T>, cx: &mut TestAppContext) {
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

fn value<T: gpui::Render>(handle: WindowHandle<T>, cx: &mut TestAppContext) -> Json {
    handle
        .update(cx, |_, window, cx| {
            let reply = glass::Reader { window, cx }.call("glass", &[]).unwrap();
            // Sample response: {"alpha":1.0,"listAlpha":1.0,"material":false,"grounds":{"body":255}}
            flea::jsondoc::parse(reply.stdout.trim()).unwrap()
        })
        .unwrap()
}

fn close<T: gpui::Render>(handle: WindowHandle<T>, cx: &mut TestAppContext) {
    handle
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn glass_actual_unregistered_quads_keep_both_simultaneous_owner_floors(cx: &mut TestAppContext) {
    let fixture = install(
        cx,
        "paint-pass-same-owner",
        &[(6, ""), (2, "[flea]\nbackground-alpha=0.34")],
    );
    let handle = open(
        Scene {
            custom: true,
            ..Default::default()
        },
        cx,
    );
    draw_scene(handle, cx);
    handle
        .update(cx, |_, window, cx| {
            let theme = cx.global::<crate::palette::Theme>();
            let quads = window.painted_quads();
            let opaque = quads
                .iter()
                .filter(|q| {
                    q.background.as_solid().is_some_and(|c| {
                        c.a == 1. && c.h == theme.foreground.h && c.l == theme.foreground.l
                    })
                })
                .count();
            let clear = quads
                .iter()
                .filter(|q| {
                    q.background
                        .as_solid()
                        .is_some_and(|c| (c.a - 87. / 255.).abs() < 0.0001)
                })
                .count();
            assert_eq!(
                opaque, 2,
                "the custom Chrome and Listing quads actually survive"
            );
            assert!(
                clear >= 3,
                "later registered sibling quads actually survive too"
            );
        })
        .unwrap();
    let reply = value(handle, cx);
    assert_eq!(
        reply.get("listAlpha").and_then(Json::as_f64),
        Some(1.),
        "{reply:?}"
    );
    assert_eq!(
        reply.get("alpha").and_then(Json::as_f64),
        Some(1.),
        "{reply:?}"
    );
    assert_eq!(
        reply
            .get("grounds")
            .unwrap()
            .get("chrome")
            .and_then(Json::as_f64),
        Some(255.)
    );
    close(handle, cx);
    fixture.cleanup();
}

#[gpui::test]
fn glass_removed_unknown_and_rail_expire_on_the_next_actual_paint_pass_with_same_theme(
    cx: &mut TestAppContext,
) {
    let fixture = install(cx, "paint-pass-retired", &[(6, "")]);
    let handle = open(
        Scene {
            unknown: true,
            rail: true,
            ..Default::default()
        },
        cx,
    );
    handle
        .update(cx, |_, window, cx| glass::rail_overlay(window, true, cx))
        .unwrap();
    draw_scene(handle, cx);
    let before = value(handle, cx);
    assert_eq!(before.get("alpha").and_then(Json::as_f64), Some(1.));
    let theme = cx.update(|cx| *cx.global::<crate::palette::Theme>());
    handle
        .update(cx, |scene, window, cx| {
            assert!(window.painted_quads().iter().any(|q| q
                .background
                .as_solid()
                .is_some_and(|c| c.a == 1. && c.l == theme.foreground.l)));
            scene.unknown = false;
            scene.rail = false;
            glass::rail_overlay(window, false, cx);
            cx.notify();
        })
        .unwrap();
    draw_scene(handle, cx);
    let after = value(handle, cx);
    assert_eq!(cx.update(|cx| *cx.global::<crate::palette::Theme>()), theme);
    let grounds = after.get("grounds").unwrap();
    assert_eq!(grounds.get("unknown"), Some(&Json::Null));
    assert_eq!(grounds.get("rail"), Some(&Json::Null));
    assert!(
        (after.get("alpha").and_then(Json::as_f64).unwrap() - 192. / 255.).abs() < 0.0001,
        "{after:?}"
    );
    handle
        .update(cx, |_, window, _| {
            assert!(!window.painted_quads().iter().any(|q| q
                .background
                .as_solid()
                .is_some_and(|c| c.a == 1. && c.l == theme.foreground.l)));
        })
        .unwrap();
    close(handle, cx);
    fixture.cleanup();
}

#[gpui::test]
fn glass_actual_opaque_body_prevents_a_translucent_material_receipt(cx: &mut TestAppContext) {
    let fixture = install(cx, "paint-pass-opaque-body", &[(6, "")]);
    let handle = open(
        Scene {
            opaque_body: true,
            ..Default::default()
        },
        cx,
    );
    draw_scene(handle, cx);
    handle
        .update(cx, |_, window, cx| {
            let theme = cx.global::<crate::palette::Theme>();
            let width = f32::from(window.viewport_size().width) * window.scale_factor();
            let height = f32::from(window.viewport_size().height) * window.scale_factor();
            assert!(window
                .painted_quads()
                .iter()
                .any(|q| q.bounds.size.width.as_f32() >= width - 1.
                    && q.bounds.size.height.as_f32() >= height - 1.
                    && q.background
                        .as_solid()
                        .is_some_and(|c| c.a == 1. && c.l == theme.foreground.l)));
        })
        .unwrap();
    let reply = value(handle, cx);
    assert_eq!(
        reply
            .get("grounds")
            .unwrap()
            .get("body")
            .and_then(Json::as_f64),
        Some(255.)
    );
    assert_eq!(
        reply.get("alpha").and_then(Json::as_f64),
        Some(1.),
        "{reply:?}"
    );
    assert_eq!(reply.get("material").and_then(Json::as_bool), Some(false));
    close(handle, cx);
    fixture.cleanup();
}

struct CachedScene {
    content: gpui::Entity<Scene>,
}
impl gpui::Render for CachedScene {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div().size_full().child(
            self.content
                .clone()
                .cached(gpui::StyleRefinement::default().size_full()),
        )
    }
}

#[gpui::test]
fn glass_cached_scene_replay_keeps_its_grounds_and_real_child_repaint_retires_an_owner(
    cx: &mut TestAppContext,
) {
    let fixture = install(cx, "paint-pass-cached-retired", &[(6, "")]);
    let handle = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                glass::mount(window, WindowBackgroundAppearance::Transparent, cx);
                let content = cx.new(|_| Scene {
                    unknown: true,
                    ..Default::default()
                });
                cx.new(|_| CachedScene { content })
            },
        )
        .unwrap()
    });
    draw_scene(handle, cx);
    assert_eq!(
        value(handle, cx).get("alpha").and_then(Json::as_f64),
        Some(1.)
    );
    draw_scene(handle, cx);
    assert_eq!(
        value(handle, cx).get("alpha").and_then(Json::as_f64),
        Some(1.),
        "cached scene still contains its actual opaque producer"
    );
    handle
        .update(cx, |root, _, cx| {
            root.content.update(cx, |scene, cx| {
                scene.unknown = false;
                cx.notify();
            })
        })
        .unwrap();
    draw_scene(handle, cx);
    let reply = value(handle, cx);
    assert_eq!(
        reply.get("grounds").unwrap().get("unknown"),
        Some(&Json::Null)
    );
    assert!(
        (reply.get("alpha").and_then(Json::as_f64).unwrap() - 192. / 255.).abs() < 0.0001,
        "{reply:?}"
    );
    close(handle, cx);
    fixture.cleanup();
}
