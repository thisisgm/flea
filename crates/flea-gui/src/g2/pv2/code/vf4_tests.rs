use super::*;
use gpui::px;

fn view(numbered: bool) -> CodeView {
    let ink = crate::palette::Theme::default().foreground;
    let mut view = CodeView::new(
        "look",
        Typeface {
            font: gpui::font("JetBrains Mono"),
            body: px(12.),
            caption: px(12.),
        },
        Palette {
            foreground: ink,
            muted: ink,
            muted_frame: ink,
            gutter: ink,
            inks: [ink; 7],
        },
        Metrics {
            inset: 10.,
            top: 11.,
            right: 10.,
            row: 17.,
            gap: 10.,
            state_mark: 16.,
        },
        (SharedString::from("file-text"), &[]),
    );
    view.numbered = numbered;
    view
}

#[test]
fn visual_geometry_plain_text_uses_its_inset_without_a_gutter() {
    let advance = px(7.2);
    assert_eq!(view(false).text_left(advance), 10.);
    assert_eq!(view(false).gutter_right(advance), 10.);
    // A numbered view keeps the same two-cell lead past its gutter, up to the final whole-pixel rounding.
    let numbered = view(true);
    let lead = numbered.text_left(advance) - numbered.gutter_right(advance);
    assert!((lead - 2. * 7.2).abs() <= 0.5, "{lead}");
}

#[test]
fn code_rows_keep_the_board_pitch_and_fit_taller_glyphs() {
    use gpui::{AppContext, HeadlessAppContext, TextRenderingMode};
    use std::{borrow::Cow, sync::Arc};

    for body in [14., 28.] {
        let mut app = HeadlessAppContext::with_platform(
            gpui_platform::current_platform(true).text_system(),
            Arc::new(()),
            gpui_platform::current_headless_renderer,
        );
        app.update(|cx| {
            cx.set_reduce_motion(true);
            cx.set_text_rendering_mode(TextRenderingMode::Grayscale);
            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(
                    include_bytes!("../../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
                )])
                .unwrap();
        });
        let handle = app
            .open_window(gpui::size(px(900.), px(600.)), |window, cx| {
                window.set_scale_factor(1.);
                window.set_rem_size(px(14.));
                cx.new(|cx| {
                    let mut code = view(true);
                    code.typeface.body = px(body);
                    code.metrics.row = 20.;
                    let text = "fn main() {}\n".to_string();
                    code.set_subject("fixture.rs", text.len() as u64, true, Some(text), cx);
                    code.set_active(true, cx);
                    code
                })
            })
            .unwrap();
        app.run_until_parked();
        app.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        let (actual, measured) = handle
            .update(&mut app, |code, window, _| {
                assert!(
                    code.advance.is_some(),
                    "actual layout measured the body font"
                );
                assert!(code.frame.1 > 0., "actual layout used a nonempty viewport");
                let run = shape::glyphs(window, "0", &code.typeface.font, code.typeface.body);
                (
                    code.metrics.row,
                    f32::from(run.line().ascent + run.line().descent).ceil(),
                )
            })
            .unwrap();
        if body == 14. {
            assert!(
                measured < 20.,
                "the tracked font exposes the short-pitch regression"
            );
            assert_eq!(actual, 20., "the HIG row pitch survives actual font layout");
        } else {
            assert!(
                measured > 20.,
                "the tall-font control exercises measured fitting"
            );
            assert!(
                actual >= measured,
                "a board minimum must not clip taller glyphs"
            );
        }
    }
}
