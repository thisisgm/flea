//! Omi retains each old spinner's slot, regardless of adjacent caption geometry.
use super::{
    spinner,
    spinner_tests::{frame, load, open, shell},
};
use crate::{pane::states::Kind, window::startup::View};
use gpui::{div, point, prelude::*, px, size, Bounds, Context, Render, TestAppContext, Window};

fn view_rect(slot: Bounds<gpui::Pixels>, scale: f32) -> Bounds<gpui::Pixels> {
    spinner::mapped(
        &omi::Rect {
            x: 0.,
            y: 0.,
            w: 22.,
            h: 22.,
            opacity: 1.,
            role: "body".into(),
        },
        [0., 0., 22., 22.],
        slot,
        scale,
    )
}

fn painted_box(window: &Window, mark: &spinner::Crawl) -> [f32; 4] {
    let slot = mark.slot().scale(window.scale_factor());
    let quads: Vec<_> = window
        .painted_quads()
        .into_iter()
        .filter(|quad| {
            quad.bounds.is_contained_within(&slot) && quad.background.as_solid() == Some(mark.ink())
        })
        .collect();
    assert!(!quads.is_empty(), "the actual Omi frame must paint");
    quads.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |b, quad| {
            [
                b[0].min(quad.bounds.left().0),
                b[1].min(quad.bounds.top().0),
                b[2].max(quad.bounds.right().0),
                b[3].max(quad.bounds.bottom().0),
            ]
        },
    )
}

#[test]
fn omi_fit_preserves_old_surface_sizes_and_snaps_every_edge() {
    for scale in [1., 1.25, 1.5, 2.] {
        // Pre-Omi: loading and menu 48, empty hero 48 at text size 14, jobs 16.
        for (surface, side) in [
            ("loading", 48.),
            ("menu", 48.),
            ("empty", 48.),
            ("job", 16.),
        ] {
            let slot = Bounds::new(point(px(100.3), px(200.7)), size(px(side), px(side)));
            let drawn = view_rect(slot, scale).scale(scale);
            assert_eq!(
                drawn.size.width.0,
                (side * scale).round(),
                "{surface} at {scale}"
            );
            assert_eq!(
                drawn.size.height.0,
                (side * scale).round(),
                "{surface} at {scale}"
            );
            for edge in [
                drawn.left().0,
                drawn.top().0,
                drawn.right().0,
                drawn.bottom().0,
            ] {
                assert_eq!(edge, edge.round(), "{surface} at {scale}");
            }
        }
    }
}

struct Surfaces(spinner::Mode);
impl Render for Surfaces {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::g2::op1::glyph::{Glyph, Paint};
        div()
            .flex()
            .items_start()
            .gap(px(20.))
            .child(
                div()
                    .debug_selector(|| "fit-menu".into())
                    .child(spinner::Slot(Default::default())),
            )
            .child(
                div().debug_selector(|| "fit-job".into()).child(Glyph {
                    paint: Paint::Spiral {
                        id: 123,
                        mode: self.0,
                        still: Some(0.),
                    },
                    side: 16.,
                    ink: cx
                        .try_global::<crate::palette::Theme>()
                        .copied()
                        .unwrap_or_default()
                        .accent,
                }),
            )
    }
}

#[gpui::test]
fn omi_fit_real_menu_and_job_slots_keep_their_pre_omi_sizes(cx: &mut TestAppContext) {
    for mode in [
        spinner::Mode::Thinking,
        spinner::Mode::Working,
        spinner::Mode::Upload,
        spinner::Mode::Download,
    ] {
        let handle = cx.add_window(|_, _| Surfaces(mode));
        for scale in [1., 1.25, 1.5, 2.] {
            cx.simulate_window_scale_factor_change(handle.into(), scale);
            frame(cx, handle);
            let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
            for (selector, side) in [("fit-menu", 48.), ("fit-job", 16.)] {
                let slot = visual.debug_bounds(selector).unwrap();
                assert_eq!(
                    slot.size,
                    size(px(side), px(side)),
                    "{selector} {mode:?} {scale}"
                );
            }
        }
    }
}

#[gpui::test]
fn omi_fit_loading_does_not_follow_message_line_count(cx: &mut TestAppContext) {
    for view in [View::List, View::Grid, View::Columns, View::Dual] {
        let (handle, _keep) = open(cx, view == View::Dual);
        cx.simulate_window_resize(handle.into(), size(px(1100.), px(700.)));
        shell(cx, handle, |shell, _, cx| {
            shell.view = view;
            cx.notify();
        });
        let mark = load(cx, handle);
        let mut first = None;
        let mut first_paint = None;
        for message in [
            "Reading",
            "Reading\nSecond line",
            "Reading\nSecond line\nThird line",
        ] {
            shell(cx, handle, |shell, _, cx| {
                shell.panes[0].as_mut().unwrap().state_text.message = message.into();
                cx.notify();
            });
            frame(cx, handle);
            let slot = mark.slot();
            let rect = view_rect(slot, 1.);
            assert_eq!(*first.get_or_insert(rect), rect, "message {message}");
            let painted = handle
                .update(cx, |_, window, _| painted_box(window, &mark))
                .unwrap();
            assert_eq!(
                *first_paint.get_or_insert(painted),
                painted,
                "painted message {message}"
            );
        }
        assert_eq!(mark.slot().size, size(px(48.), px(48.)));
    }
}

#[gpui::test]
fn omi_fit_empty_does_not_follow_rotating_caption_metrics(cx: &mut TestAppContext) {
    for view in [View::List, View::Grid, View::Columns, View::Dual] {
        let (handle, _keep) = open(cx, view == View::Dual);
        cx.simulate_window_resize(handle.into(), size(px(1100.), px(700.)));
        shell(cx, handle, |shell, window, cx| {
            window.set_rem_size(px(14.));
            shell.view = view;
            for host in shell.panes.iter_mut().flatten() {
                host.pane.state.kind = Kind::Empty;
                host.changed(&shell.locale);
            }
            cx.notify();
        });
        for _ in 0..150 {
            frame(cx, handle);
        }
        let initial = shell(cx, handle, |shell, window, cx| {
            shell
                .surfaces
                .iter()
                .filter_map(|surface| surface.hero.as_ref())
                .map(|hero| {
                    (
                        hero.read(cx).omi.slot(),
                        painted_box(window, &hero.read(cx).omi),
                    )
                })
                .collect::<Vec<_>>()
        });
        assert!(!initial.is_empty(), "{view:?} mounts a hero");
        for _ in 0..2 {
            for _ in 0..175 {
                frame(cx, handle);
            }
            shell(cx, handle, |shell, window, cx| {
                for (hero, (slot, paint)) in shell
                    .surfaces
                    .iter()
                    .filter_map(|surface| surface.hero.as_ref())
                    .zip(&initial)
                {
                    assert_eq!(hero.read(cx).omi.slot(), *slot, "{view:?} caption changed");
                    assert_eq!(
                        painted_box(window, &hero.read(cx).omi),
                        *paint,
                        "{view:?} painted caption changed"
                    );
                }
            });
        }
        shell(cx, handle, |shell, _, cx| {
            assert!(shell
                .surfaces
                .iter()
                .filter_map(|surface| surface.hero.as_ref())
                .any(|hero| hero.read(cx).rotation().index() >= 2));
        });
    }
}
