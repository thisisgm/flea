//! The focused Dual pane paints its edge below the path strip, through the column header and rows.
use crate::{
    backend::Backend,
    list::{Metrics, Palette},
    state::{tests::shell_with, Shell},
    window::{composition::Geometry, pane::PaneHost, startup::View},
};
use gpui::{px, size, AppContext, Background, Quad, TestAppContext, WindowHandle};

fn open(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    let (backend, _requests) = Backend::test_channel();
    cx.open_window(size(px(1100.), px(700.)), |window, cx| {
        let mut shell = shell_with(backend, cx);
        let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
        right.pane.held = shell.panes[0].as_ref().unwrap().pane.held.clone();
        shell.panes[1] = Some(right);
        shell.view = View::Dual;
        window.focus(&shell.focus, cx);
        shell
    })
}

fn draw(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
) -> (Vec<Quad>, Geometry, Palette, f32, f32) {
    let quads = cx
        .update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            window.painted_quads()
        })
        .unwrap();
    handle
        .update(cx, |shell, window, cx| {
            let theme = cx
                .try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default();
            (
                quads,
                shell.geometry(window),
                Palette::inherited(&window.text_style(), theme),
                Metrics::for_typeface(&shell.active_typeface()).header,
                window.scale_factor(),
            )
        })
        .unwrap()
}

fn edges(
    quads: &[Quad],
    geometry: &Geometry,
    colour: Background,
    header: f32,
    scale: f32,
) -> Vec<usize> {
    let near = |a: f32, b: f32| (a - b).abs() < 0.1;
    geometry
        .panes
        .iter()
        .enumerate()
        .filter_map(|(side, pane)| {
            quads
                .iter()
                .any(|quad| {
                    quad.background == colour
                        && near(quad.bounds.origin.x.0 / scale, pane.x)
                        && near(quad.bounds.origin.y.0 / scale, pane.y + header)
                        && near(quad.bounds.size.width.0 / scale, 2.)
                        && near(quad.bounds.size.height.0 / scale, pane.height - header)
                })
                .then_some(side)
        })
        .collect()
}

#[gpui::test]
fn owner_parity_dual_edge_tracks_focus_and_leaves_single_pane(cx: &mut TestAppContext) {
    let handle = open(cx);
    for side in [0, 1, 0] {
        handle
            .update(cx, |shell, _, cx| shell.focus_pane(side, cx))
            .unwrap();
        let (quads, geometry, palette, header, scale) = draw(cx, handle);
        assert_eq!(
            edges(
                &quads,
                &geometry,
                palette.accent_frame.into(),
                header,
                scale
            ),
            [side],
            "only the focused pane has a two-pixel edge below its path strip through its column header and rows"
        );
        let divider = geometry.panes[0].x + geometry.panes[0].width;
        assert!(
            quads.iter().any(|quad| {
                quad.background == Background::from(palette.muted_frame)
                    && (quad.bounds.origin.x.0 / scale - divider).abs() < 0.1
                    && (quad.bounds.size.width.0 / scale - 1.).abs() < 0.1
                    && (quad.bounds.size.height.0 / scale - geometry.panes[0].height).abs() < 0.1
            }),
            "the centre divider remains independent of pane focus"
        );
    }
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::List, window, cx)
        })
        .unwrap();
    let (quads, geometry, palette, header, scale) = draw(cx, handle);
    assert!(edges(
        &quads,
        &geometry,
        palette.accent_frame.into(),
        header,
        scale
    )
    .is_empty());
}

struct Cover;

impl gpui::Render for Cover {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

#[gpui::test]
fn owner_parity_dual_edge_hides_under_cover_and_for_absent_pane(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |shell, _, cx| {
            shell.slots.covers[0] = Some(cx.new(|_| Cover).into());
        })
        .unwrap();
    let (quads, geometry, palette, header, scale) = draw(cx, handle);
    assert!(edges(
        &quads,
        &geometry,
        palette.accent_frame.into(),
        header,
        scale
    )
    .is_empty());
    handle
        .update(cx, |shell, _, _| {
            shell.slots.covers[0] = None;
            shell.panes[0] = None;
            assert!(shell.dual_edge(&geometry, palette).is_none());
        })
        .unwrap();
}

fn overlay_ground() -> gpui::Hsla {
    crate::palette::Theme::from_text("background='#123456'").background
}

struct OverlayGround;

impl gpui::Render for OverlayGround {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::Styled;
        gpui::div().size_full().bg(overlay_ground())
    }
}

#[gpui::test]
fn the_focused_dual_edge_paints_below_an_overlapping_overlay_rail(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |shell, _, cx| {
            shell.launch.settings = crate::window::startup::Settings::from_json(
                flea::jsondoc::parse(r#"{"places":{"autoHide":true}}"#).unwrap(),
            );
            shell.slots.rail = Some(cx.new(|_| OverlayGround).into());
        })
        .unwrap();
    for side in [0, 1] {
        handle
            .update(cx, |shell, _, cx| shell.focus_pane(side, cx))
            .unwrap();
        let (quads, geometry, palette, header, scale) = draw(cx, handle);
        assert!(geometry.rail_overlay, "the rail must overlap the panes");
        assert_eq!(
            edges(
                &quads,
                &geometry,
                palette.accent_frame.into(),
                header,
                scale
            ),
            [side],
            "the focused pane keeps its complete edge below the rail"
        );
        let edge = quads
            .iter()
            .find(|quad| {
                quad.background == Background::from(palette.accent_frame)
                    && (quad.bounds.size.height.0 / scale - (geometry.panes[side].height - header))
                        .abs()
                        < 0.1
            })
            .expect("the full-pane edge is painted");
        let ground = quads
            .iter()
            .find(|quad| quad.background == Background::from(overlay_ground()))
            .expect("the overlay rail paints its ground");
        assert!(
            edge.bounds.intersects(&ground.bounds),
            "the stacking assertion must cover an overlap"
        );
        assert!(
            edge.order < ground.order,
            "the overlay ground must cover the pane edge"
        );
    }
}
