//! EY-413: the Permissions card on a folder draws the folder mark and v0.3.12's scope box; a file draws neither.
use super::permissions::Permissions;
use super::shell::{Block, Card, DialogShell, Glyph, StandIn, BOARD_BODY, CAPTION, ROW_GAP};
use super::tests::{english, permissions};
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use flea::jsondoc::parse;
use gpui::{prelude::*, px, size, Context, Render, TestAppContext, Window};

const NOTE: &str = "Scope: this folder only. Items inside it keep their permissions.";

fn folder() -> Permissions {
    let (mut card, _) = Permissions::open(8, vec!["/fx/v/work/A".into()], "");
    card.reply(&english(), &parse(r#"{"t":"permissions","id":8,"op":"inspect","ok":true,"path":"/fx/v/work/A","directory":true,"mode":"0755","uid":1000,"gid":1000,"owner":"gm","group":"gm","reason":""}"#).unwrap());
    card
}

fn stack(card: &Card) -> Vec<Block> {
    card.body
        .iter()
        .find_map(|b| match b {
            Block::Stack(v) => Some(v.clone()),
            _ => None,
        })
        .expect("the single-item stack is drawn")
}

#[test]
fn fixcards_ey413_a_folder_draws_the_folder_mark_and_a_file_the_file_mark() {
    let i18n = english();
    let mark = |card: &Card| {
        stack(card).iter().find_map(|b| match b {
            Block::Subject { glyph, .. } => Some(*glyph),
            _ => None,
        })
    };
    assert_eq!(mark(&folder().card(&i18n)), Some(Glyph::Folder));
    assert_eq!(mark(&permissions().card(&i18n)), Some(Glyph::File));
}

#[test]
fn fixcards_ey413_a_folder_shows_the_scope_note_under_owner_and_group_and_a_file_does_not() {
    let i18n = english();
    let parts = stack(&folder().card(&i18n));
    let at = parts
        .iter()
        .position(|b| *b == Block::Scope(NOTE.into()))
        .expect("a folder shows the scope note");
    assert!(
        matches!(&parts[at - 1], Block::Info { label, .. } if label == "Group"),
        "the box sits flush under the Group row: {:?}",
        parts[at - 1]
    );
    assert!(
        !stack(&permissions().card(&i18n))
            .iter()
            .any(|b| matches!(b, Block::Scope(_))),
        "a file shows no scope note"
    );
}

struct Probe(Card);

impl Render for Probe {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::inherited(&window.text_style(), Theme::default());
        StandIn.present(&self.0, &palette, &Typeface::omarchy_default(), 0., 1100.)
    }
}

#[gpui::test]
fn fixcards_ey413_the_scope_box_wraps_its_sentence_inside_the_card(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Probe(folder().card(&english())));
    cx.simulate_resize(size(px(1100.), px(900.)));
    cx.run_until_parked();
    let card = cx.debug_bounds("op3-card").expect("the card is drawn");
    let scope = cx
        .debug_bounds("op3-scope")
        .expect("the scope box is drawn");
    assert!(
        scope.left() >= card.left() && scope.right() <= card.right(),
        "the box stays inside the card: {scope:?} in {card:?}"
    );
    let text = cx
        .debug_bounds("op3-scope-text")
        .expect("the scope sentence is drawn");
    let s = f32::from(Typeface::omarchy_default().body) / BOARD_BODY;
    assert!(
        f32::from(text.top() - scope.top()) < (ROW_GAP + CAPTION) * s
            && f32::from(text.left() - scope.left()) > 2. * ROW_GAP * s,
        "the sentence starts beside the mark, after it: {text:?} in {scope:?}"
    );
    assert!(
        text.bottom() <= scope.bottom() && text.right() <= scope.right(),
        "the wrapped sentence stays inside its box: {text:?} in {scope:?}"
    );
    assert!(
        f32::from(text.size.height) >= 2. * CAPTION * s,
        "the sentence wraps onto a second line: {text:?}"
    );
}
