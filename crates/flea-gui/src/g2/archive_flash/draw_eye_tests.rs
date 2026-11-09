//! EY-381: an empty, unfocused secret field still draws its eye at the trailing edge, compress.png and v0.3.12.
use super::cards::{Block, Card};
use super::compress::Compress;
use super::draw::{EyeProbe, StandIn};
use super::tests::english;
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use gpui::{prelude::*, px, size, Context, Render, TestAppContext, Window};

struct EyeCard {
    card: Card,
    eye: EyeProbe,
}

impl Render for EyeCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let typeface = Typeface {
            body: px(14.),
            ..Typeface::omarchy_default()
        };
        StandIn.present_probed(
            &self.card,
            &palette,
            &typeface,
            f32::from(window.viewport_size().width),
            0.,
            std::slice::from_ref(&self.eye),
            None,
        )
    }
}

/// The eye's right edge sits within one field padding and the eye's own width of the card's right edge.
fn eye_at_trailing_edge(cx: &mut TestAppContext, value: &str) {
    let i18n = english();
    let mut card = Compress::open(
        vec!["/fx/v/work/notes.txt".into()],
        "/fx/v/work",
        "work",
        0,
        &["7z", "zip"],
    )
    .0
    .card(&i18n);
    // The CONFIRM field alone, empty and unfocused, as the card draws it under a focused PASSWORD.
    let confirm = i18n.tr("archive.confirm");
    card.body.retain(|block| {
        matches!(block, Block::Field(field) if field.label.as_deref() == Some(confirm.as_str()))
    });
    assert_eq!(card.body.len(), 1, "the confirm field is kept");
    if let Block::Field(field) = &mut card.body[0] {
        assert!(!field.focused, "the confirm field is unfocused");
        field.value = value.into();
    }
    let eye = EyeProbe::default();
    let probe = eye.clone();
    let (_, cx) = cx.add_window_view(|_, _| EyeCard { card, eye: probe });
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    let frame = cx.debug_bounds("op2-card").expect("the card draws");
    let eye = eye.get().expect("the eye paints");
    let gap = frame.right() - eye.right();
    assert!(
        gap >= px(0.) && gap < px(64.),
        "the eye sits at the trailing edge (value {value:?}): eye right {:?}, card right {:?}",
        eye.right(),
        frame.right()
    );
}

#[gpui::test]
fn an_empty_unfocused_confirm_field_draws_its_eye_at_the_trailing_edge(cx: &mut TestAppContext) {
    eye_at_trailing_edge(cx, "");
}

#[gpui::test]
fn a_filled_unfocused_confirm_field_draws_its_eye_at_the_trailing_edge(cx: &mut TestAppContext) {
    eye_at_trailing_edge(cx, "secret");
}
