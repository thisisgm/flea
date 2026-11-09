//! One frame's ops gathered from laid blocks: grounds, glyph runs, pictures, figures, tables and their hit rects.
use super::super::doc::PictureState;
use super::super::Art;
use super::{Build, Hit, Op, TableId};
use crate::row::marks::Mark;
use flea::ui_logic_doc_preview::markdown::{
    markdown_flow::{Atom, Face, Laid},
    markdown_layout::{dashes, Item},
    Block,
};
use gpui::Hsla;
use std::sync::Arc;

/// A run's underline below its baseline and its strike above it, in ems.
const UNDERLINE_EM: f32 = 0.1;
const STRIKE_EM: f32 = 0.27;

impl Build<'_> {
    pub(super) fn select_last(&mut self, id: usize, byte: usize) {
        if let Some(Op::Text {
            x,
            baseline,
            run,
            selected,
            ..
        }) = self.ops.last_mut()
        {
            *selected = Some(super::super::selection::span(
                self.selection,
                id,
                byte,
                *x,
                *baseline,
                run,
            ));
        }
    }

    /// A table chunk's first body row, top and height from the chunk's top: the band its row rules bound, past the header's.
    fn first_body_row(&self, items: &[Item]) -> Option<(f32, f32)> {
        let headed = matches!(
            self.doc.blocks().get(self.block),
            Some(Block::Table(table)) if !table.head.is_empty()
        );
        let mut rules = items.iter().filter_map(|item| match item {
            Item::Rule { y, wash: true, .. } => Some(*y),
            _ => None,
        });
        let top = if headed {
            rules.next()? + self.hairline
        } else {
            0.
        };
        Some((top, rules.next()? - top))
    }

    fn quad(&mut self, rect: (f32, f32, f32, f32), color: Hsla) {
        self.ops.push(Op::Quad { rect, color });
    }

    pub(super) fn text(
        &mut self,
        text: &str,
        face: Face,
        size: f32,
        x: f32,
        baseline: f32,
        color: Hsla,
    ) -> f32 {
        let run = self.glyphs.get(text, face, size, self.faces, self.window);
        let width = f32::from(run.line().width);
        self.ops.push(Op::Text {
            selected: None,
            x,
            baseline,
            run,
            color,
        });
        self.first.texts += 1;
        width
    }

    fn laid(&mut self, laid: &Laid, x0: f32, y0: f32) {
        let id = super::super::selection::chunk(self.block, self.text_id);
        self.text_id += 1;
        let mut byte = 0;
        for (i, line) in laid.lines.iter().enumerate() {
            if i > 0 {
                byte += laid.lines[i - 1].after.len();
            }
            let top = y0 + line.top;
            if top > self.height || top + line.height < 0. {
                byte += line
                    .atoms
                    .iter()
                    .map(|atom| match atom {
                        Atom::Text { text, .. } => text.len(),
                        _ => 0,
                    })
                    .sum::<usize>();
                continue;
            }
            let base = top + line.baseline;
            // Chip grounds sit behind every run of the line.
            for atom in &line.atoms {
                if let Atom::Ground {
                    x,
                    width,
                    top: at,
                    height,
                } = atom
                {
                    self.quad((x0 + x, top + at, *width, *height), self.palette.code);
                }
            }
            let mut previous = None;
            for atom in &line.atoms {
                match atom {
                    Atom::Text {
                        x,
                        width,
                        text,
                        look,
                    } => {
                        if text.is_empty() {
                            continue;
                        }
                        let (x, baseline, color) =
                            (x0 + x, base + look.shift, self.palette.ink(&look.ink));
                        self.text(text, look.face, look.px, x, baseline, color);
                        self.select_last(id, byte);
                        // A chip's padding sits between two pieces, so the earlier piece's wash reaches this one.
                        let spans = &mut self.selection.spans;
                        if let Some(span) = previous.and_then(|i: usize| spans.get_mut(i)) {
                            span.reach = gpui::px(x);
                        }
                        previous = spans.len().checked_sub(1);
                        byte += text.len();
                        if look.underline {
                            self.quad(
                                (
                                    x,
                                    baseline + (look.px * UNDERLINE_EM).round().max(1.),
                                    *width,
                                    self.hairline,
                                ),
                                color,
                            );
                        }
                        if look.strike {
                            self.quad(
                                (
                                    x,
                                    baseline - (look.px * STRIKE_EM).round(),
                                    *width,
                                    self.hairline,
                                ),
                                color,
                            );
                        }
                        if let Some(url) = &look.link {
                            self.hits.push(Hit::Link {
                                rect: (x, top, *width, line.height),
                                url: url.clone(),
                            });
                        }
                    }
                    Atom::Picture {
                        x,
                        width,
                        height,
                        url,
                        link,
                        ..
                    } => {
                        previous = None;
                        let rect = (x0 + x, base - height, *width, *height);
                        self.picture(url, rect, link.as_deref());
                    }
                    Atom::Formula {
                        x,
                        width,
                        above,
                        below,
                        index,
                    } => {
                        previous = None;
                        if let Some(image) = self.doc.art.get(&Art::Formula(*index)) {
                            let rect = (x0 + x, base - above, *width, above + below);
                            self.ops.push(Op::Image {
                                rect,
                                image: Arc::clone(image),
                            });
                        }
                    }
                    Atom::Ground { .. } => {}
                }
            }
        }
    }

    fn picture(&mut self, url: &str, rect: (f32, f32, f32, f32), link: Option<&str>) {
        if let Some(PictureState::Drawn(image)) = self.doc.pictures.get(url) {
            self.ops.push(Op::Image {
                rect,
                image: Arc::clone(image),
            });
            self.first.pictures += 1;
        }
        if let Some(url) = link {
            self.hits.push(Hit::Link {
                rect,
                url: url.to_string(),
            });
        }
    }

    pub(super) fn items(&mut self, items: &[Item], x0: f32, y0: f32) {
        for item in items {
            match item {
                Item::Text { x, y, laid } => self.laid(laid, x0 + x, y0 + y),
                Item::Ground {
                    x,
                    y,
                    width,
                    height,
                    fence,
                } => {
                    self.quad((x0 + x, y0 + y, *width, *height), self.palette.code);
                    self.first.fences += usize::from(fence.is_some());
                }
                Item::Bar {
                    x,
                    y,
                    width,
                    height,
                } => self.quad((x0 + x, y0 + y, *width, *height), self.palette.muted_frame),
                Item::Rule { x, y, width, wash } => {
                    let color = if *wash {
                        self.palette.rule()
                    } else {
                        self.palette.muted_frame
                    };
                    self.quad((x0 + x, y0 + y, *width, self.hairline), color);
                }
                Item::Dashed {
                    x,
                    y,
                    width,
                    height,
                } => {
                    for (dx, dy, w, h) in dashes(*width, *height, self.hairline) {
                        self.quad((x0 + x + dx, y0 + y + dy, w, h), self.palette.muted_frame);
                    }
                }
                Item::Glyph { x, y, size } => {
                    let (key, data) = Mark::Image.svg();
                    let color = self.palette.muted_frame;
                    self.ops.push(Op::Mark {
                        rect: (x0 + x, y0 + y, *size, *size),
                        key,
                        data,
                        color,
                    });
                }
                Item::Picture {
                    x,
                    y,
                    width,
                    height,
                    url,
                    link,
                    ..
                } => {
                    self.picture(url, (x0 + x, y0 + y, *width, *height), link.as_deref());
                }
                Item::Figure {
                    x,
                    y,
                    width,
                    height,
                    ordinal,
                } => {
                    if let (true, Some(image)) =
                        (*height > 0., self.doc.art.get(&Art::Figure(*ordinal)))
                    {
                        self.ops.push(Op::Image {
                            rect: (x0 + x, y0 + y, *width, *height),
                            image: Arc::clone(image),
                        });
                        self.first.figures += 1;
                    }
                }
                Item::Table {
                    key,
                    x,
                    y,
                    width,
                    content,
                    height,
                    items,
                } => {
                    let id = key.map_or(TableId::Block(self.block, self.table), TableId::Key);
                    self.table += 1;
                    let most = (content - width).max(0.);
                    let shift = self.tables.get(&id).copied().unwrap_or(0.).clamp(0., most);
                    let rect = (x0 + x, y0 + y, *width, *height);
                    let at = self.ops.len();
                    self.ops.push(Op::Clip { rect, len: 0 });
                    let old_clip = self.selection.clip;
                    let clip = gpui::Bounds::new(
                        gpui::point(gpui::px(rect.0), gpui::px(rect.1)),
                        gpui::size(gpui::px(rect.2), gpui::px(rect.3)),
                    );
                    self.selection.clip = old_clip.intersect(&clip);
                    self.items(items, x0 + x - shift, y0 + y);
                    self.selection.clip = old_clip;
                    let len = self.ops.len() - at - 1;
                    self.ops[at] = Op::Clip { rect, len };
                    let row = self
                        .first_body_row(items)
                        .map(|(top, height)| (x0 + x - shift, y0 + y + top, *content, height));
                    self.hits.push(Hit::Table {
                        rect,
                        id,
                        most,
                        row,
                    });
                }
            }
        }
    }
}

pub(super) fn centred(
    build: &mut Build,
    sentence: &str,
    size: f32,
    (w, h): (f32, f32),
    color: Hsla,
) {
    let run = build
        .glyphs
        .get(sentence, Face::Regular, size, build.faces, build.window);
    let line = run.line();
    let x = ((w - f32::from(line.width)) / 2.).round();
    let baseline =
        ((h - f32::from(line.ascent + line.descent)) / 2.).round() + f32::from(line.ascent);
    build.ops.push(Op::Text {
        selected: None,
        x,
        baseline,
        run,
        color,
    });
}
