//! An HTML block's local images taken out as image blocks, joined into rows, with the text around them kept.

use super::super::markdown_html::{read_tag, Dead};
use super::super::markdown_html_image::{
    is_centred, leading_breaks, link_of, open_tag_at, raw_image, skip_space,
};
use super::super::markdown_model::{Align, Block, Image};
use super::super::markdown_text::{at, blank, find_str, is_word};
use super::{
    block_start, complete_tag, holds_break, holds_tag_word, without_stray_closers, without_tags,
    BLOCK_TAG_NAMES, BOUNDARY_NAMES,
};

/// The most pictures one images block holds.
pub const ROW_IMAGE_CAP: usize = 64;
const CENTRED_WRAP: &str = "<p align=\"center\">";
const LINK_CLOSE: &str = "</a>";

/// Where a link opener ends the text before an image, as 'x <a href="https://y">' does; None when it does not.
fn link_before(text: &str) -> Option<usize> {
    let lt = text.rfind('<')?;
    let rest = &text[lt..];
    if !matches!(at(rest, 1), b'a' | b'A') || is_word(at(rest, 2)) {
        return None;
    }
    let gt = rest.find('>')?;
    blank(&rest[gt + 1..]).then_some(lt)
}

// What stays of the lines around an extracted image: lines with visible text, their stray closers gone, centred when its block was.
fn remainder(kept: &[String], centred: bool) -> Vec<String> {
    let joined = without_stray_closers(&kept.join("\n"));
    let mut lines: Vec<String> = Vec::new();
    let mut nests = false;
    for line in joined.split('\n') {
        // A line holding an image the splitter did not take (a remote one) draws its placeholder, so it stays.
        if blank(&without_tags(line)) && !holds_tag_word(line, &["hr", "img"], false, false) {
            continue;
        }
        nests = nests || holds_tag_word(line, BLOCK_TAG_NAMES, true, false);
        lines.push(line.to_string());
    }
    if let Some(first) = lines.first_mut() {
        *first = first[leading_breaks(first)..].to_string();
    }
    if lines.is_empty() {
        Vec::new()
    } else if centred && !nests {
        vec![format!("{CENTRED_WRAP}{}</p>", lines.join("\n"))]
    } else {
        lines
    }
}

/// One part of an HTML-split run: lines to parse, an image block, or a row break between pictures.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Lines(Vec<String>),
    Block(Block),
    RowBreak,
}

// Images with nothing between them join one row block of at most ROW_IMAGE_CAP; a row break or text keeps them apart.
fn image_rows(parts: Vec<Part>) -> Vec<Part> {
    let mut out = Vec::new();
    let mut row: Vec<Image> = Vec::new();
    let close = |row: &mut Vec<Image>, out: &mut Vec<Part>| {
        let all = std::mem::take(row);
        for chunk in all.chunks(ROW_IMAGE_CAP) {
            if chunk.len() == 1 {
                out.push(Part::Block(Block::Image(chunk[0].clone())));
                continue;
            }
            let align = chunk[0].align;
            let items = chunk
                .iter()
                .cloned()
                .map(|image| Image {
                    align: None,
                    ..image
                })
                .collect();
            out.push(Part::Block(Block::Images { items, align }));
        }
    };
    for part in parts {
        match part {
            Part::Block(Block::Image(image)) => row.push(image),
            Part::Lines(lines) => {
                close(&mut row, &mut out);
                out.push(Part::Lines(lines));
            }
            _ => close(&mut row, &mut out),
        }
    }
    close(&mut row, &mut out);
    out
}

// The opener a block that may centre starts with, `^\s*<(?:p|div|center)\b[^<>]*>`, leading whitespace kept.
fn centred_open(text: &str) -> Option<&str> {
    let i = skip_space(text, 0);
    open_tag_at(text, i, &["p", "div", "center"]).map(|(end, _)| &text[..end])
}

// The next local image tag in a line from a byte on, read in its lowercased form: its start, end and block.
fn find_image(
    line: &str,
    lower: &str,
    from: usize,
    dead: &mut Dead,
    dir: &str,
) -> Option<(usize, usize, Image)> {
    let mut search = from;
    while let Some(start) = find_str(lower, "<img", search) {
        if let Some((tag, end)) = read_tag(line, start, Some(dead)) {
            if let Some(Block::Image(image)) = raw_image(tag, dir) {
                return Some((start, end, image));
            }
        }
        search = start + 1;
    }
    None
}

// One HTML block's lines as parts; the block's wrappers do not outlive its image.
fn split_block(group: Vec<String>, dir: &str) -> Vec<Part> {
    let centred = centred_open(&group[0]).is_some_and(|open| {
        let i = skip_space(open, 0);
        open_tag_at(open, i, &["center"]).is_some() || is_centred(open)
    });
    let mut parts = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut found = false;
    let flush = |kept: &mut Vec<String>, parts: &mut Vec<Part>| {
        let lines = remainder(kept, centred);
        // A line break or a block boundary between two images starts the next row.
        let between = kept.join("\n");
        let broke = holds_break(&between) || holds_tag_word(&between, BOUNDARY_NAMES, true, true);
        kept.clear();
        if !lines.is_empty() {
            parts.push(Part::Lines(lines));
        } else if broke {
            parts.push(Part::RowBreak);
        }
    };
    for line in &group {
        let lower = line.to_ascii_lowercase();
        let mut dead = Dead::default();
        // The scan walks the line by position, so a line of many images is cut once, never once per image.
        let mut from = 0;
        while let Some((start, end, mut image)) = find_image(line, &lower, from, &mut dead, dir) {
            let mut before = &line[from..start];
            let next = skip_space(line, end);
            from = end;
            if let Some(wrap) = link_before(before) {
                let closes = line
                    .get(next..next + LINK_CLOSE.len())
                    .is_some_and(|close| close.eq_ignore_ascii_case(LINK_CLOSE));
                if closes {
                    if let Some(link) = link_of(&before[wrap..]) {
                        image.link = Some(link);
                    }
                    before = &before[..wrap];
                    from = next + LINK_CLOSE.len();
                }
            }
            kept.push(before.to_string());
            flush(&mut kept, &mut parts);
            if centred {
                image.align = Some(Align::Center);
            }
            parts.push(Part::Block(Block::Image(image)));
            found = true;
        }
        kept.push(line[from..].to_string());
    }
    flush(&mut kept, &mut parts);
    if found {
        image_rows(parts)
    } else {
        vec![Part::Lines(group)]
    }
}

/// A run's lines with every HTML block's local images taken out as image parts; a paragraph's inline image stays in its lines.
pub fn split_html_images(lines: &[String], dir: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut text: Vec<String> = Vec::new();
    let mut group: Option<Vec<String>> = None;
    let mut paragraph = false;
    for line in lines {
        if blank(line) {
            if let Some(group) = group.take() {
                parts.extend(split_block(group, dir));
            }
            text.push(line.clone());
            paragraph = false;
        } else if let Some(group) = group.as_mut() {
            group.push(line.clone());
        } else if block_start(line) || (!paragraph && complete_tag(line)) {
            if !text.is_empty() {
                parts.push(Part::Lines(std::mem::take(&mut text)));
            }
            group = Some(vec![line.clone()]);
        } else {
            paragraph = true;
            text.push(line.clone());
        }
    }
    if let Some(group) = group.take() {
        parts.extend(split_block(group, dir));
    }
    if !text.is_empty() {
        parts.push(Part::Lines(text));
    }
    // Lines the split left alone stay one run, as the importer joins an HTML block with the paragraph after it.
    let mut merged: Vec<Part> = Vec::new();
    for part in parts {
        match (merged.last_mut(), part) {
            (Some(Part::Lines(last)), Part::Lines(more)) => last.extend(more),
            (_, part) => merged.push(part),
        }
    }
    merged
}
