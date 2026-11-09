//! The keymap board's 628px card grows to the Russian board's 642px for its longest caption.
use super::{Sheet, Typeface, CLAMP, COL_GAP, GAP, KEY_COL, PAD, SHEET_H, SHEET_W};
use gpui::{Hsla, TextRun, Window};

pub const MAX_SHEET_W: f32 = 642.;
pub const KEY_MARGIN: f32 = 5.;
// The caption size the boards draw, text size 14's.
const BOARD_CAPTION: f32 = 12.;
const BORDER: f32 = 2.;

#[cfg(test)]
pub fn caption_width(card: f32) -> f32 {
    (card - 2. * PAD - BORDER - COL_GAP) / 2. - KEY_COL - KEY_MARGIN - GAP
}

#[cfg(test)]
pub fn wanted_width(longest: f32) -> f32 {
    wanted_width_at(longest, KEY_COL, MAX_SHEET_W)
}

// A larger text size widens the key column to its longest chord and lifts the cap by the caption's own growth.
fn wanted_width_at(longest: f32, key_col: f32, max: f32) -> f32 {
    (2. * (key_col + KEY_MARGIN + GAP + longest) + COL_GAP + 2. * PAD + BORDER)
        .ceil()
        .clamp(SHEET_W, max)
}

fn shaped(text: &str, face: &Typeface, window: &Window) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: face.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    f32::from(
        window
            .text_system()
            .layout_line(text, face.caption, &[run], None)
            .width,
    )
}

/// The key column: the board's 117 unless a chord, as the sheet draws it, shapes wider at this text size.
pub fn key_column(sheet: &Sheet, face: &Typeface, window: &Window) -> f32 {
    sheet
        .groups()
        .iter()
        .flat_map(|(_, _, rows)| rows)
        .fold(KEY_COL, |widest, row| {
            widest.max(drawn_keys(sheet.cap(&row.keys), face, window))
        })
}

// Sample keys: ". / ctrl-shift-."
// SheetView::keys lays out each alternative and each " / " as its own element, so their rounded widths add up.
fn drawn_keys(keys: &str, face: &Typeface, window: &Window) -> f32 {
    let slash = shaped(" / ", face, window).ceil();
    keys.split(" / ")
        .enumerate()
        .map(|(index, key)| shaped(key, face, window).ceil() + if index > 0 { slash } else { 0. })
        .sum()
}

pub fn rect(width: f32, height: f32, wanted: f32) -> (f32, f32, f32, f32) {
    let fit = |want: f32, room: f32| want.min((room - 2. * CLAMP).floor()).max(0.);
    let (card_w, card_h) = (fit(wanted, width), fit(SHEET_H, height));
    (
        ((width - card_w) / 2.).round(),
        ((height - card_h) / 2.).round(),
        card_w,
        card_h,
    )
}

pub fn card_rect(width: f32, height: f32) -> (f32, f32, f32, f32) {
    rect(width, height, SHEET_W)
}

pub fn for_sheet(
    sheet: &Sheet,
    face: &Typeface,
    key_col: f32,
    window: &Window,
) -> (f32, f32, f32, f32) {
    let longest = sheet
        .groups()
        .iter()
        .flat_map(|(_, _, rows)| rows)
        .fold(0_f32, |longest, row| {
            longest.max(shaped(&row.label, face, window))
        });
    let growth = (f32::from(face.caption) / BOARD_CAPTION).max(1.);
    let viewport = window.viewport_size();
    rect(
        f32::from(viewport.width),
        f32::from(viewport.height),
        wanted_width_at(longest, key_col, (MAX_SHEET_W * growth).ceil()),
    )
}
