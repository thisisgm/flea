//! Every rounded surface is painted with the same compositor value at zero and eight.
use crate::state::Shell;
use gpui::{
    prelude::*, AppContext, Context, Entity, Quad, Render, Subscription, TestAppContext,
    VisualTestContext, Window,
};

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

fn frames(
    name: &str,
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    corner: f32,
    checks: &[f32],
) {
    for value in [8. - corner, corner] {
        update(shell, cx, value);
        assert_frames(name, cx, value, checks);
    }
}

fn assert_frames(name: &str, cx: &mut VisualTestContext, corner: f32, checks: &[f32]) {
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
    build: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> Entity<V>,
) -> (Entity<Shell>, Entity<V>, &mut VisualTestContext) {
    let (backend, _requests) = crate::backend::Backend::test_channel();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.launch.settings.data = flea::uischema::defaults();
            shell
        });
        let view = shell.update(cx, |shell, cx| build(shell, window, cx));
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
}

#[gpui::test]
fn m9_corner_all_surfaces_square(cx: &mut TestAppContext) {
    all(cx, 0.);
}

#[gpui::test]
fn m9_corner_all_surfaces_rounded(cx: &mut TestAppContext) {
    all(cx, 8.);
}
