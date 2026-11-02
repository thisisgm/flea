//! An independently cached ground must survive fresh sibling paints until it is removed.
use super::*;
use crate::window::glass::{
    self,
    producers::{self, Producer},
};

struct CachedOwner(bool);

impl gpui::Render for CachedOwner {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div().size_full().when(self.0, |div| {
            div.child(producers::backdrop(
                Producer::Unknown,
                cx.global::<crate::palette::Theme>().foreground,
            ))
        })
    }
}

struct MixedScene {
    fresh: gpui::Entity<Scene>,
    cached: gpui::Entity<CachedOwner>,
}

impl gpui::Render for MixedScene {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div()
            .relative()
            .size_full()
            .child(self.fresh.clone())
            .child(
                self.cached.clone().cached(
                    gpui::StyleRefinement::default()
                        .absolute()
                        .top(gpui::px(0.))
                        .left(gpui::px(0.))
                        .size_full(),
                ),
            )
    }
}

#[gpui::test]
fn glass_cached_owner_survives_uncached_sibling_then_retires(cx: &mut TestAppContext) {
    let fixture = install(cx, "paint-pass-mixed-cache", &[(6, "")]);
    let handle = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                glass::mount(window, WindowBackgroundAppearance::Transparent, cx);
                let fresh = cx.new(|_| Scene::default());
                let cached = cx.new(|_| CachedOwner(true));
                cx.new(|_| MixedScene { fresh, cached })
            },
        )
        .unwrap()
    });
    handle
        .update(cx, |root, _, cx| {
            root.cached.update(cx, |_, cx| cx.notify());
        })
        .unwrap();
    draw_scene(handle, cx);
    draw_scene(handle, cx);
    handle
        .update(cx, |_, window, cx| {
            let ink = cx.global::<crate::palette::Theme>().foreground;
            assert!(
                window.painted_quads().iter().any(|q| q
                    .background
                    .as_solid()
                    .is_some_and(|color| color.a == 1. && color.l == ink.l)),
                "cached opaque quad survives sibling repaint"
            );
        })
        .unwrap();
    let mixed = value(handle, cx);
    assert_eq!(
        mixed.get("alpha").and_then(Json::as_f64),
        Some(1.),
        "{mixed:?}"
    );
    assert_eq!(mixed.get("material").and_then(Json::as_bool), Some(false));
    handle
        .update(cx, |root, _, cx| {
            root.cached.update(cx, |cached, cx| {
                cached.0 = false;
                cx.notify();
            });
        })
        .unwrap();
    draw_scene(handle, cx);
    let removed = value(handle, cx);
    assert_eq!(
        removed.get("grounds").unwrap().get("unknown"),
        Some(&Json::Null)
    );
    assert_eq!(
        (removed.get("alpha").and_then(Json::as_f64).unwrap() * 255.).round() as u8,
        192
    );
    close(handle, cx);
    fixture.cleanup();
}
