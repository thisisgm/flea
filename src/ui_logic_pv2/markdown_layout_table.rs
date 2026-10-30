//! A table chunk laid at its block's width: columns shared by the fit, the header and body rows with their rules, and the sideways lane.

use super::{
    prose, Ctx, Geometry, Item, Voice, CELL_GAP, CELL_PAD, FALLBACK_GLYPH_EM, OVERFLOW_PX,
};
use crate::ui_logic_pv2::markdown::markdown_fit::table_fit;
use crate::ui_logic_pv2::markdown::markdown_flow::{Face, Ink, Measure, SLACK};
use crate::ui_logic_pv2::markdown::markdown_model::{Align, CellAlign, Table};

struct CellMeasure<'a>(&'a mut dyn Measure);

impl Measure for CellMeasure<'_> {
    fn width(&mut self, text: &str, face: Face, px: f32) -> f32 {
        self.0.table_width(text, face, px)
    }

    fn extent(&mut self, face: Face, px: f32) -> (f32, f32) {
        self.0.extent(face, px)
    }
}

fn shaped_cell(markup: &str, voice: &Voice, width: f32, ctx: &mut Ctx) -> Geometry {
    let mut measure = CellMeasure(&mut *ctx.measure);
    let mut cell = Ctx {
        metrics: ctx.metrics,
        measure: &mut measure,
        assets: ctx.assets,
        counter: ctx.counter,
        remote: ctx.remote,
    };
    let geometry = prose(markup, Some(voice), 0, width, &mut cell);
    ctx.counter = cell.counter;
    geometry
}

/// The widest line a markup draws in the bold body face, unwrapped: what a column's measure text asks of it.
pub fn natural_width(markup: &str, ctx: &mut Ctx) -> f32 {
    if markup.is_empty() {
        return 0.0;
    }
    let voice = Voice {
        px: ctx.metrics.body,
        line: super::Metrics::line(ctx.metrics.body),
        face: Face::Bold,
        ink: Ink::Foreground,
        align: None,
    };
    let laid = shaped_cell(markup, &voice, f32::INFINITY, ctx);
    laid.items
        .iter()
        .map(|item| match item {
            Item::Text { x, laid, .. } => x + laid.width,
            _ => 0.0,
        })
        .fold(0.0, f32::max)
}

/// Each column's width: natural when the table fits, else shared so every column keeps its longest word whole first.
pub fn widths(table: &Table, avail: f32, ctx: &mut Ctx) -> Vec<f32> {
    let columns = table.cols.max(1);
    let text_px: Vec<f32> = (0..columns)
        .map(|c| natural_width(table.measure.get(c).map_or("", String::as_str), ctx))
        .collect();
    let column_px: Vec<f32> = (0..columns)
        .map(
            |c| match table.pictures.get(c).filter(|picture| !picture.is_empty()) {
                Some(picture) => text_px[c].max(natural_width(picture, ctx)),
                None => text_px[c],
            },
        )
        .collect();
    // The parser weighs text alone, so a glyph's price divides the measured text by its weight.
    let (px, weight) = (0..columns)
        .filter(|c| table.weights.get(*c).is_some_and(|w| *w > 0))
        .fold((0.0, 0usize), |(px, weight), c| {
            (px + text_px[c], weight + table.weights[c])
        });
    let glyph = if weight > 0 {
        px / weight as f32
    } else {
        ctx.metrics.body * FALLBACK_GLYPH_EM
    };
    let natural: Vec<f64> = column_px
        .iter()
        // The placer's own slack, so accumulated advances cannot add a whole pixel to an integer width.
        .map(|w| f64::from((w - SLACK).max(0.0).ceil() + CELL_GAP))
        .collect();
    let minimum: Vec<f64> = (0..columns)
        .map(|c| {
            f64::from((table.words.get(c).copied().unwrap_or(0) as f32 * glyph).ceil() + CELL_GAP)
        })
        .collect();
    // A wide glyph takes two, so no column goes under two glyphs of text.
    let least = f64::from(CELL_GAP + (2.0 * glyph).ceil());
    let fitted = if avail > 0.0 {
        table_fit(&natural, &minimum, f64::from(avail), least)
    } else {
        natural
    };
    fitted.into_iter().map(|w| w as f32).collect()
}

fn align_of(table: &Table, c: usize) -> Option<Align> {
    match table.aligns.get(c) {
        Some(CellAlign::Center) => Some(Align::Center),
        Some(CellAlign::Right) => Some(Align::Right),
        _ => None,
    }
}

// One row of cells from the top, each inside its column less the gap the next cell starts after. A row whose cells are
// all one line takes the board's row height and centres its text; a row with a wrapped cell keeps the oracle's padded
// height. Returns the row's height above its rule.
fn row(
    cells: &[&str],
    head: bool,
    table: &Table,
    widths: &[f32],
    y: f32,
    items: &mut Vec<Item>,
    ctx: &mut Ctx,
) -> f32 {
    let line = super::Metrics::line(ctx.metrics.body);
    let mut laid_cells = Vec::with_capacity(cells.len());
    let mut x = 0.0;
    for (c, cell) in cells.iter().enumerate() {
        let column = widths.get(c).copied().unwrap_or(0.0);
        let voice = Voice {
            px: ctx.metrics.body,
            line,
            face: if head { Face::Bold } else { Face::Regular },
            ink: if head { Ink::Bright } else { Ink::Foreground },
            align: align_of(table, c),
        };
        let laid = shaped_cell(cell, &voice, (column - CELL_GAP).max(0.0), ctx);
        laid_cells.push((laid, x));
        x += column;
    }
    let single = laid_cells.iter().all(|(laid, _)| laid.height <= line);
    let content = if single {
        ctx.metrics.table_row(head) - ctx.metrics.hairline
    } else {
        // An empty cell still holds its line, as a text of no characters does.
        let tallest = laid_cells
            .iter()
            .map(|(laid, _)| laid.height.max(line))
            .fold(0.0, f32::max);
        tallest + 2.0 * CELL_PAD
    };
    for (laid, x) in laid_cells {
        let top = if single {
            (content - laid.height) / 2.0
        } else {
            CELL_PAD
        };
        let mut geometry = Geometry::default();
        geometry.absorb(laid, x, y + top);
        items.extend(geometry.items);
    }
    content
}

/// The chunk's header and body rows, a faint rule under each, and the lane a sideways table keeps under its last chunk.
pub fn table(table: &Table, avail: f32, ctx: &mut Ctx) -> Geometry {
    let widths = widths(table, avail, ctx);
    let table_width: f32 = widths.iter().sum();
    let overflows = avail > 0.0 && table_width - avail > OVERFLOW_PX;
    let lane = overflows && table.last != Some(false);
    let hairline = ctx.metrics.hairline;
    let mut items = Vec::new();
    let mut y = 0.0;
    if !table.head.is_empty() {
        let head: Vec<&str> = table.head.iter().map(String::as_str).collect();
        y += row(&head, true, table, &widths, y, &mut items, ctx);
        items.push(Item::Rule {
            x: 0.0,
            y,
            width: table_width,
            wash: true,
        });
        y += hairline;
    }
    let columns = table.cols.max(1);
    for cells in &table.rows {
        let cells: Vec<&str> = (0..columns)
            .map(|c| cells.get(c).map_or("", String::as_str))
            .collect();
        y += row(&cells, false, table, &widths, y, &mut items, ctx);
        items.push(Item::Rule {
            x: 0.0,
            y,
            width: table_width,
            wash: true,
        });
        y += hairline;
    }
    let height = y + if lane { ctx.metrics.row_pad_x } else { 0.0 };
    if overflows {
        return Geometry {
            height,
            items: vec![Item::Table {
                key: table.key,
                x: 0.0,
                y: 0.0,
                width: avail,
                content: table_width,
                height,
                items,
            }],
            first_baseline: None,
        };
    }
    Geometry {
        height,
        items,
        first_baseline: None,
    }
}
