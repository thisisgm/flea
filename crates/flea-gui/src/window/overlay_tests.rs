//! The operations overlay draws over the window under its scrim once opened, and never builds at startup.
use crate::{
    backend::Backend,
    g2::file_ops::model::Dialog,
    g2::settings_pages::seams::COMPOSITOR_CORNER_DEFAULT,
    state::{tests::shell_with, Shell},
    visual::contract::Spec,
    window::{
        locale::Snapshot,
        root::Root,
        startup::{Arguments, Launch, Settings, View},
    },
};
use gpui::{
    AppContext, Bounds, Pixels, Quad, TestAppContext, VisualTestContext, Window, WindowHandle,
};
use std::sync::Arc;

#[path = "gap64_overlay_tests.rs"]
mod gap64;

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Root> {
    cx.add_window(|_, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        Root { shell }
    })
}

// The window through its real startup constructor, as `window::open` builds it.
fn open_startup(cx: &mut TestAppContext) -> WindowHandle<Root> {
    // The real constructor starts the backend thread, whose failure lands from that thread.
    cx.executor().allow_parking();
    cx.add_window(|window, cx| {
        let (_events_tx, events) = async_channel::bounded(1);
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let settings = Settings::from_json(flea::jsondoc::parse("{}").unwrap());
        let launch = Launch::resolve(
            settings,
            Arguments {
                path: Some("/files".into()),
                select: None,
            },
            "/home/test",
        );
        let shell = cx.new(|cx| Shell::new(launch, locale, events, window, cx));
        Root { shell }
    })
}

fn shell<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .clone()
                .update(cx, |shell, cx| update(shell, window, cx))
        })
        .expect("the window is open")
}

// The window's painted quads for one whole frame, after the root has drawn it.
fn frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Vec<Quad> {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    })
    .expect("the window is open")
}

fn same_frame(before: &[Quad], after: &[Quad]) -> bool {
    format!("{before:?}") == format!("{after:?}")
}

// The laid-out bounds of one tagged part, read from the frame the window drew.
fn marks(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    selector: &'static str,
) -> Option<Bounds<Pixels>> {
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.debug_bounds(selector)
}

fn viewport(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> gpui::Size<Pixels> {
    cx.update_window(handle.into(), |_, window, _| window.viewport_size())
        .expect("the window is open")
}

fn resize(cx: &mut TestAppContext, handle: WindowHandle<Root>, width: f32, height: f32) {
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
}

fn open_copy_to(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    shell(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/fx/v/work", window, cx);
    });
}

fn inside(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> bool {
    inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

#[gpui::test]
fn a_window_that_opens_nothing_builds_no_overlay(cx: &mut TestAppContext) {
    let handle = open_startup(cx);
    frame(cx, handle);
    let overlay = shell(cx, handle, |shell, _, _| shell.overlay.is_some());
    assert!(!overlay, "startup builds no overlay card");
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "no scrim draws before the first open"
    );
}

#[gpui::test]
fn copy_to_paints_its_scrim_and_card_over_the_window(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    let own = frame(cx, handle);
    open_copy_to(cx, handle);
    let opened = frame(cx, handle);
    let view = viewport(cx, handle);
    let scrim = marks(cx, handle, "op1-scrim").expect("the open card dims the window");
    assert_eq!(
        f32::from(scrim.origin.x),
        0.,
        "the scrim starts at the window left"
    );
    assert_eq!(
        f32::from(scrim.origin.y),
        0.,
        "the scrim starts at the window top"
    );
    assert_eq!(
        f32::from(scrim.size.width),
        f32::from(view.width),
        "the scrim spans the window"
    );
    assert_eq!(
        f32::from(scrim.size.height),
        f32::from(view.height),
        "the scrim spans the window"
    );
    let (ink, scale) = shell(cx, handle, |_, window, cx| {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        (
            theme
                .background
                .opacity(flea::ui_logic_file_ops::card::scrim_alpha(0)),
            window.scale_factor(),
        )
    });
    assert!(ink.a > 0., "the theme's scrim fill is visible");
    let painted = scrim.scale(scale);
    assert!(
        opened.iter().any(|quad| {
            quad.bounds == painted
                && quad.bounds.intersect(&quad.content_mask.bounds) == painted
                && quad.background.as_solid().is_some_and(|fill| {
                    (fill.h, fill.s, fill.l, fill.a) == (ink.h, ink.s, ink.l, ink.a)
                })
        }),
        "the open frame paints the non-transparent theme scrim over the whole window"
    );
    let card = marks(cx, handle, "op1-card").expect("the open card draws centred");
    assert!(
        inside(scrim, card)
            && f32::from(card.right()) < f32::from(view.width)
            && f32::from(card.bottom()) < f32::from(view.height),
        "the card at {card:?} sits inside the viewport {view:?}"
    );
    shell(cx, handle, |shell, window, cx| {
        shell.close_overlay(window, cx)
    });
    let back = frame(cx, handle);
    assert!(
        same_frame(&back, &own),
        "a closed card returns the window's own frame"
    );
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "the scrim leaves with the card"
    );
}

#[gpui::test]
fn the_card_stays_inside_the_viewport_at_both_window_sizes(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    open_copy_to(cx, handle);
    for (width, height) in [(1100., 700.), (1920., 1080.)] {
        resize(cx, handle, width, height);
        frame(cx, handle);
        let view = viewport(cx, handle);
        let card = marks(cx, handle, "op1-card").expect("the open card draws");
        assert!(
            f32::from(card.origin.x) >= 0.
                && f32::from(card.origin.y) >= 0.
                && f32::from(card.right()) <= f32::from(view.width)
                && f32::from(card.bottom()) <= f32::from(view.height),
            "the card at {card:?} sits inside the {width}x{height} viewport {view:?}"
        );
    }
}

#[gpui::test]
fn the_copy_to_scene_opens_through_the_scene_registry(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    // The compiled registry resolves the scene the op1 manifest registers.
    let spec = Spec::parse(
        &std::process::id().to_string(),
        "g2.op1.copyto.r8",
        r#"{"path":"/fx/v/work"}"#,
    )
    .expect("the copy_to scene is registered");
    assert_eq!(spec.view, View::List);
    let (answer, _receive) = std::sync::mpsc::sync_channel(1);
    shell(cx, handle, |shell, window, cx| {
        shell.apply_scene(spec, answer, window, cx);
    });
    frame(cx, handle);
    assert!(
        marks(cx, handle, "op1-scrim").is_some(),
        "the scene setup opens the card on the overlay slot"
    );
    assert!(
        marks(cx, handle, "op1-card").is_some(),
        "the scene draws the copy card"
    );
}

#[gpui::test]
fn a_later_list_scene_closes_the_card(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    apply_named_scene(cx, handle, "g2.op1.copyto.r8");
    frame(cx, handle);
    assert!(
        marks(cx, handle, "op1-scrim").is_some(),
        "the copy_to scene opens the card"
    );
    apply_named_scene(cx, handle, "g1-01-list-r8");
    frame(cx, handle);
    assert_eq!(
        marks(cx, handle, "op1-scrim"),
        None,
        "a later scene starts with no card open"
    );
}

fn apply_named_scene(cx: &mut TestAppContext, handle: WindowHandle<Root>, name: &str) {
    let spec = Spec::parse(
        &std::process::id().to_string(),
        name,
        r#"{"path":"/fx/v/work"}"#,
    )
    .expect("the scene is registered");
    let (answer, _receive) = std::sync::mpsc::sync_channel(1);
    shell(cx, handle, |shell, window, cx| {
        shell.apply_scene(spec, answer, window, cx);
    });
}

#[gpui::test]
fn rev_r4_every_registered_copy_to_id_reaches_apply_scene(cx: &mut TestAppContext) {
    let pid = std::process::id().to_string();
    for (name, corner, path) in [
        ("g2.op1.copyto.r8", 8., "/fx/v/work"),
        ("g2.op1.copyto.r0", 0., "/fx/v/work"),
        ("g2.op1.copyto-empty.r8", 8., ""),
        ("g2.op1.copyto-empty.r0", 0., ""),
    ] {
        let (backend, _receiver) = Backend::test_channel();
        let handle = open(cx, backend);
        frame(cx, handle);
        let spec = Spec::parse(&pid, name, r#"{"path":"/fx/v/work"}"#).expect("registered");
        assert_eq!(
            spec.corner, corner as u32,
            "the registered {name} carries corner {corner}"
        );
        let (answer, _receive) = std::sync::mpsc::sync_channel(1);
        shell(cx, handle, |shell, window, cx| {
            shell.apply_scene(spec, answer, window, cx)
        });
        frame(cx, handle);
        let card = marks(cx, handle, "op1-card");
        let scale = shell(cx, handle, |_, window, _| window.scale_factor());
        let quads = frame(cx, handle);
        let radius = card
            .and_then(|card| largest_inside(&quads, card, scale))
            .map(|quad| quad.corner_radii.top_left.0 / scale);
        let field = shell(cx, handle, |shell, _, cx| {
            shell.overlay.as_ref().and_then(|overlay| {
                overlay.read_with(cx, |card, _| match &card.model.dialog {
                    Some(Dialog::CopyTo { path, .. }) => Some(path.clone()),
                    _ => None,
                })
            })
        });
        assert!(card.is_some(), "the {name} scene draws its card");
        assert_eq!(
            radius,
            Some(corner),
            "the {name} scene draws its registered radius"
        );
        assert_eq!(
            field.as_deref(),
            Some(path),
            "the {name} scene fills its field"
        );
    }
}

#[gpui::test]
fn the_default_scene_draws_the_seam_radius(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    // No scene drives the window, so the card takes the shared compositor seam default.
    open_copy_to(cx, handle);
    frame(cx, handle);
    let card = marks(cx, handle, "op1-card").expect("the open card draws");
    let scale = shell(cx, handle, |_, window, _| window.scale_factor());
    let quads = frame(cx, handle);
    let surface = largest_inside(&quads, card, scale).expect("the card paints its surface");
    let drawn = surface.corner_radii.top_left.0 / scale;
    assert!(
        (drawn - COMPOSITOR_CORNER_DEFAULT).abs() < 0.05,
        "the default card draws the seam radius, drew {drawn}"
    );
}

fn largest_inside(quads: &[Quad], card: Bounds<Pixels>, scale: f32) -> Option<Quad> {
    quads
        .iter()
        .filter(|quad| {
            let bounds = quad.bounds;
            bounds.origin.x.0 / scale >= f32::from(card.origin.x) - 1.
                && bounds.origin.y.0 / scale >= f32::from(card.origin.y) - 1.
                && (bounds.origin.x.0 + bounds.size.width.0) / scale
                    <= f32::from(card.origin.x) + f32::from(card.size.width) + 1.
                && (bounds.origin.y.0 + bounds.size.height.0) / scale
                    <= f32::from(card.origin.y) + f32::from(card.size.height) + 1.
        })
        .max_by(|a, b| {
            (a.bounds.size.width.0 * a.bounds.size.height.0)
                .partial_cmp(&(b.bounds.size.width.0 * b.bounds.size.height.0))
                .expect("quad areas order")
        })
        .copied()
}
