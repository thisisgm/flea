//! The Settings card's measures from the settings boards, and the pane's scroll rules from ui/SettingsPane.qml.
use flea::ui_logic_settings_pages::{
    rows::{focusable, Kind, Row},
    visible, Text,
};

// At text size 14 the 630 by 684 card has a 27px head, 168px rail, 29px rail rows and 37px pane rows.
pub const CARD_W: f32 = 630.;
pub const BOARD_CARD_H: f32 = 684.;
pub const CLAMP: f32 = 8.;
pub const BORDER: f32 = 1.;
pub const RAIL_W: f32 = 168.;
pub const RAIL_PAD_TOP: f32 = 10.;
pub const RAIL_MARK: f32 = 15.;
pub const PAD_X: f32 = 14.;
pub const GAP: f32 = 9.;
pub const MARK_SLOT: f32 = 19.;
pub const ROW_MARK: f32 = 18.;
pub const BRAND_MARK: f32 = 16.;
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
// A hint: 4 over, 2 under, 18px lines; the footer: 10 over its hairline, 10 and 12 around 16px lines.
pub const HINT_LINE: f32 = 18.;
pub const FOOTER_LINE: f32 = 16.;
// The fade over a cut row, seven tenths of a row, and the share of it the next row's first ink shows under.
pub const FADE: f32 = 26.;
const CUT_SHARE: f32 = 0.5;
// LANGUAGES-SPEC: a colliding label wraps inside the row's 8px padding, and two of these line boxes make the row 51.
pub const WRAP_LINE: f32 = 17.5;
// A label never narrows past one letter and its ellipsis, in body sizes.
pub const LABEL_FLOOR: f32 = 1.2;
// LANGUAGES-SPEC wraps a label into two line boxes at most; a longer wrap elides on one line instead.
pub const LABEL_LINES: usize = 2;
// A control row's value and caption stop at these shares of the pane.
pub const VALUE_SHARE: f32 = 0.56;
pub const CAPTION_SHARE: f32 = 0.5;
// View's four marks: each 16 wide, 17 apart.
pub const VIEW_MARK: f32 = 16.;
pub const VIEW_MARK_GAP: f32 = 17.;
// A chip's text inset on each side.
pub const CHIP_PAD: f32 = 7.;
// Layout lands each box on a whole pixel, so the label's maths keeps this much in hand.
pub const LAYOUT_SLACK: f32 = 1.;
// The row padding a control's ink starts under.
pub(super) const ROW_PAD_Y: f32 = 8.;
const BOARD_BODY: f32 = 14.;

// Theme.railRowRatio: a rail row is 0.78 of a padded row.
const RAIL_ROW_RATIO: f32 = 0.78;

/// The boards draw Settings at text size 14; v0.3.12 grows the card, its rail and every row with the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    ratio: f32,
    /// Theme.rowHeight, Theme.railRowHeight and Theme.chromeHeight, rounded as the list rounds them.
    pub row: f32,
    pub rail_row: f32,
    pub head: f32,
}

impl Scale {
    pub fn at(face: &crate::list::Typeface) -> Self {
        let metrics = crate::list::Metrics::for_typeface(face);
        Self {
            ratio: f32::from(face.body) / BOARD_BODY,
            row: metrics.nominal_row,
            rail_row: (metrics.nominal_row * RAIL_ROW_RATIO).round(),
            head: metrics.header,
        }
    }

    /// A board measure at this size, on a whole pixel; hairlines stay one pixel.
    pub fn of(self, board: f32) -> f32 {
        (board * self.ratio).round()
    }

    /// A board line box at this size, unrounded as the board's line heights are.
    pub fn line(self, board: f32) -> f32 {
        board * self.ratio
    }

    /// The label column: the row's padding, the mark slot and the gap.
    pub fn label_x(self) -> f32 {
        self.of(PAD_X) + self.of(MARK_SLOT) + self.of(GAP)
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::at(&crate::list::Typeface::omarchy_default())
    }
}

/// A row's height with its wrapped line count at a text size; only hints wrap.
pub fn row_height(row: &Row, first: bool, lines: usize, s: Scale) -> f32 {
    let lines = if row.elide_right { 1 } else { lines.max(1) } as f32;
    let heading = s.line(GROUP_LINE) + s.line(7.8);
    match row.kind {
        Kind::Group if first => s.of(16.) + heading,
        Kind::Group => s.of(7.) + BORDER + s.of(19.) + heading,
        Kind::Hint if row.footer => {
            s.of(10.) + BORDER + s.of(10.) + s.of(FOOTER_LINE) * lines + s.of(12.)
        }
        Kind::Hint => s.of(4.) + s.of(HINT_LINE) * lines + s.of(2.),
        Kind::Ruler => s.of(RULER_H),
        Kind::KeyPreview => {
            let pairs = row.items.len().div_ceil(2) as f32;
            2. * s.of(7.) + s.of(CHIP_H) * pairs + s.of(GAP) * (pairs - 1.).max(0.)
        }
        Kind::Hero => s.of(HERO_H),
        Kind::Favourite => s.of(FAVOURITE_H),
        _ if lines > 1. => 2. * s.of(ROW_PAD_Y) + s.line(WRAP_LINE) * lines,
        _ => s.row,
    }
}

/// What a control row gives its label, value and caption, in SettingsRow.qml's order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    pub label: f32,
    /// The value's widest draw, under VALUE_SHARE and never into the label's longest word.
    pub value: f32,
    /// The caption's widest draw, under CAPTION_SHARE and never into the label's longest word.
    pub caption: f32,
}

/// A control row's room: the fixed parts come off first, then the value and the caption take what the label's longest `word` leaves, so a label wraps between words and never inside one.
pub fn control_room(
    row: &Row,
    segment: bool,
    width: f32,
    s: Scale,
    word: f32,
    body: impl Fn(&Text) -> f32,
    caption: impl Fn(&Text) -> f32,
) -> Room {
    let segment = row.kind == Kind::Choice && segment && row.labels.len() > 1;
    let steps = row.kind == Kind::Choice && !segment;
    let mut fixed = Vec::new();
    if segment && visible::marks(row) {
        let count = row.labels.len() as f32;
        fixed.push(count * s.of(VIEW_MARK) + (count - 1.) * s.of(VIEW_MARK_GAP) + s.of(GAP));
    } else if segment {
        let chips: f32 = row
            .labels
            .iter()
            .map(|label| (caption(label) + 2. * s.of(CHIP_PAD) + 2. * BORDER).max(s.of(CHIP_H)))
            .sum();
        fixed.push(chips + s.of(CHIP_GAP) * (row.labels.len() - 1) as f32);
    }
    if steps || (row.kind == Kind::Action && !row.inert) || row.kind == Kind::Lock {
        fixed.push(s.of(MARK_SLOT));
    }
    if row.kind == Kind::Check {
        fixed.push(s.of(CHECK));
    }
    let mut free =
        width - s.label_x() - s.of(PAD_X) - fixed.iter().map(|part| part + s.of(GAP)).sum::<f32>();
    let mut room = Room {
        label: 0.,
        value: 0.,
        caption: 0.,
    };
    // A clamped value or caption leaves the word plus the slack the label keeps in hand, so its box still holds the word.
    let word = word + 2. * LAYOUT_SLACK;
    if matches!(row.kind, Kind::Fact | Kind::Action)
        || steps
        || (row.kind == Kind::Check && !row.value.is_empty())
    {
        room.value = (width * VALUE_SHARE).min((free - s.of(GAP) - word).max(0.));
        free -= body(&row.value).min(room.value) + s.of(GAP);
    }
    if !row.caption.is_empty() {
        room.caption = (width * CAPTION_SHARE)
            .round()
            .min((free - s.of(GAP) - word).max(0.));
        free -= caption(&row.caption).min(room.caption) + s.of(GAP);
    }
    room.label = free - LAYOUT_SLACK;
    room
}

/// Every row's height, from the hint and colliding label lines the window shaped, and one line elsewhere.
pub fn heights(rows: &[Row], lines: &[usize], s: Scale) -> Vec<f32> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| row_height(row, index == 0, lines.get(index).copied().unwrap_or(1), s))
        .collect()
}

/// Where a row's first ink starts: a heading's label under its rule and padding, any other row under its padding.
fn ink_top(row: &Row, first: bool, s: Scale) -> f32 {
    match row.kind {
        Kind::Group if first => s.of(16.),
        Kind::Group => s.of(7.) + BORDER + s.of(19.),
        _ => s.of(ROW_PAD_Y),
    }
}

/// The scroll after a cursor move: up to its row, down until it and a cut of the next clear the fade; the last control goes to the top.
pub fn reveal(
    rows: &[Row],
    heights: &[f32],
    cursor: usize,
    scroll: f32,
    viewport: f32,
    s: Scale,
) -> f32 {
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
    let fade = s.of(FADE);
    let mut bottom = top + height + fade;
    if let Some(next) = rows.get(cursor + 1) {
        bottom = bottom
            .max(top + height + ink_top(next, cursor + 1 == 0, s) + (fade * CUT_SHARE).round());
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

/// The card's size in a window: the board's width and as tall as View needs at the text size, both 8px clear of every edge.
pub fn card_size(window_w: f32, window_h: f32, view_h: f32, s: Scale) -> (f32, f32) {
    let rail = s.of(RAIL_PAD_TOP) + 8. * s.rail_row + s.of(RAIL_PAD_TOP);
    let want = s.head + 2. * BORDER + rail.max(view_h + s.of(PANE_FOOT));
    let span = |want: f32, room: f32| want.ceil().min(room.floor()).max(0.);
    (
        span(s.of(CARD_W), window_w - 2. * CLAMP),
        span(want, window_h - 2. * CLAMP),
    )
}

/// The pane's inside: the card less its frame and the rail across, less the head, whose rule it holds, and the foot down.
pub fn pane_size(card_w: f32, card_h: f32, s: Scale) -> (f32, f32) {
    (
        card_w - 2. * BORDER - s.of(RAIL_W),
        card_h - 2. * BORDER - s.head - s.of(PANE_FOOT),
    )
}

/// Wide characters (CJK and fullwidth forms) take two monospace cells and may break between any two of them.
fn wide(letter: char) -> bool {
    matches!(letter as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFF00..=0xFF60)
}

/// Closing punctuation never starts a line.
fn closing(letter: char) -> bool {
    matches!(
        letter,
        ',' | '.'
            | ':'
            | ';'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
            | '、'
            | '。'
            | '，'
            | '．'
            | '：'
            | '；'
            | '！'
            | '？'
            | '）'
            | '］'
            | '｝'
            | '」'
            | '』'
            | '】'
            | '〉'
            | '》'
            | '〕'
            | '・'
            | 'ー'
    )
}

/// Opening punctuation never ends a line.
fn opening(letter: char) -> bool {
    matches!(
        letter,
        '(' | '[' | '{' | '（' | '［' | '｛' | '「' | '『' | '【' | '〈' | '《' | '〔'
    )
}

/// A hint's layout units: each word whole with its trailing space (a key chord is one word), split only beside a wide character, never inside a grapheme, before closing or after opening punctuation.
pub fn hint_units(text: &str) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut units: Vec<String> = Vec::new();
    for (at, word) in words.iter().enumerate() {
        let mut run = String::new();
        let mut before: Option<char> = None;
        for grapheme in word.graphemes(true) {
            let first = grapheme.chars().next().expect("a grapheme has a character");
            if let Some(last) = before {
                if (wide(last) || wide(first)) && !closing(first) && !opening(last) {
                    units.push(std::mem::take(&mut run));
                }
            }
            run.push_str(grapheme);
            before = Some(first);
        }
        if !run.is_empty() {
            units.push(run);
        }
        if at + 1 < words.len() {
            if let Some(last) = units.last_mut() {
                last.push(' ');
            }
        }
    }
    units
}

/// Lines the units take packed whole in a width: a unit that does not fit starts a line, a wider one keeps its own.
pub fn hint_line_count(widths: &[f32], width: f32) -> usize {
    let mut lines = 1;
    let mut used = 0.;
    for unit in widths {
        if used > 0. && used + unit > width {
            lines += 1;
            used = *unit;
        } else {
            used += unit;
        }
    }
    lines
}

/// Hint lines in a monospace face, a stand-in until the window has shaped the text: wide characters take two cells.
pub fn estimated_lines(text: &str, size: f32, width: f32) -> usize {
    let cells = |unit: &str| {
        unit.chars()
            .map(|letter| if wide(letter) { 2 } else { 1 })
            .sum::<usize>() as f32
    };
    let widths: Vec<f32> = hint_units(text).iter().map(|unit| cells(unit)).collect();
    hint_line_count(&widths, (width / (size * 0.6)).floor().max(1.))
}
