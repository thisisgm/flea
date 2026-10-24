//! The pending run of paragraph lines written as blocks: HTML headings, images and display formulas lifted out, the rest parsed inline.

use super::super::markdown_html_block::{nest_depth, separate_blocks, split_html_images, Part};
use super::super::markdown_html_heading::{
    drawn_picture, heading_picture, heading_unit, html_heading, HtmlHeading,
};
use super::super::markdown_html_image::image_unit;
use super::super::markdown_leaf::{heading_safe, standalone_image};
use super::super::markdown_math::{inline_sources, split_display, Piece};
use super::super::markdown_model::{Align, Block, Figure, FigureKind};
use super::super::markdown_paragraphs::cut_after;
use super::super::markdown_resolve::plain_text;
use super::super::markdown_run::Shape;
use super::super::markdown_text::{at, blank, char_at, is_space};
use super::Writer;

impl Writer<'_, '_> {
    // Prose outside every container: a ">" in it, even after a formula's figure, is text and never a quote mark.
    fn push_piece(&mut self, lines: &[String]) {
        let text = separate_blocks(lines).join("\n");
        let shape = Shape {
            bare_text: true,
            ..Shape::default()
        };
        let inlined = collapse_break_newlines(&self.inline_with(&text, true, shape));
        // The text's formulas are listed on its first block, so a split never lists one twice.
        let mut maths = inline_sources(&text);
        for piece in split_html_after_blank(&inlined) {
            if blank(piece) {
                continue;
            }
            self.out.push(Block::Run {
                text: piece.to_string(),
                maths: std::mem::take(&mut maths),
            });
        }
    }

    // An image inside an HTML block leaves it as an image block, since the importer draws no Markdown there.
    fn push_images(&mut self, lines: &[String]) {
        for part in split_html_images(lines, self.ctx.dir) {
            match part {
                Part::Block(block) => self.out.push(block),
                Part::Lines(lines) => self.push_piece(&lines),
                Part::RowBreak => {}
            }
        }
    }

    // A display formula inside a paragraph stands on its own as a figure, the way a display block does.
    fn push_run(&mut self, lines: &[String]) {
        let Some(pieces) = split_display(&lines.join("\n")) else {
            return self.push_images(lines);
        };
        for piece in pieces {
            match piece {
                Piece::Text(text) => {
                    self.push_images(&text.split('\n').map(str::to_string).collect::<Vec<_>>())
                }
                Piece::Math(source) if !source.is_empty() => self.out.push(math_figure(source)),
                Piece::Math(_) => {}
            }
        }
    }

    // The blocks an HTML heading draws as, its leading picture an image block; None when its drawn text still holds a picture.
    fn heading_blocks(&mut self, head: HtmlHeading) -> Option<Vec<Block>> {
        let mut blocks = Vec::new();
        let mut inner = head.inner;
        let logo = if head.align == Some(Align::Right) {
            None
        } else {
            heading_picture(&inner, self.ctx.dir)
        };
        if let Some((mut block, rest)) = logo {
            if head.align == Some(Align::Center) {
                match &mut block {
                    Block::Image(image) => image.align = Some(Align::Center),
                    Block::Remote { align, .. } => *align = Some(Align::Center),
                    _ => {}
                }
            }
            blocks.push(block);
            inner = rest;
            if blank(&inner) {
                return Some(blocks);
            }
        }
        let text = self.inline_with(&heading_safe(&inner), true, Shape::default());
        if drawn_picture(&text) {
            return None;
        }
        blocks.push(Block::Heading {
            level: head.level,
            text,
            align: head.align,
        });
        Some(blocks)
    }

    fn flush_plain(&mut self, plain: &mut Vec<String>) {
        if !plain.is_empty() {
            let lines = std::mem::take(plain);
            self.push_run(&lines);
        }
    }

    pub(super) fn flush_run(&mut self) {
        let run = std::mem::take(&mut self.run);
        // cut_after says after which blanks the paragraph ends; open HTML across the blank joins it instead.
        let cut = cut_after(&run);
        let mut plain: Vec<String> = Vec::new();
        let mut depth = 0;
        let mut i = 0;
        while i < run.len() {
            if blank(&run[i]) {
                if cut[i] {
                    self.flush_plain(&mut plain);
                    depth = 0;
                } else {
                    plain.push(run[i].clone());
                }
                i += 1;
                continue;
            }
            // A heading in a lone wrapper lifts with it; one in other open HTML stays in its run, so no wrapper is ever split.
            let lifted = if depth == 0 {
                heading_unit(&run, i)
            } else {
                None
            };
            let head = match &lifted {
                Some(unit) => Some(unit.head.clone()),
                None if depth == 0 => html_heading(&run[i]),
                None => None,
            };
            if let Some(drawn) = head.and_then(|head| self.heading_blocks(head)) {
                self.flush_plain(&mut plain);
                depth = 0;
                self.out.extend(drawn);
                if let Some(unit) = lifted {
                    i = unit.end;
                    plain = unit.wrapper;
                }
                i += 1;
                continue;
            }
            let solo = (i == 0 || blank(&run[i - 1])) && (i + 1 == run.len() || blank(&run[i + 1]));
            let image = if solo {
                standalone_image(&run[i], self.ctx.dir, &self.shared.refs.defs)
            } else {
                None
            };
            // A raw image inside its own paragraph or div, on one line or three, is an image block too.
            let unit = if image.is_none() {
                image_unit(&run, i, self.ctx.dir)
            } else {
                None
            };
            if let Some(mut image) = image {
                if let Block::Image(picture) = &mut image {
                    let shape = Shape {
                        fragment: true,
                        ..Shape::default()
                    };
                    picture.alt = plain_text(&self.inline_with(&picture.alt.clone(), false, shape));
                }
                self.flush_plain(&mut plain);
                depth = 0;
                self.out.push(image);
            } else if let Some((block, end, wrapper)) = unit {
                self.flush_plain(&mut plain);
                depth = 0;
                self.out.push(block);
                i = end;
                // What its wrapper held under the image is drawn centred under it.
                plain = wrapper;
            } else {
                plain.push(run[i].clone());
                depth = nest_depth(&run[i], depth);
            }
            i += 1;
        }
        self.flush_plain(&mut plain);
    }
}

pub(super) fn math_figure(source: String) -> Block {
    Block::Figure(Figure {
        kind: FigureKind::Math,
        source,
        display: true,
    })
}

// "<br />" then a single soft newline collapses, as in a browser, so the next line starts at the text column.
fn collapse_break_newlines(text: &str) -> String {
    const BREAK: &str = "<br />";
    let mut out = String::with_capacity(text.len());
    let mut plain = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find(BREAK).map(|found| found + from) {
        let mut j = found + BREAK.len();
        while matches!(at(text, j), b' ' | b'\t') {
            j += 1;
        }
        from = found + 1;
        if at(text, j) != b'\n' {
            continue;
        }
        let mut k = j + 1;
        while matches!(at(text, k), b' ' | b'\t') {
            k += 1;
        }
        if at(text, k) == b'\n' {
            continue;
        }
        out.push_str(&text[plain..found + BREAK.len()]);
        plain = k;
        from = k;
    }
    out.push_str(&text[plain..]);
    out
}

// The text split before each block-level raw tag that follows a blank line, which the importer would join onto the paragraph above.
fn split_html_after_blank(text: &str) -> Vec<&str> {
    split_html_after_blank_counted(text, &mut 0)
}

// The same split, counting the newline-run bytes the scan walks. The run cache keeps the walk linear: each run
// scans once, so the count grows with the input length, not its square.
pub(crate) fn split_html_after_blank_counted<'t>(
    text: &'t str,
    walked: &mut usize,
) -> Vec<&'t str> {
    let mut pieces = Vec::new();
    let mut piece_from = 0;
    let mut p = 0;
    // The newline run last scanned, as (from, to, html): every newline inside it ends at the same place, so a blank run scans once.
    let mut scanned = (0, 0, false);
    while let Some(nl) = text[p..].find('\n').map(|nl| nl + p) {
        let mut j = nl + 1;
        while matches!(at(text, j), b' ' | b'\t') {
            j += 1;
        }
        if j < scanned.0 || j > scanned.1 {
            let end = j + text[j..].bytes().take_while(|b| *b == b'\n').count();
            *walked += end - j;
            scanned = (j, end, html_block_ahead(&text[end..]));
        }
        let end = scanned.1;
        if end > j && scanned.2 {
            pieces.push(&text[piece_from..nl]);
            piece_from = end;
            p = piece_from;
        } else {
            p = nl + 1;
        }
    }
    pieces.push(&text[piece_from..]);
    pieces
}

// ' <div>' and '<h2 x>' open a block the importer starts its own run with; '<span>' does not.
fn html_block_ahead(text: &str) -> bool {
    let i = text.bytes().take(3).take_while(|b| *b == b' ').count();
    if at(text, i) != b'<' {
        return false;
    }
    let len = text[i + 1..]
        .bytes()
        .take_while(u8::is_ascii_alphanumeric)
        .count();
    let name = text[i + 1..i + 1 + len].to_ascii_lowercase();
    let known = matches!(
        name.as_str(),
        "p" | "div" | "hr" | "table" | "ul" | "ol" | "blockquote" | "pre"
    ) || (name.len() == 2
        && name.starts_with('h')
        && (b'1'..=b'6').contains(&name.as_bytes()[1]));
    let after = char_at(text, i + 1 + len);
    known && after.is_some_and(|c| c == '>' || c == '/' || is_space(c))
}
