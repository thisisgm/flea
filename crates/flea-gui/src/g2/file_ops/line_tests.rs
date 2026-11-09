//! Shared buttons retain baselines with descenders, accents and translated labels.
use super::{
    fix10_tests::{init, real_app, redraw},
    palette::CardPalette,
    press::Press,
    view::{self, Pointer, Scene},
};
use crate::{list::Typeface, palette::Theme};
use flea::ui_logic_file_ops::card::Kind;
use gpui::{div, prelude::*, px, size, Context, Render, Window};

struct Controls {
    labels: [&'static str; 2],
    title: String,
}
impl Render for Controls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let face = Typeface::for_window(window, cx);
        let palette = CardPalette::from_theme(&window.text_style(), Theme::default(), 8.);
        let scene = Scene {
            copy_field: None,
            frame: None,
            typeface: &face,
            palette: &palette,
            catalog: &crate::test_catalogs::english(),
            pointer: &Pointer::default(),
            wire: &|_, button| button,
            pinned: None,
            feedback: None,
        };
        let buttons = vec![
            view::button(
                &scene,
                Press::CopyCancel,
                self.labels[0].into(),
                view::state(false, false, false, false),
            ),
            view::button(
                &scene,
                Press::CopyConfirm,
                self.labels[1].into(),
                view::state(true, false, false, false),
            ),
        ];
        view::scrim(
            &palette,
            0,
            view::shell(
                &scene,
                Kind::CopyTo,
                (1100., 700.),
                self.title.clone(),
                vec![div().h(px(49.)).into_any_element()],
                buttons,
            ),
        )
    }
}

#[test]
fn fix10_shared_buttons_align_localized_ascenders_and_descenders() {
    let mut app = real_app();
    init(&mut app);
    let (view, cx) = app.add_window_view(|window, _| {
        window.set_rem_size(px(14.));
        Controls {
            labels: ["Cancel", "Copy"],
            title: "Copy To".into(),
        }
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    for base in [12., 14., 16.] {
        for scale in [1., 2.] {
            cx.simulate_scale_factor_change(scale);
            for labels in [
                ["Cancel", "Copy"],
                ["Agjpqy", "Égyptien"],
                ["Abbrechen", "Kopieren"],
                ["Отмена", "Копировать"],
            ] {
                cx.update(|window, cx| {
                    window.set_rem_size(px(base));
                    view.update(cx, |view, cx| {
                        view.labels = labels;
                        cx.notify();
                    });
                });
                cx.run_until_parked();
                let samples = redraw(cx);
                let left = samples.iter().find(|s| s.text == labels[0]).unwrap();
                let right = samples.iter().find(|s| s.text == labels[1]).unwrap();
                assert_eq!(
                    left.baseline, right.baseline,
                    "{labels:?} body={base} scale={scale}"
                );
                assert_eq!(left.bounds.top(), right.bounds.top());
                assert_eq!(left.line, right.line);
                assert!(!left.ink.is_empty() && !right.ink.is_empty());
                let buttons = [
                    cx.debug_bounds("op1-button-0").unwrap(),
                    cx.debug_bounds("op1-button-1").unwrap(),
                ];
                assert_eq!(buttons[0].top(), buttons[1].top());
                assert_eq!(buttons[0].size.height, buttons[1].size.height);
                for (sample, button) in [(left, buttons[0]), (right, buttons[1])] {
                    let top = f32::from(button.top()) * scale;
                    let bottom = f32::from(button.bottom()) * scale;
                    assert!(sample.ink.iter().all(|(_, first, last)| *first as f32 >= top && (*last as f32) < bottom), "localized ink stays inside its control");
                }
                assert_eq!(
                    right.displayed, labels[1],
                    "translated labels keep one complete line"
                );
            }
        }
    }
}

#[test]
fn fix10_card_title_still_ellipsizes() {
    let mut app = real_app();
    init(&mut app);
    let (_, cx) = app.add_window_view(|window, _| {
        window.set_rem_size(px(14.));
        Controls {
            labels: ["Cancel", "Copy"],
            title: "Ordner kopieren avec des caractères Égjpq ".repeat(30),
        }
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let samples = redraw(cx);
    let title = samples
        .iter()
        .find(|s| s.text.starts_with("Ordner"))
        .unwrap();
    assert!(
        title.displayed.ends_with('…'),
        "title uses the existing ellipsis recipe"
    );
    assert!(title.displayed.len() < title.text.len());
    assert_eq!(title.line, px(19.2));
    assert_eq!(cx.debug_bounds("op1-card").unwrap().size.width, px(552.));
}

#[test]
fn fix11_long_title_ellipsizes_at_each_theme_stop() {
    for base in [12., 14., 16.] {
        let mut app = real_app();
        init(&mut app);
        let (_, cx) = app.add_window_view(|window, _| {
            window.set_rem_size(px(base));
            Controls {
                labels: ["Cancel", "Copy"],
                title: "Ordner kopieren avec des caractères Égjpq ".repeat(30),
            }
        });
        cx.simulate_resize(size(px(1100.), px(700.)));
        let samples = redraw(cx);
        let title = samples
            .iter()
            .find(|s| s.text.starts_with("Ordner"))
            .unwrap();
        let card = cx.debug_bounds("op1-card").unwrap();
        println!(
            "long title body={base}: line={:?} source={} displayed={} ellipsis={}",
            title.bounds,
            title.text.len(),
            title.displayed.len(),
            title.displayed.ends_with('…')
        );
        assert!(title.displayed.ends_with('…'));
        assert!(title.displayed.len() < title.text.len());
        assert!(title.bounds.right() < card.right());
        assert!(title.bounds.left() > card.left());
    }
}
