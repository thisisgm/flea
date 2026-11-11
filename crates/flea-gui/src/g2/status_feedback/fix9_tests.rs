//! Text-size changes reach the actual empty and locked views at all three oracle stops.
use super::{
    hero::EmptyHero,
    hero_tests::{bounds, english, frame},
    state::{Facts, StateMessage, GRID, LIST},
};
use gpui::{px, size, TestAppContext};

#[gpui::test]
fn fix9_hero_caption_follows_text_size_and_the_mark_stays_66(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let view = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        let mut hero = EmptyHero::new(&english(), cx);
        hero.show(false, cx);
        hero
    });
    cx.simulate_window_resize(view.into(), size(px(908.), px(619.)));
    for (base, mark, line) in [
        (12_f32, 66., 10_f32 * 4. / 3.),
        (14., 66., 16.),
        (16., 66., 13. * 4. / 3.),
    ] {
        view.update(cx, |_, window, cx| {
            window.set_rem_size(px(base));
            cx.notify();
        })
        .unwrap();
        frame(cx, view);
        frame(cx, view);
        let parts = bounds(cx, view, &["fb1-hero-mark", "fb1-hero-caption"]);
        assert_eq!(parts[0].size.width, px(mark), "hero mark at {base}");
        let line = view
            .update(cx, |_, window, _| window.pixel_snap(px(line)))
            .unwrap();
        assert_eq!(parts[1].size.height, line, "caption line at {base}");
    }
}

#[gpui::test]
fn fix9_locked_list_and_grid_follow_body_small_body_and_caption(cx: &mut TestAppContext) {
    let view = cx.add_window(|window, _| {
        window.set_rem_size(px(14.));
        StateMessage::new()
    });
    cx.simulate_window_resize(view.into(), size(px(908.), px(619.)));
    for metrics in [LIST, GRID] {
        for (base, mark, caption) in [(12_f32, 34., 10.), (14., 40., 12.), (16., 47., 13.)] {
            view.update(cx, |view, window, cx| {
                window.set_rem_size(px(base));
                view.set_metrics(metrics, cx);
                view.set(
                    Some(Facts::failed(
                        "scan",
                        "Permission denied",
                        0o40000,
                        &english(),
                    )),
                    cx,
                );
                cx.notify();
            })
            .unwrap();
            frame(cx, view);
            frame(cx, view);
            let parts = bounds(
                cx,
                view,
                &["fb1-state-mark", "fb1-state-sentence", "fb1-state-detail"],
            );
            assert_eq!(parts[0].size.width, px(mark), "lock at {base}");
            let sentence = base * if metrics == GRID { 1.32_f32 } else { 1.2_f32 };
            let detail: f32 = caption * if metrics == GRID { 22. / 12. } else { 1.2 };
            let lines = view
                .update(cx, |_, window, _| {
                    (
                        window.pixel_snap(px(sentence)),
                        window.pixel_snap(px(detail)),
                    )
                })
                .unwrap();
            assert_eq!(parts[1].size.height, lines.0);
            assert_eq!(parts[2].size.height, lines.1);
        }
    }
}

/// EY-83 supersedes the FIX9 neutral-ink assertion: locked, grid-locked and locked-menu draw the padlock in var(--error), as StateMessage.qml does.
#[test]
fn ey83_locked_uses_the_error_ink_even_when_it_is_distinct() {
    let theme = crate::palette::Theme::default();
    let mut ink = super::Ink::interim(&crate::list::Palette::inherited(
        &gpui::TextStyle::default(),
        theme,
    ));
    ink.error = theme.background;
    let locked = Facts::failed("scan", "Permission denied", 0, &english());
    let error = Facts::failed("scan", "item not found", 0, &english());
    assert_ne!(ink.foreground, ink.error);
    assert_eq!(locked.mark_ink(ink), ink.error);
    assert_eq!(error.mark_ink(ink), ink.error);
}

#[test]
fn fix9_flea_mark_arrives_whole_without_the_loading_spinner() {
    let mut empty = 0;
    super::brand::drawn(0., 1., |_, _| empty += 1);
    assert_eq!(empty, 0);
    let mut area = 0.;
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    super::brand::drawn(114., 1., |[x0, y0, x1, y1], coverage| {
        area += (x1 - x0) * (y1 - y0) * coverage;
        bounds = [
            bounds[0].min(x0),
            bounds[1].min(y0),
            bounds[2].max(x1),
            bounds[3].max(y1),
        ];
    });
    assert_eq!(bounds, [2., 2., 22., 22.]);
    assert_eq!(area, 232.);
}
