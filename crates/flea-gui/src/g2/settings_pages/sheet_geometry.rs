//! The keymap board's 628px card grows to the Russian board's 642px for its longest caption.
use super::{Sheet, Typeface, CLAMP, COL_GAP, GAP, KEY_COL, PAD, SHEET_H, SHEET_W};
use gpui::{Hsla, TextRun, Window};

pub const MAX_SHEET_W: f32 = 642.;
pub const KEY_MARGIN: f32 = 5.;
const BORDER: f32 = 2.;

#[cfg(test)]
pub fn caption_width(card: f32) -> f32 {
    (card - 2. * PAD - BORDER - COL_GAP) / 2. - KEY_COL - KEY_MARGIN - GAP
}

pub fn wanted_width(longest: f32) -> f32 {
    (2. * (KEY_COL + KEY_MARGIN + GAP + longest) + COL_GAP + 2. * PAD + BORDER)
        .ceil()
        .clamp(SHEET_W, MAX_SHEET_W)
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

pub fn for_sheet(sheet: &Sheet, face: &Typeface, window: &Window) -> (f32, f32, f32, f32) {
    let longest =
        sheet
            .groups()
            .iter()
            .flat_map(|(_, _, rows)| rows)
            .fold(0_f32, |longest, row| {
                let run = TextRun {
                    len: row.label.len(),
                    font: face.font.clone(),
                    color: Hsla::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                longest.max(f32::from(
                    window
                        .text_system()
                        .layout_line(&row.label, face.caption, &[run], None)
                        .width,
                ))
            });
    let viewport = window.viewport_size();
    rect(
        f32::from(viewport.width),
        f32::from(viewport.height),
        wanted_width(longest),
    )
}
