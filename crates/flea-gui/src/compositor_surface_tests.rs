//! Every rounded surface is painted with the same compositor value at zero and eight.
use crate::presentation::frame_gate::theme;
use crate::state::Shell;
use gpui::{
    prelude::*, AppContext, Context, Entity, Quad, Render, Subscription, TestAppContext,
    VisualTestContext, Window,
};

mod chrome;
mod dialogs;
mod network;
mod operations;
mod pictures;
mod settings;

// A divider a pixel thick or less has no room for a corner, so it draws square at any compositor value.
const HAIRLINE: f32 = 1.;

fn painted(cx: &mut VisualTestContext) -> (f32, Vec<Quad>) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.scale_factor(), window.painted_quads())
    })
}

fn frames(
    name: &str,
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    corner: f32,
    checks: &[f32],
) {
    // The first frame is the corner the surface took at mount, before any live update.
    for (index, value) in [corner, 8. - corner, corner].into_iter().enumerate() {
        if index > 0 {
            update(shell, cx, value);
        }
        assert_frames(name, cx, value, checks);
    }
}

fn assert_frames(name: &str, cx: &mut VisualTestContext, corner: f32, checks: &[f32]) {
    let (scale, quads) = painted(cx);
    assert!(!quads.is_empty(), "{name} must paint");
    let mut count = 0;
    let mut wrong_radius = 0;
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
        // A reserved border that is clear over a clear fill draws nothing, so it is not a frame.
        if !widths.iter().all(|width| width.0 > 0.)
            || (quad.border_color.a == 0. && quad.background.is_transparent())
        {
            continue;
        }
        let w = quad.bounds.size.width.0 / scale;
        let h = quad.bounds.size.height.0 / scale;
        let check = checks
            .iter()
            .any(|side| (w - side).abs() <= 0.5 && (h - side).abs() <= 0.5);
        let expected = if w <= HAIRLINE || h <= HAIRLINE {
            0.
        } else if check || h < 24. {
            corner / 2.
        } else {
            corner
        };
        let actual = radii.map(|radius| radius.0 / scale);
        if !actual.iter().all(|radius| (radius - expected).abs() < 0.01) {
            eprintln!("VF_FRAMES violation surface={name} frame={w}x{h} expected_radius={expected} actual={actual:?}");
            wrong_radius += 1;
        }
        count += 1;
    }
    assert_eq!(
        wrong_radius, 0,
        "{name}: every visible card and control frame must follow the compositor"
    );
    assert!(count > 0, "{name} must paint a complete frame");
    let theme = cx.read(|cx| {
        cx.try_global::<crate::palette::Theme>()
            .copied()
            .expect("the ink check needs the distinct test theme that mounted sets")
    });
    crate::presentation::frame_gate::assert_inks(name, &quads, scale, theme);
    println!("M9_CORNER surface={name} corner={corner} frames={count} PASS");
}

struct Mounted<V: Render> {
    shell: Entity<Shell>,
    view: Entity<V>,
    _watch: Subscription,
}

impl<V: Render> Render for Mounted<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div().size_full().child(self.view.clone())
    }
}

fn mounted<V: Render>(
    cx: &mut TestAppContext,
    corner: f32,
    build: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> Entity<V>,
) -> (Entity<Shell>, Entity<V>, &mut VisualTestContext) {
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let (root, cx) = cx.add_window_view(|window, cx| {
        // Frame roles must be distinct, which the default theme's are not.
        cx.set_global(theme());
        let shell = cx.new(|cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.launch.settings.data = flea::uischema::defaults();
            shell
        });
        // The compositor answers before the surface mounts, as in production.
        let view = shell.update(cx, |shell, cx| {
            shell.receive_settings(
                crate::window::settings::store::Update::Corner(corner),
                window,
                cx,
            );
            build(shell, window, cx)
        });
        let watch = cx.observe(&shell, |_, _, cx| cx.notify());
        Mounted {
            shell,
            view,
            _watch: watch,
        }
    });
    let (shell, view) = root.read_with(cx, |root, _| (root.shell.clone(), root.view.clone()));
    (shell, view, cx)
}

fn update(shell: &Entity<Shell>, cx: &mut VisualTestContext, corner: f32) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive_settings(
                crate::window::settings::store::Update::Corner(corner),
                window,
                cx,
            );
        })
    });
}

fn all(cx: &mut TestAppContext, corner: f32) {
    settings::check(cx, corner);
    dialogs::check(cx, corner);
    chrome::check(cx, corner);
    network::check(cx, corner);
}

#[gpui::test]
fn m9_corner_vf_frames_all_surfaces_square(cx: &mut TestAppContext) {
    all(cx, 0.);
}

#[gpui::test]
fn m9_corner_vf_frames_all_surfaces_rounded(cx: &mut TestAppContext) {
    all(cx, 8.);
}

struct ClearBorderOverFill;
impl Render for ClearBorderOverFill {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // A styled div paints its fill and its border as two quads, so only a direct paint carries both.
        gpui::canvas(
            |_, _, _| {},
            |bounds, _, window, _| {
                window.paint_quad(gpui::quad(
                    bounds,
                    gpui::px(3.),
                    theme().foreground,
                    gpui::px(1.),
                    gpui::transparent_black(),
                    gpui::BorderStyle::Solid,
                ))
            },
        )
        .size(gpui::px(100.))
    }
}

#[gpui::test]
#[should_panic(expected = "every visible card and control frame must follow the compositor")]
fn vf_frames_clear_border_over_a_fill_is_still_checked(cx: &mut TestAppContext) {
    let (_, _, cx) = mounted(cx, 8., |_, _, cx| cx.new(|_| ClearBorderOverFill));
    assert_frames("clear-border-over-fill", cx, 8., &[]);
}
