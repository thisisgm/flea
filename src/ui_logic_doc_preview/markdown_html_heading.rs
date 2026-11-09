//! An HTML heading inside a wrapper or opening with a picture, lifted whole so no wrapper is split and no picture sits in a text line.

use super::markdown_html::{read_tag, tag_head};
use super::markdown_html_block::is_heading_name;
use super::markdown_html_image::{
    break_at, close_tag_at, is_centred, is_wrapper_open, link_of, open_tag_at, raw_image,
    skip_space, wrapper_tail,
};
use super::markdown_model::{Align, Block};
use super::markdown_text::{at, find_str, is_word, trim};

// The shortest heading line that holds text, "<h1>x</h1>": anything shorter is never one.
const HEADING_SHORTEST: usize = 10;

/// An HTML heading line: its level, alignment, whether it named one, and its inner markup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HtmlHeading {
    pub level: u8,
    pub align: Option<Align>,
    pub aligned: bool,
    pub inner: String,
}

/// '<h1 align="center">Flea</h1>' answers level 1, centred, inner "Flea"; '<h2>x</h2> tail' answers None.
pub fn html_heading(line: &str) -> Option<HtmlHeading> {
    let text = trim(line);
    if text.len() < HEADING_SHORTEST {
        return None;
    }
    let (open, open_end) = read_tag(text, 0, None)?;
    let head = tag_head(open);
    if !is_heading_name(&head.name) || head.closing {
        return None;
    }
    let level = head.name.as_bytes()[1] - b'0';
    let rest = &text[open_end..];
    let close_at = rest.to_ascii_lowercase().find(&format!("</h{level}"))?;
    let (close, close_end) = read_tag(text, open_end + close_at, None)?;
    let close_head = tag_head(close);
    if close_end != text.len() || close_head.name != head.name || !close_head.closing {
        return None;
    }
    let mut align = None;
    let mut aligned = false;
    for value in head
        .attributes
        .iter()
        .filter(|attr| attr.name == "align")
        .filter_map(|attr| attr.value.as_deref())
    {
        aligned = true;
        match value.to_lowercase().as_str() {
            "center" => align = Some(Align::Center),
            "right" => align = Some(Align::Right),
            _ => {}
        }
    }
    Some(HtmlHeading {
        level,
        align,
        aligned,
        inner: rest[..close_at].to_string(),
    })
}

/// A heading lifted out of its wrapper: the heading, the wrapper's closing line, and what the wrapper held after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingUnit {
    pub head: HtmlHeading,
    pub end: usize,
    pub wrapper: Vec<String>,
}

/// ['<div align="center">', '<h1>Flea</h1>', '<p>x</p>', '</div>'] at 0 lifts the heading, centred, with '<div align="center"><p>x</p></div>' after it.
pub fn heading_unit<S: AsRef<str>>(lines: &[S], at_line: usize) -> Option<HeadingUnit> {
    let open = trim(lines[at_line].as_ref());
    if !is_wrapper_open(open) || at_line + 1 >= lines.len() {
        return None;
    }
    let mut head = html_heading(lines[at_line + 1].as_ref())?;
    let name = tag_head(open).name;
    let (rest, end) = wrapper_tail(lines, at_line + 2, &name)?;
    if !head.aligned && is_centred(open) {
        head.align = Some(Align::Center);
    }
    let shown: Vec<String> = rest.into_iter().filter(|line| !line.is_empty()).collect();
    let wrapper = if shown.is_empty() {
        Vec::new()
    } else {
        vec![format!("{open}{}</{name}>", shown.join("\n"))]
    };
    Some(HeadingUnit { head, end, wrapper })
}

/// A picture opening a heading's text, '<a href="https://x/"><img src="logo.png"></a><br>Flea', as its image block and the rest.
pub fn heading_picture(inner: &str, dir: &str) -> Option<(Block, String)> {
    let mut i = skip_space(inner, 0);
    let link = open_tag_at(inner, i, &["a"]).map(|(end, _)| {
        let tag = &inner[i..end];
        i = skip_space(inner, end);
        tag
    });
    let (image_end, _) = open_tag_at(inner, i, &["img"])?;
    let image = &inner[i..image_end];
    i = skip_space(inner, image_end);
    let closed = match close_tag_at(inner, i, &["a"]) {
        Some((end, _)) => {
            i = skip_space(inner, end);
            true
        }
        None => false,
    };
    while let Some(end) = break_at(inner, i) {
        i = skip_space(inner, end);
    }
    if link.is_some() != closed {
        return None;
    }
    let mut block = raw_image(image, dir)?;
    if let (Some(link), Block::Image(picture)) = (link, &mut block) {
        if let Some(url) = link_of(link) {
            picture.link = Some(url);
        }
    }
    Some((block, inner[i..].to_string()))
}

/// True when drawn heading text holds a picture: '![](file:///d/logo.png)Flea' and '<img src="a.png">' do, 'Wow!\[beta]' does not.
pub fn drawn_picture(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower
        .match_indices("<img")
        .any(|(i, _)| !is_word(at(&lower, i + 4)))
    {
        return true;
    }
    let mut from = 0;
    while let Some(bang) = find_str(text, "![", from) {
        from = bang + 1;
        let mut chars = text[bang + 2..].chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some(next) if !matches!(next, '\n' | '\r' | '\u{2028}' | '\u{2029}') => {}
                    _ => break,
                },
                ']' => {
                    if chars.next() == Some('(') {
                        return true;
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    false
}
