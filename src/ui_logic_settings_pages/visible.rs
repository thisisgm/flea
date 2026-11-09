// What a Settings row shows, in the order the row draws it, and which control a choice gets, from ui/SettingsRow.qml.
use super::rows::{Kind, Row};
use super::Text;
use crate::i18n::I18n;

// SettingsRow.qml's padding 14 each side, the 19px mark slot and three 9px gaps around label, control and mark.
pub const SEGMENT_CHROME: f32 = 2. * 14. + 19. + 3. * 9.;
// A chip: 7px each side and its 1px frame, never narrower than 24, 4 apart.
pub const CHIP_PAD: f32 = 7.;
pub const CHIP_MIN: f32 = 24.;
pub const CHIP_GAP: f32 = 4.;

// SettingsGrammar rule 1, from the caller's shaped widths: a segment when every option fits beside the label, else the chevron walk.
pub fn segment_fits(label_width: f32, option_widths: &[f32], row_width: f32) -> bool {
    let chips: f32 = option_widths.iter().map(|width| (width + 2. * CHIP_PAD + 2.).max(CHIP_MIN)).sum();
    let gaps = CHIP_GAP * option_widths.len().saturating_sub(1) as f32;
    chips + gaps + label_width + SEGMENT_CHROME <= row_width
}

// The view choice draws the four view marks, the current one in foreground.
pub fn marks(row: &Row) -> bool {
    row.kind == Kind::Choice && row.id == "view"
}

// The texts a row draws, left to right: label, then the trailing parts in the order the boards set them.
pub fn texts(row: &Row, segment: bool) -> Vec<Text> {
    let mut out = Vec::new();
    let mut push = |text: &Text| {
        if !text.is_empty() {
            out.push(text.clone());
        }
    };
    match row.kind {
        Kind::Ruler => return row.stops.iter().map(|stop| Text::data(stop.to_string())).collect(),
        Kind::KeyPreview => {
            for item in &row.items {
                push(&Text::data(item.keys.clone()));
                push(&item.label);
            }
            return out;
        }
        _ => {}
    }
    push(&row.label);
    match row.kind {
        // A check's caption sits before its box; a fact's after its value.
        Kind::Check => {
            push(&row.caption);
            push(&row.value);
        }
        // Last-used view draws one mark per view, no words.
        Kind::Choice if marks(row) => {}
        Kind::Choice if segment => row.labels.iter().for_each(&mut push),
        Kind::Fact => {
            push(&row.value);
            push(&row.caption);
        }
        Kind::Favourite => {
            push(&Text::data(row.display.clone()));
            push(&row.error);
        }
        _ => push(&row.value),
    }
    out
}

pub fn said(row: &Row, segment: bool, i18n: &I18n) -> Vec<String> {
    texts(row, segment).iter().map(|text| text.resolve(i18n)).collect()
}

#[cfg(test)]
#[path = "visible_tests.rs"]
mod tests;
