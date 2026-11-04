//! A standalone image line, inline or by reference, read as an image block.

use super::super::markdown_html_image::raw_image;
use super::super::markdown_link::{normalize_label, read_inline_target, read_label_ref};
use super::super::markdown_model::{Block, Image};
use super::super::markdown_text::{at, is_word, trim};
use super::super::markdown_url::{classify_image, Picture};
use std::collections::HashMap;

// The "]" that balances the "[" before pos, backslash escapes skipped.
fn scan_balanced(text: &str, pos: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0;
    let mut i = pos;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'[' => depth += 1,
            b']' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn image_block(alt: &str, target: &str, dir: &str) -> Option<Block> {
    match classify_image(target, dir) {
        Picture::Remote(host) => Some(Block::Remote { host, align: None }),
        Picture::Local(url) => Some(Block::Image(Image {
            url,
            alt: alt.to_string(),
            ..Image::default()
        })),
        Picture::Dropped => None,
    }
}

/// A line that is one picture: "![alt](a.png)", "![alt][ref]" or one raw image tag.
pub fn standalone_image(line: &str, dir: &str, defs: &HashMap<String, String>) -> Option<Block> {
    let text = trim(line);
    if text.starts_with("![") {
        if let Some(end) = scan_balanced(text, 2).filter(|end| *end > 0) {
            let alt = &text[2..end];
            let after = end + 1;
            let target = match at(text, after) {
                b'(' => {
                    let (url, stop) = read_inline_target(text, after)?;
                    (stop == text.len()).then_some(url)?
                }
                b'[' => {
                    let (label, stop) = read_label_ref(text, after)?;
                    if stop != text.len() {
                        return None;
                    }
                    defs.get(&normalize_label(if label.is_empty() { alt } else { label }))?
                        .clone()
                }
                _ if after == text.len() => {
                    if alt.is_empty() {
                        return None;
                    }
                    defs.get(&normalize_label(alt))?.clone()
                }
                _ => return None,
            };
            return image_block(alt, &target, dir);
        }
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("<img")
        && !is_word(at(text, 4))
        && text.ends_with('>')
        && !text[1..text.len() - 1].contains(['<', '>'])
    {
        return raw_image(text, dir);
    }
    None
}
