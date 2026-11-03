//! The path surfaces' geometry at the text size: the field, the popup and its rows, from the formulas the chrome and list use.
use super::suggestions::FRAME;
use crate::list::{Metrics, Typeface, CONTROL_PADDING_Y, STOCK_BASE};

// Board path-suggest: the popup opens 3 px under the chrome's rule and keeps 10 px clear beyond the gap above the status bar.
const RULE_GAP: f32 = 3.;
// The 10 px clear scales with the text like the menu's padding, so it reads 10 at body 14.
const STATUS_EXTRA: f32 = 10.;
// Board: the 13 px field draws a 16 px selection wash and a 17 px caret, the menu's padding is 5 px at 14.
const WASH_RATIO: f32 = 1.2;
const CARET_RATIO: f32 = 1.3;
const LINE_RATIO: f32 = 1.2;
const FADE_RATIO: f32 = 0.7;
const MENU_PAD: f32 = 5.;
const BOARD_BODY: f32 = 14.;

/// Every number the path surfaces place at the text size; at body 14 each equals the board's pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scale {
    pub field: f32,
    pub inset: f32,
    pub wash: f32,
    pub caret: f32,
    pub chrome: f32,
    pub row: f32,
    pub line: f32,
    pub pad: f32,
    pub pad_x: f32,
    pub gap: f32,
    pub mark: f32,
    pub fade: f32,
    pub clear: f32,
}

impl Scale {
    pub fn at(typeface: &Typeface) -> Self {
        let metrics = Metrics::at(typeface.body);
        let body = f32::from(typeface.body);
        let field = f32::from(typeface.body_small);
        let pad = (MENU_PAD * body / BOARD_BODY).round();
        // Theme.rowHeight, the menu's row: the list's slot plus a full control padding each side.
        let padding = (CONTROL_PADDING_Y * body / STOCK_BASE).round();
        let row = metrics.slot + 2. * padding;
        Self {
            field,
            inset: metrics.gap,
            wash: (field * WASH_RATIO).round(),
            caret: (field * CARET_RATIO).round(),
            chrome: metrics.header,
            row,
            line: body * LINE_RATIO,
            pad,
            pad_x: metrics.pad,
            gap: metrics.gap,
            mark: metrics.mark.floor(),
            fade: pad + FRAME + (FADE_RATIO * row).round() - 1.,
            clear: metrics.gap + (STATUS_EXTRA * body / BOARD_BODY).round(),
        }
    }

    /// The top of the popup: the chrome's height plus the gap under its rule.
    pub fn top(&self) -> f32 {
        self.chrome + RULE_GAP
    }

    /// The top of row index inside the popup's inner box at scroll: the padding, then the rows.
    pub fn row_top(&self, index: usize, scroll: f32) -> f32 {
        self.pad + index as f32 * self.row - scroll
    }

    /// The label column's left edge inside a row: past the mark and its gap.
    pub fn label_x(&self) -> f32 {
        self.pad_x + self.mark + self.gap
    }

    /// The mark's box top inside its row, centred and on a whole pixel.
    pub fn mark_top(&self) -> f32 {
        ((self.row - self.mark) / 2.).round()
    }

    /// The label's line box top inside its row, centred and on a whole pixel.
    pub fn label_top(&self) -> f32 {
        ((self.row - self.line) / 2.).round()
    }

    /// The typed run's wash in a row: the line box, on whole pixels.
    pub fn wash_line(&self) -> f32 {
        self.line.round()
    }
}
