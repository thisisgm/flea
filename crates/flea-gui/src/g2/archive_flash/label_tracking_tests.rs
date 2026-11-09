//! Caption tracking is observed from the real cards' painted glyphs, including their available width.
use super::cards::{Card, DialogShell};
use super::{compress::Compress, draw::StandIn, extract::Extract, tests::english};
use crate::g2::device_rail::{card as unlock, ink::CardInk};
use crate::g2::status_feedback::caption::{origins, INKED, PAINTED};
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use gpui::{prelude::*, px, Context, Render, TestAppContext, TextRun, Window};
use std::time::Duration;

enum Scene {
    Archive(Card),
    Unlock(Box<crate::g2::device_rail::unlock::Card>, unlock::CardText),
}

struct Probe(Scene, Typeface);

impl Render for Probe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::inherited(&window.text_style(), Theme::default());
        match &self.0 {
            Scene::Archive(card) => StandIn
                .present(card, &palette, &self.1, 1100., 0.)
                .into_any_element(),
            Scene::Unlock(card, text) => unlock::card(
                card,
                text,
                &CardInk::inherited(&palette),
                1100.,
                f32::from(self.1.body) / 14.,
                unlock::Press {
                    cancel: Box::new(|_, _, _| {}),
                    unlock: Box::new(|_, _, _| {}),
                },
            )
            .into_any_element(),
        }
    }
}

fn assert_tracked(
    cx: &mut TestAppContext,
    scene: Scene,
    body: f32,
    expected: &[&str],
    tracking: f32,
) {
    let scale = body / 14.;
    let slot = match &scene {
        Scene::Archive(card) if card.body_layout == super::cards::BodyLayout::Compress => {
            (card.width - 34. - 12.) / 2.
        }
        Scene::Archive(card) => card.width - 34.,
        Scene::Unlock(_, _) => unlock::CARD_WIDTH - 34.,
    };
    let face = Typeface {
        body: px(body),
        ..Typeface::omarchy_default()
    };
    let font = face.font.clone();
    let handle = cx.add_window(|_, _| Probe(scene, face));
    cx.run_until_parked();
    let plain = handle
        .update(cx, |_, window, _| {
            expected
                .iter()
                .map(|text| {
                    let run = TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: Theme::default().foreground,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        text.to_string().into(),
                        px(12. * scale),
                        &[run],
                        None,
                    );
                    origins(&line, px(0.))
                })
                .collect::<Vec<_>>()
        })
        .unwrap();
    PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    INKED.with(|inked| *inked.borrow_mut() = Some(Vec::new()));
    handle.update(cx, |_, window, _| window.refresh()).unwrap();
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    handle
        .update(cx, |_, window, cx| window.simulate_next_frame(cx))
        .unwrap();
    cx.run_until_parked();
    let painted = PAINTED.with(|painted| painted.borrow_mut().take().unwrap());
    let inked = INKED.with(|inked| inked.borrow_mut().take().unwrap());
    assert_eq!(
        painted.len(),
        plain.iter().map(Vec::len).sum::<usize>(),
        "each caption must paint through Tracked: {expected:?}"
    );
    assert_eq!(inked.len(), expected.len());
    let mut offset = 0;
    for ((line, (text, _, spacing, bounds)), expected) in plain.iter().zip(&inked).zip(expected) {
        assert_eq!(text.as_ref(), *expected);
        assert!((*spacing - px(tracking * scale)).abs() < px(0.001));
        assert_eq!(bounds.size.height, px(14. * scale));
        assert!(
            bounds.size.width <= px(slot * scale),
            "caption must fit its card slot: {expected}"
        );
        assert!(
            bounds.origin.x >= px(0.) && bounds.right() <= px(1100.),
            "caption must fit the window"
        );
        for (i, x) in line.iter().enumerate() {
            let want = *x + px(tracking * scale * i as f32);
            assert!(
                (painted[offset + i] - want).abs() < px(0.001),
                "caption glyph {i} must have {tracking}px tracking at scale {scale}"
            );
        }
        offset += line.len();
    }
}

#[gpui::test]
fn label_tracking_extract_password_uses_the_board_spacing(cx: &mut TestAppContext) {
    for body in [14., 16.] {
        let catalog = english();
        let mut extract = Extract::closed();
        extract.open("/fixture/locked.7z", "/fixture/unpacked", "", &catalog);
        assert_tracked(
            cx,
            Scene::Archive(extract.card(&catalog)),
            body,
            &["PASSWORD"],
            1.2,
        );
    }
}

#[gpui::test]
fn label_tracking_compress_tracks_chips_and_fields(cx: &mut TestAppContext) {
    for body in [14., 16.] {
        let catalog = english();
        let (compress, _) = Compress::open(
            vec!["/fixture/file.txt".into()],
            "/fixture",
            "fixture",
            0,
            &["7z"],
        );
        assert_tracked(
            cx,
            Scene::Archive(compress.card(&catalog)),
            body,
            &["FORMAT", "NAME", "PASSWORD", "CONFIRM"],
            1.2,
        );
    }
}

#[gpui::test]
fn label_tracking_unlock_passphrase_uses_the_board_spacing(cx: &mut TestAppContext) {
    for body in [14., 16.] {
        let mut card = crate::g2::device_rail::unlock::Card::new();
        card.open_for("/dev/fixture", "Fixture", 0, "crypto_LUKS", "");
        let text = unlock::CardText::of(&card, &english());
        let expected = text.label.clone();
        assert_tracked(
            cx,
            Scene::Unlock(Box::new(card), text),
            body,
            &[&expected],
            1.2,
        );
    }
}

#[gpui::test]
fn label_tracking_captions_obey_all_caseless_locales_and_turkish(cx: &mut TestAppContext) {
    for language in ["ja-JP", "zh_CN", "ko", "ar", "fa", "he", "ur", "th", "tr"] {
        let (expected, tracking) = if language == "tr" {
            ("İZİN", 1.2)
        } else {
            ("izin", 0.)
        };
        let catalog = english();
        let mut extract = Extract::closed();
        extract.open("/fixture/locked.7z", "/fixture/unpacked", "", &catalog);
        let mut card = extract.card(&catalog);
        card.language = language.into();
        let super::cards::Block::Field(field) = &mut card.body[0] else {
            unreachable!()
        };
        field.label = Some("izin".into());
        assert_tracked(cx, Scene::Archive(card), 14., &[expected], tracking);
        let mut card = crate::g2::device_rail::unlock::Card::new();
        card.open_for("/dev/fixture", "Fixture", 0, "crypto_LUKS", "");
        let mut text = unlock::CardText::of(&card, &catalog);
        text.language = language.into();
        text.label = crate::g2::status_feedback::caption::heading_for("izin", language);
        assert_tracked(
            cx,
            Scene::Unlock(Box::new(card), text),
            14.,
            &[expected],
            tracking,
        );
    }
}

#[test]
fn label_tracking_card_producers_keep_the_catalog_language() {
    let catalog = super::tests::language("language-de");
    assert_eq!(catalog.language(), "de");
    let mut extract = Extract::closed();
    extract.open("/fixture/locked.7z", "/fixture/unpacked", "", &catalog);
    let (compress, _) = Compress::open(
        vec!["/fixture/file.txt".into()],
        "/fixture",
        "fixture",
        0,
        &["7z"],
    );
    for card in [extract.card(&catalog), compress.card(&catalog)] {
        assert_eq!(card.language, catalog.language());
    }
    let text = unlock::CardText::of(&crate::g2::device_rail::unlock::Card::new(), &catalog);
    assert_eq!(text.language, catalog.language());
}
