//! Regression proof through the shipped root, cached shell and upstream Omi geometry.
use super::hero_tests::{bounds, frame, run};
use crate::{
    backend::Backend,
    palette::Theme,
    pane::states::Kind,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use gpui::{
    px, size, AppContext, Bounds, Context, Hsla, Pixels, Quad, TestAppContext, Window, WindowHandle,
};

pub(super) fn edit<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| update(shell, window, cx))
        })
        .unwrap()
}

pub(super) fn open(cx: &mut TestAppContext, dual: bool) -> WindowHandle<Root> {
    let (backend, requests) = Backend::test_channel();
    let (spare, spare_requests) = Backend::test_channel();
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        let shell = cx.new(|cx| {
            cx.on_release(move |_, _| drop((requests, spare_requests)))
                .detach();
            let mut shell = shell_with(backend, cx);
            if dual {
                shell.panes[1] = shell_with(spare, cx).panes[0].take();
                shell.view = View::Dual;
            }
            for host in shell.panes.iter_mut().flatten() {
                host.pane.state.kind = Kind::Empty;
            }
            shell
        });
        shell.update(cx, |shell, cx| shell.follow_frame_feedback(window, cx));
        Root { shell }
    });
    cx.simulate_window_resize(handle.into(), size(px(1100.), px(700.)));
    cx.simulate_window_scale_factor_change(handle.into(), 1.);
    frame(cx, handle);
    handle
}

fn mark_quads(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
) -> (Bounds<Pixels>, Vec<Quad>, f32) {
    let slot = bounds(cx, handle, &["fb1-hero-mark"])[0];
    handle
        .update(cx, |_, window, _| {
            let scale = window.scale_factor();
            let quads = window
                .painted_quads()
                .into_iter()
                .filter(|quad| quad.bounds.is_contained_within(&slot.scale(scale)))
                .collect();
            (slot, quads, scale)
        })
        .unwrap()
}

// Compare the actual scene against the upstream kit, with the approved device size supplied independently.
fn matches_rest(actual: Vec<Quad>, slot: Bounds<Pixels>, scale: f32, side: f32, ink: Hsla) -> bool {
    let kit = omi::Kit::bundled();
    let player = omi::Player::with_options(
        &kit,
        omi::Options {
            mode: Some("thinking".into()),
            body_motion: side >= 44.,
            ..Default::default()
        },
    );
    matches_rects(
        actual,
        slot,
        scale,
        side,
        ink,
        kit.view,
        player.mode_rects("thinking", 0.).unwrap(),
    )
}

fn matches_rects(
    mut actual: Vec<Quad>,
    slot: Bounds<Pixels>,
    scale: f32,
    side: f32,
    ink: Hsla,
    view: [f64; 4],
    rects: Vec<omi::Rect>,
) -> bool {
    let left = (f32::from(slot.origin.x) * scale
        + (f32::from(slot.size.width) * scale - side) / 2.)
        .round();
    let top = (f32::from(slot.origin.y) * scale
        + (f32::from(slot.size.height) * scale - side) / 2.)
        .round();
    let unit = f64::from(side) / view[2];
    for rect in rects {
        let x = |x: f64| (f64::from(left) + (x - view[0]) * unit).round() as f32;
        let y = |y: f64| (f64::from(top) + (y - view[1]) * unit).round() as f32;
        let expected = [x(rect.x), y(rect.y), x(rect.x + rect.w), y(rect.y + rect.h)];
        if expected[0] == expected[2] || expected[1] == expected[3] {
            continue;
        }
        let Some(index) = actual.iter().position(|quad| {
            let edges = [
                quad.bounds.origin.x.0,
                quad.bounds.origin.y.0,
                quad.bounds.right().0,
                quad.bounds.bottom().0,
            ];
            edges
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() < 0.001)
                && quad.background.as_solid().is_some_and(|color| {
                    (color.h - ink.h).abs() < 0.00001
                        && (color.s - ink.s).abs() < 0.00001
                        && (color.l - ink.l).abs() < 0.00001
                        && (color.a - ink.a * rect.opacity as f32).abs() < 0.00001
                })
        }) else {
            return false;
        };
        actual.remove(index);
    }
    actual.is_empty()
}

#[gpui::test]
fn empty_omi_paints_upstream_thinking_rest_at_device_scales_and_theme_accents(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_reduce_motion(true));
    for accent in [
        Theme::from_text(r##"accent="#7aa2f7""##).accent,
        Theme::from_text(r##"accent="#e0af68""##).accent,
    ] {
        cx.update(|cx| {
            cx.set_global(Theme {
                accent,
                ..Theme::default()
            })
        });
        let handle = open(cx, false);
        for (scale, side) in [(1., 66.), (1.25, 66.), (1.5, 88.), (2., 132.)] {
            cx.simulate_window_scale_factor_change(handle.into(), scale);
            frame(cx, handle);
            let (slot, quads, actual_scale) = mark_quads(cx, handle);
            assert_eq!(actual_scale, scale);
            assert!(
                (f32::from(slot.size.width) * scale - 66. * scale).abs() <= 1.
                    && slot.size.width == slot.size.height,
                "approved 66 px hero slot: {slot:?}"
            );
            assert!(
                !quads.is_empty(),
                "empty Omi must paint actual upstream quads"
            );
            assert!(
                matches_rest(quads.clone(), slot, scale, side, accent),
                "scale {scale}: wrong Omi geometry or accent"
            );
            let mut missing = quads.clone();
            missing.remove(0);
            assert!(!matches_rest(missing, slot, scale, side, accent));
            let mut extra = quads.clone();
            extra.push(quads[0]);
            assert!(!matches_rest(extra, slot, scale, side, accent));
            for quad in quads {
                for edge in [
                    quad.bounds.origin.x.0,
                    quad.bounds.origin.y.0,
                    quad.bounds.right().0,
                    quad.bounds.bottom().0,
                ] {
                    assert!((edge - edge.round()).abs() < 0.001);
                }
            }
            assert_eq!(
                run(cx, handle, 1024),
                0,
                "reduced motion requests no frames"
            );
        }
    }
}

#[test]
fn upstream_frame_clock_is_not_a_mode_loop_phase() {
    let mut player = omi::Player::with_options(
        &omi::Kit::bundled(),
        omi::Options {
            mode: Some("thinking".into()),
            ..Default::default()
        },
    );
    player.frame(0.);
    player.frame(1000.);
    assert_eq!(player.rects(), player.mode_rects("thinking", 0.1).unwrap());
    assert_ne!(player.rects(), player.mode_rects("thinking", 1.).unwrap());
}

pub(super) fn sample_players(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    players: &mut [omi::Player; 2],
) {
    edit(cx, handle, |shell, _, cx| {
        for (side, player) in players.iter_mut().enumerate() {
            let mark = &shell.surfaces[side].hero.as_ref().unwrap().read(cx).omi;
            assert!(mark.counters.writes.get() > 0, "each Omi has sampled");
            player.frame(f64::from(mark.offset()) * 1000.);
        }
    });
}

// Each hero retains its own painted layout; the shared static selector names the last slot.
pub(super) fn dual_scenes(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    players: &[omi::Player; 2],
) -> [String; 2] {
    let slots = edit(cx, handle, |shell, _, cx| {
        [0, 1].map(|side| {
            let hero = shell.surfaces[side].hero.as_ref().unwrap().read(cx);
            let slot = hero.painted.get("mark").expect("each hero is drawn").bounds;
            assert_eq!(slot, hero.omi.slot(), "Omi occupies its own hero slot");
            (
                slot,
                hero.omi.offset(),
                hero.omi.ink(),
                hero.omi.counters.writes.get(),
            )
        })
    });
    assert!(
        slots[0].0.right() <= slots[1].0.left(),
        "Dual slots are distinct"
    );
    let tagged = bounds(cx, handle, &["fb1-hero-mark"])[0];
    assert!(slots.iter().any(|slot| slot.0 == tagged));
    handle
        .update(cx, |_, window, _| {
            let scale = window.scale_factor();
            assert_eq!(scale, 1., "Dual fixture owns its native scale");
            let quads = window.painted_quads();
            std::array::from_fn(|side| {
                let (slot, seconds, ink, writes) = slots[side];
                let player = &players[side];
                let painted: Vec<_> = quads
                    .iter()
                    .copied()
                    .filter(|quad| quad.bounds.is_contained_within(&slot.scale(scale)))
                    .collect();
                assert!(!painted.is_empty(), "each Dual Omi must paint");
                let pure = player.mode_rects("thinking", f64::from(seconds)).unwrap();
                assert!(
                    matches_rects(painted.clone(), slot, scale, 66., ink,
                        player.kit().view, player.rects()),
                    "each Dual slot paints upstream thinking geometry: side={side}, elapsed={seconds}, samples={writes}, slot={slot:?}, ink={ink:?}, scale={scale}, quads={painted:?}, upstream={:?}, pure_matches_replay={}",
                    player.rects(), pure == player.rects()
                );
                format!("{painted:?}")
            })
        })
        .unwrap()
}

#[gpui::test]
fn empty_omi_hide_and_hidden_window_stop_paint_and_frame_requests(cx: &mut TestAppContext) {
    let handle = open(cx, false);
    run(cx, handle, 11000);
    cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Hidden);
    frame(cx, handle);
    assert_eq!(run(cx, handle, 1024), 0);
    cx.simulate_window_visibility_change(handle.into(), gpui::WindowVisibility::Visible);
    frame(cx, handle);
    assert!(run(cx, handle, 64) > 0, "visible empty Omi resumes");
    let (slot, _, scale) = mark_quads(cx, handle);
    edit(cx, handle, |shell, _, cx| {
        shell.panes[0].as_mut().unwrap().pane.state.kind = Kind::Ready;
        cx.notify();
    });
    frame(cx, handle);
    frame(cx, handle);
    assert_eq!(run(cx, handle, 1024), 0);
    assert!(gpui::VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("fb1-hero-mark")
        .is_none());
    let accent = Theme::default().accent;
    handle
        .update(cx, |_, window, _| {
            assert!(!window.painted_quads().iter().any(|quad| quad
                .bounds
                .is_contained_within(&slot.scale(scale))
                && quad
                    .background
                    .as_solid()
                    .is_some_and(|ink| (ink.h, ink.s, ink.l) == (accent.h, accent.s, accent.l))));
        })
        .unwrap();
}
