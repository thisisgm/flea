//! The card's field names, grid heads and section heads use the shared tracked caption painter.
use super::{Block, Card, DialogShell, StandIn};
use crate::g2::open_with_properties::tests::{english, permissions};
use crate::g2::status_feedback::caption::{origins, PAINTED};
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use gpui::{prelude::*, px, Context, Pixels, Render, TestAppContext, TextRun, Window};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Site {
    Field,
    Grid,
    Section,
}

impl Site {
    fn matches(self, block: &Block) -> bool {
        matches!(
            (self, block),
            (Self::Field, Block::Field(_))
                | (Self::Grid, Block::Grid { .. })
                | (Self::Section, Block::Section { .. })
        )
    }

    fn card(self) -> Card {
        let mut card = permissions().card(&english());
        let block = card
            .body
            .iter()
            .find_map(|block| match block {
                Block::Stack(blocks) => blocks.iter().find(|block| self.matches(block)),
                _ => None,
            })
            .expect("the Permissions card has this caption site")
            .clone();
        card.body = vec![block];
        card.buttons.clear();
        card
    }
}

struct Probe(Card, Typeface);

impl Render for Probe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::inherited(&window.text_style(), Theme::default());
        StandIn.present(&self.0, &palette, &self.1, 0., 1100.)
    }
}

fn assert_tracked(
    cx: &mut TestAppContext,
    card: Card,
    body: f32,
    expected: &[&str],
    tracking: f32,
) {
    let scale = body / 14.;
    let face = Typeface {
        body: px(body),
        ..Typeface::omarchy_default()
    };
    let font = face.font.clone();
    let handle = cx.add_window(|_, _| Probe(card, face));
    cx.run_until_parked();
    let plain: Vec<Vec<Pixels>> = handle
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
                    let shaped = window.text_system().shape_line(
                        text.to_string().into(),
                        px(12. * scale),
                        &[run],
                        None,
                    );
                    origins(&shaped, px(0.))
                })
                .collect()
        })
        .unwrap();
    PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    handle.update(cx, |_, window, _| window.refresh()).unwrap();
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    handle
        .update(cx, |_, window, cx| window.simulate_next_frame(cx))
        .unwrap();
    cx.run_until_parked();
    let painted = PAINTED.with(|painted| painted.borrow_mut().take().unwrap());
    let glyphs: usize = plain.iter().map(Vec::len).sum();
    assert_eq!(
        painted.len(),
        glyphs,
        "each caption must paint through Tracked: {expected:?}"
    );
    let mut offset = 0;
    for line in plain {
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
fn permissions_octal_label_uses_tracked_caption_at_board_spacing(cx: &mut TestAppContext) {
    for body in [14., 16.] {
        assert_tracked(cx, Site::Field.card(), body, &["OCTAL"], 1.2);
    }
}

#[gpui::test]
fn permissions_grid_heads_use_tracked_caption_at_board_spacing(cx: &mut TestAppContext) {
    for body in [14., 16.] {
        assert_tracked(
            cx,
            Site::Grid.card(),
            body,
            &["READ", "WRITE", "EXECUTE"],
            1.2,
        );
    }
}

#[gpui::test]
fn permissions_section_head_uses_tracked_caption_at_board_spacing(cx: &mut TestAppContext) {
    for body in [14., 16.] {
        assert_tracked(cx, Site::Section.card(), body, &["WILL CHANGE"], 1.2);
    }
}

#[gpui::test]
fn permissions_captions_follow_the_catalog_language(cx: &mut TestAppContext) {
    for site in [Site::Field, Site::Grid, Site::Section] {
        for (language, text, expected, tracking) in [
            ("ja-JP", "読み取り", "読み取り", 0.),
            ("ar", "قراءة", "قراءة", 0.),
            ("tr", "izin", "İZİN", 1.2),
        ] {
            let mut card = site.card();
            card.language = language.into();
            match &mut card.body[0] {
                Block::Field(field) => field.label = Some(text.into()),
                Block::Grid { columns, .. } => *columns = vec![text.into()],
                Block::Section { head, .. } => *head = text.into(),
                _ => unreachable!(),
            }
            assert_tracked(cx, card, 14., &[expected], tracking);
        }
    }
}

#[test]
fn every_card_producer_carries_the_catalog_language() {
    let de = crate::g2::open_with_properties::tests::language("language-de");
    assert_ne!(de.language(), english().language());
    let bulk = crate::g2::open_with_properties::bulkrename::BulkRename::open(vec![
        "/fx/b/alpha.txt".into(),
    ]);
    let (properties, _) =
        crate::g2::open_with_properties::properties::Properties::open(5, 1, &[2], 0);
    // Sample input: the backend's run ask for a script, as card_tests.rs ASK.
    let ask = r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/fx/v/work/hello.sh"}"#;
    let run = crate::g2::open_with_properties::runcard::RunCard::open(
        &de,
        "flea",
        "/fx/v/work/hello.sh",
        ask,
    )
    .expect("the run card opens on a script ask");
    for card in [
        permissions().card(&de),
        bulk.card(&de),
        crate::g2::open_with_properties::tests::open_with().card(&de),
        properties.card(&de),
        run.card(&de),
    ] {
        assert_eq!(card.language, de.language(), "{}", card.title);
    }
}

#[test]
fn turkish_captions_take_the_dotted_capital() {
    assert_eq!(
        flea::ui_logic_settings_pages::caps::heading("izin", "tr"),
        "\u{130}Z\u{130}N"
    );
}

#[gpui::test]
fn bulk_rename_field_and_name_labels_share_the_board_caption_tracking(cx: &mut TestAppContext) {
    let bulk = crate::g2::open_with_properties::bulkrename::BulkRename::open(vec![
        "/fx/b/alpha.txt".into(),
        "/fx/b/beta.md".into(),
    ]);
    assert_tracked(
        cx,
        bulk.card(&english()),
        14.,
        &["FIND", "REPLACE", "OLD NAMES", "NEW NAMES"],
        1.2,
    );
}

#[gpui::test]
fn openwith_eyebrows_use_the_hig_tracking(cx: &mut TestAppContext) {
    let mut card = crate::g2::open_with_properties::tests::open_with().card(&english());
    card.body.retain(|block| matches!(block, Block::Apps(..)));
    card.buttons.clear();
    assert_tracked(
        cx,
        card,
        14.,
        &["REGISTERED FOR PLAIN TEXT DOCUMENT", "ALL APPLICATIONS"],
        1.68,
    );
}
