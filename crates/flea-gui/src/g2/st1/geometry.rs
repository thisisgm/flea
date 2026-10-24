//! The Settings card's measures from the settings boards, and the pane's scroll rules from ui/SettingsPane.qml.
use flea::ui_logic_st1::rows::{focusable, Kind, Row};

// A 630 by 684 card in a 700px window: 8px clear of each edge, a 27px head over its hairline, a 168px rail.
pub const CARD_W: f32 = 630.;
pub const BOARD_CARD_H: f32 = 684.;
pub const CLAMP: f32 = 8.;
pub const BORDER: f32 = 1.;
pub const HEAD_H: f32 = 27.;
pub const RAIL_W: f32 = 168.;
pub const RAIL_PAD_TOP: f32 = 10.;
pub const RAIL_ROW_H: f32 = 29.;
pub const RAIL_MARK: f32 = 15.;
pub const PAD_X: f32 = 14.;
pub const GAP: f32 = 9.;
pub const MARK_SLOT: f32 = 19.;
pub const ROW_MARK: f32 = 18.;
pub const BRAND_MARK: f32 = 16.;
pub const LABEL_X: f32 = PAD_X + MARK_SLOT + GAP;
pub const ROW_H: f32 = 37.;
pub const FAVOURITE_H: f32 = 29.;
pub const HERO_H: f32 = 121.;
pub const RULER_H: f32 = 39.;
pub const CHIP_H: f32 = 24.;
pub const CHIP_GAP: f32 = 4.;
pub const CHECK: f32 = 18.;
pub const FIELD_H: f32 = 30.;
// The pane stops 8px above the card's foot.
pub const PANE_FOOT: f32 = 8.;
// Headings: 16px over the first, a hairline 7px under the row above and 19px over every later one, 7.8 under.
pub const GROUP_LINE: f32 = 14.4;
pub const GROUP_FIRST_H: f32 = 16. + GROUP_LINE + 7.8;
pub const GROUP_H: f32 = 7. + 1. + 19. + GROUP_LINE + 7.8;
// A hint: 4 over, 2 under, 18px lines; the footer: 10 over its hairline, 10 and 12 around 16px lines.
pub const HINT_LINE: f32 = 18.;
pub const FOOTER_LINE: f32 = 16.;
// The fade over a cut row, seven tenths of a row, and the share of it the next row's first ink shows under.
pub const FADE: f32 = 26.;
const CUT_SHARE: f32 = 0.5;
// The row padding a control's ink starts under.
const ROW_PAD_Y: f32 = 8.;

/// A row's height with its wrapped line count; only hints wrap.
pub fn row_height(row: &Row, first: bool, lines: usize) -> f32 {
    let lines = if row.elide_right { 1 } else { lines.max(1) } as f32;
    match row.kind {
        Kind::Group if first => GROUP_FIRST_H,
        Kind::Group => GROUP_H,
        Kind::Hint if row.footer => 10. + 1. + 10. + FOOTER_LINE * lines + 12.,
        Kind::Hint => 4. + HINT_LINE * lines + 2.,
        Kind::Ruler => RULER_H,
        Kind::KeyPreview => {
            let pairs = row.items.len().div_ceil(2) as f32;
            14. + CHIP_H * pairs + GAP * (pairs - 1.).max(0.)
        }
        Kind::Hero => HERO_H,
        Kind::Favourite => FAVOURITE_H,
        _ => ROW_H,
    }
}

/// Every row's height, from measured hint lines where the window shaped them and one line elsewhere.
pub fn heights(rows: &[Row], lines: &[usize]) -> Vec<f32> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| row_height(row, index == 0, lines.get(index).copied().unwrap_or(1)))
        .collect()
}

/// Where a row's first ink starts: a heading's label under its rule and padding, any other row under its padding.
fn ink_top(row: &Row, first: bool) -> f32 {
    match row.kind {
        Kind::Group if first => 16.,
        Kind::Group => 27.,
        _ => ROW_PAD_Y,
    }
}

/// The scroll after a cursor move: up to its row, down until it and a cut of the next clear the fade; the last control goes to the top.
pub fn reveal(rows: &[Row], heights: &[f32], cursor: usize, scroll: f32, viewport: f32) -> f32 {
    let total: f32 = heights.iter().sum();
    let most = (total - viewport).max(0.);
    let top: f32 = heights.iter().take(cursor).sum();
    let Some(height) = heights.get(cursor) else {
        return scroll.clamp(0., most);
    };
    if !rows.iter().skip(cursor + 1).any(focusable) {
        return top.min(most);
    }
    if top < scroll {
        return top;
    }
    let mut bottom = top + height + FADE;
    if let Some(next) = rows.get(cursor + 1) {
        bottom =
            bottom.max(top + height + ink_top(next, cursor + 1 == 0) + (FADE * CUT_SHARE).round());
    }
    if bottom > scroll + viewport {
        return (bottom - viewport).clamp(0., most);
    }
    scroll.clamp(0., most)
}

/// The fade shows only while more than a hairline of rows lies below the pane's foot.
pub fn fade_shown(heights: &[f32], scroll: f32, viewport: f32) -> bool {
    heights.iter().sum::<f32>() - scroll - viewport > BORDER
}

/// The card's size in a window: the board's width and as tall as View needs, both 8px clear of every edge.
pub fn card_size(window_w: f32, window_h: f32, view_h: f32) -> (f32, f32) {
    let rail = RAIL_PAD_TOP + 8. * RAIL_ROW_H + RAIL_PAD_TOP;
    let want = HEAD_H + 2. * BORDER + rail.max(view_h + PANE_FOOT);
    let span = |want: f32, room: f32| want.ceil().min(room.floor()).max(0.);
    (
        span(CARD_W, window_w - 2. * CLAMP),
        span(want, window_h - 2. * CLAMP),
    )
}

/// The pane's inside: the card less its frame and the rail across, less the head and the foot down.
pub fn pane_size(card_w: f32, card_h: f32) -> (f32, f32) {
    (
        card_w - 2. * BORDER - RAIL_W,
        card_h - 2. * BORDER - HEAD_H - BORDER - PANE_FOOT,
    )
}

/// Hint lines in a monospace face, a stand-in until the window has shaped the text: wide characters take two cells.
pub fn estimated_lines(text: &str, size: f32, width: f32) -> usize {
    let wide = |letter: char| matches!(letter as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFF00..=0xFF60);
    let cells: usize = text
        .chars()
        .map(|letter| if wide(letter) { 2 } else { 1 })
        .sum();
    let per_line = ((width / (size * 0.6)).floor() as usize).max(1);
    cells.div_ceil(per_line).max(1)
}
