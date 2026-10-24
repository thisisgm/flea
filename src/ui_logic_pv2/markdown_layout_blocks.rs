//! Each block kind laid at a width: prose and headings, fences, figures, quotes, lists, pictures and the remote box.

use super::{stack, Ctx, FigureState, Geometry, Item, Metrics, NoSizes, RunSizes, QUOTE_BAR};
use crate::ui_logic_pv2::code::pairs;
use crate::ui_logic_pv2::markdown::markdown_flow::{
    flow, flow_lines, Atom, Base, Face, Ink, Laid, Look,
};
use crate::ui_logic_pv2::markdown::markdown_lists::{self, BULLETS};
use crate::ui_logic_pv2::markdown::markdown_model::{Align, Block, List, Quote};
use crate::ui_logic_pv2::markdown::markdown_rich::{rich, Para, Piece, Style};
use std::cell::RefCell;

#[path = "markdown_layout_pictures.rs"]
mod pictures;

use pictures::{figure_block, image_block, images, remote};

/// How a text draws when its block sets it: a heading's or a table cell's size, face, ink and alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    pub px: f32,
    /// The line box each line of the text takes, which a heading's board draws apart from 1.7 times its size.
    pub line: f32,
    pub face: Face,
    pub ink: Ink,
    pub align: Option<Align>,
}

/// One block laid at the width, its ordinals read from the counter, which it moves past its own.
pub fn layout(block: &Block, width: f32, ctx: &mut Ctx) -> Geometry {
    match block {
        Block::Run { text, maths } => prose(text, None, maths.len(), width, ctx),
        Block::Heading { level, text, align } => {
            if text.is_empty() {
                return Geometry::default();
            }
            let voice = heading_voice(&ctx.metrics, *level, *align);
            prose(text, Some(&voice), 0, width, ctx)
        }
        Block::Fence { text, info } => fence(text, info, width, ctx),
        Block::Figure(figure) => figure_block(figure, width, ctx),
        Block::Quote(quote) => quote_block(quote, width, ctx),
        Block::List(list) => list_block(list, width, ctx),
        Block::Table(table) => super::table::table(table, width, ctx),
        Block::Image(image) => image_block(image, width, ctx),
        Block::Images { items, align } => images(items, *align, width, ctx),
        Block::Remote { host, .. } => remote(host, width, ctx),
        Block::Deep { .. } => Geometry::default(),
    }
}

/// h1 and h2 draw bold in the bright ink at their board sizes; deeper levels draw bold body text in the foreground.
pub fn heading_voice(metrics: &Metrics, level: u8, align: Option<Align>) -> Voice {
    Voice {
        px: metrics.heading_px(level),
        line: metrics.heading_line(level),
        face: Face::Bold,
        ink: if level <= 2 {
            Ink::Bright
        } else {
            Ink::Foreground
        },
        align,
    }
}

fn body_base(metrics: &Metrics, ink: Ink) -> Base {
    metrics.base(metrics.body, Face::Regular, ink, None)
}

/// A line of plain text in the body face, laid unbounded: a list marker, an HTML item's marker.
fn one_line(text: &str, base: &Base, ctx: &mut Ctx) -> Laid {
    flow(
        &[Piece::Text(text.to_string(), Style::default())],
        base,
        f32::INFINITY,
        ctx.measure,
        &NoSizes,
    )
}

// An HTML paragraph inside list or quote levels starts a marker column per level; its marker sits in the last one.
fn indent(para: &Para, ctx: &mut Ctx) -> (f32, Option<(f32, Laid)>) {
    if para.indent == 0 {
        return (0.0, None);
    }
    let metrics = ctx.metrics;
    let step = ctx.measure.width(BULLETS[0], Face::Regular, metrics.body) + metrics.gap;
    let column = f32::from(para.indent) * step;
    if para.marker.is_empty() {
        return (column, None);
    }
    let marker_x = column - step;
    let laid = one_line(&para.marker, &body_base(&metrics, Ink::Foreground), ctx);
    (
        column.max(marker_x + laid.width + metrics.gap),
        Some((marker_x, laid)),
    )
}

/// A block's markup as drawn paragraphs at the width, a list gap between them; `maths` inline formulas take the next ordinals.
pub fn prose(
    markup: &str,
    voice: Option<&Voice>,
    maths: usize,
    width: f32,
    ctx: &mut Ctx,
) -> Geometry {
    let paras = rich(markup);
    let first = ctx.counter.formula;
    ctx.counter.formula += maths;
    // A formula the parser and the text disagree on stays its code span, so the run asks for none of its pictures.
    let found = paras
        .iter()
        .flat_map(|para| &para.pieces)
        .filter(|piece| matches!(piece, Piece::Formula { .. }))
        .count();
    let formulas = (maths > 0 && found == maths).then_some(first);
    let metrics = ctx.metrics;
    let mut out = Geometry::default();
    let mut y = 0.0;
    for (i, para) in paras.iter().enumerate() {
        if i > 0 {
            y += metrics.block_gap();
        }
        if para.rule {
            let line = Metrics::line(metrics.body);
            out.items.push(Item::Rule {
                x: 0.0,
                y: y + (line / 2.0).floor(),
                width,
                wash: false,
            });
            y += line;
            continue;
        }
        let voice = match (para.heading, voice) {
            (level @ 1..=6, _) => heading_voice(&metrics, level, None),
            (_, Some(voice)) => voice.clone(),
            _ => Voice {
                px: metrics.body,
                line: Metrics::line(metrics.body),
                face: Face::Regular,
                ink: Ink::Foreground,
                align: None,
            },
        };
        let mut base = metrics.base(voice.px, voice.face, voice.ink, para.align.or(voice.align));
        base.line = voice.line;
        base.pre = para.pre;
        let (text_x, marker) = indent(para, ctx);
        let sizes = RunSizes {
            assets: ctx.assets,
            formulas,
        };
        let mut laid = flow(
            &para.pieces,
            &base,
            (width - text_x).max(0.0),
            ctx.measure,
            &sizes,
        );
        // A drawn formula names its document ordinal, which the painter looks its picture up by.
        for atom in laid.lines.iter_mut().flat_map(|line| line.atoms.iter_mut()) {
            if let (Atom::Formula { index, .. }, Some(first)) = (atom, formulas) {
                *index += first;
            }
        }
        if out.first_baseline.is_none() {
            out.first_baseline = laid.first_baseline().map(|b| b + y);
        }
        if let Some((x, marker)) = marker {
            let dy = laid
                .first_baseline()
                .zip(marker.first_baseline())
                .map_or(0.0, |(a, b)| a - b);
            out.items.push(Item::Text {
                x,
                y: y + dy,
                laid: marker,
            });
        }
        let height = laid.height;
        if !laid.lines.is_empty() {
            out.items.push(Item::Text { x: text_x, y, laid });
        }
        y += height;
    }
    out.height = y;
    out
}

/// The base a fence and a failed figure's source lay their lines in: the body face on the fence's own line.
pub fn fence_base(metrics: &Metrics) -> Base {
    let mut base = body_base(metrics, Ink::Foreground);
    base.line = metrics.fence_line();
    base.pre = true;
    base
}

fn plain_look(base: &Base, ink: Ink) -> Look {
    Look {
        face: Face::Regular,
        px: base.px,
        ink,
        underline: false,
        strike: false,
        shift: 0.0,
        chip: false,
        link: None,
    }
}

// Preformatted text on the code surface, the fence's pads around it.
fn grounded(
    lines: &[Vec<(Look, String)>],
    fence: Option<usize>,
    width: f32,
    ctx: &mut Ctx,
) -> Geometry {
    let metrics = ctx.metrics;
    let (pad_x, pad_top, pad_bottom) = metrics.fence_pad();
    let laid = flow_lines(
        lines,
        &fence_base(&metrics),
        (width - 2.0 * pad_x).max(0.0),
        ctx.measure,
    );
    let height = laid.height + pad_top + pad_bottom;
    Geometry {
        height,
        items: vec![
            Item::Ground {
                x: 0.0,
                y: 0.0,
                width,
                height,
                fence,
            },
            Item::Text {
                x: pad_x,
                y: pad_top,
                laid,
            },
        ],
        first_baseline: None,
    }
}

/// A fence draws its lines in the foreground until its code reply colours them; tabs fill to their stops.
fn fence(text: &str, info: &str, width: f32, ctx: &mut Ctx) -> Geometry {
    let ordinal = ctx.counter.fence;
    ctx.counter.fence += 1;
    let base = fence_base(&ctx.metrics);
    let named = !info.trim().is_empty();
    let lines: Vec<Vec<(Look, String)>> = text
        .split('\n')
        .enumerate()
        .map(|(i, line)| {
            let runs = if named {
                ctx.assets.code(ordinal, i + 1)
            } else {
                None
            };
            let runs = runs.unwrap_or_else(|| vec![(0, line.to_string())]);
            let borrowed: Vec<(i64, &str)> = runs
                .iter()
                .map(|(role, text)| (*role, text.as_str()))
                .collect();
            pairs(&borrowed)
                .into_iter()
                .map(|(role, text)| {
                    (
                        plain_look(
                            &base,
                            if role == 0 {
                                Ink::Foreground
                            } else {
                                Ink::Code(role)
                            },
                        ),
                        text,
                    )
                })
                .collect()
        })
        .collect();
    grounded(&lines, Some(ordinal), width, ctx)
}

/// A quote: one bar per level, each the theme's gap right of the one before, then its text or its blocks beside them.
fn quote_block(quote: &Quote, width: f32, ctx: &mut Ctx) -> Geometry {
    let parts = quote.parts.as_deref().filter(|parts| !parts.is_empty());
    if quote.text.is_empty() && parts.is_none() {
        return Geometry::default();
    }
    let levels = quote.depth.unwrap_or(1);
    let step = QUOTE_BAR + ctx.metrics.gap;
    let x = levels as f32 * step;
    let content = match parts {
        Some(parts) => stack(parts, (width - x).max(0.0), ctx),
        None => prose(&quote.text, None, 0, (width - x).max(0.0), ctx),
    };
    let mut out = Geometry {
        height: content.height,
        items: (0..levels)
            .map(|i| Item::Bar {
                x: i as f32 * step,
                y: 0.0,
                width: QUOTE_BAR,
                height: content.height,
            })
            .collect(),
        first_baseline: content.first_baseline,
    };
    out.absorb(content, x, 0.0);
    out
}

/// A list chunk: each entry's marker in its depth's column on the entry's first baseline, its text or blocks in the column after.
fn list_block(list: &List, width: f32, ctx: &mut Ctx) -> Geometry {
    let metrics = ctx.metrics;
    let cells = {
        let measure = RefCell::new(&mut *ctx.measure);
        let advance = |text: &str| {
            measure
                .borrow_mut()
                .width(text, Face::Regular, metrics.body)
        };
        markdown_lists::layout(list, &advance, metrics.gap)
    };
    let marker_base = body_base(&metrics, Ink::Foreground);
    let mut out = Geometry::default();
    let mut y = 0.0;
    for (k, cell) in cells.iter().enumerate() {
        if cell.gap {
            y += metrics.block_gap();
        }
        let text_x = cell.x + cell.w + metrics.gap;
        let text_width = (width - text_x).max(0.0);
        let parts = list
            .parts
            .as_ref()
            .and_then(|parts| parts.get(k))
            .and_then(Option::as_deref)
            .filter(|parts| !parts.is_empty());
        let content = match parts {
            Some(parts) => stack(parts, text_width, ctx),
            None => prose(
                list.items.get(k).map_or("", String::as_str),
                None,
                0,
                text_width,
                ctx,
            ),
        };
        let row = content.height.max(marker_base.line);
        if !cell.marker.is_empty() {
            let marker = one_line(&cell.marker, &marker_base, ctx);
            let dy = content
                .first_baseline
                .zip(marker.first_baseline())
                .map_or(0.0, |(a, b)| a - b);
            out.items.push(Item::Text {
                x: cell.x,
                y: y + dy,
                laid: marker,
            });
        }
        if out.first_baseline.is_none() {
            out.first_baseline = Some(y + content.first_baseline.unwrap_or(0.0));
        }
        out.absorb(content, text_x, y);
        y += row;
    }
    out.height = y;
    out
}
