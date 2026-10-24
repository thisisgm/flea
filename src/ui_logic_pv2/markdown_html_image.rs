//! A raw HTML image on its own line, alone or inside a centring paragraph, as an image block, and the readers its tags share.

use super::markdown_html::{read_tag, tag_head};
use super::markdown_model::{Align, Block, Image};
use super::markdown_text::{at, is_space, is_word, trim};
use super::markdown_url::{canonical_url, classify_image, stripped_target, Picture};

fn lower_starts(text: &str, i: usize, prefix: &str) -> bool {
    text.len() >= i + prefix.len()
        && text.as_bytes()[i..i + prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// The index after an opening tag `<name\b[^<>]*>` at i for one of the names, any case, and the name it matched.
pub fn open_tag_at<'n>(text: &str, i: usize, names: &[&'n str]) -> Option<(usize, &'n str)> {
    if at(text, i) != b'<' {
        return None;
    }
    let name = names
        .iter()
        .copied()
        .filter(|name| lower_starts(text, i + 1, name) && !is_word(at(text, i + 1 + name.len())))
        .max_by_key(|name| name.len())?;
    let from = i + 1 + name.len();
    let close = text[from..].find(['<', '>'])? + from;
    (at(text, close) == b'>').then_some((close + 1, name))
}

/// The index after a closing tag `</name>` at i for one of the names, any case, and the name it matched.
pub fn close_tag_at<'n>(text: &str, i: usize, names: &[&'n str]) -> Option<(usize, &'n str)> {
    if at(text, i) != b'<' || at(text, i + 1) != b'/' {
        return None;
    }
    names.iter().copied().find_map(|name| {
        let end = i + 2 + name.len();
        (lower_starts(text, i + 2, name) && at(text, end) == b'>').then_some((end + 1, name))
    })
}

/// The index after the whitespace at i.
pub fn skip_space(text: &str, mut i: usize) -> usize {
    while let Some(c) = super::markdown_text::char_at(text, i) {
        if !is_space(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// The index after a `<br\s*\/?>` at i.
pub fn break_at(text: &str, i: usize) -> Option<usize> {
    if !lower_starts(text, i, "<br") {
        return None;
    }
    let mut j = skip_space(text, i + 3);
    if at(text, j) == b'/' {
        j += 1;
    }
    (at(text, j) == b'>').then_some(j + 1)
}

/// The index after the run of breaks with their whitespace at the start of the text, `^\s*(?:<br\s*\/?>\s*)+`.
pub fn leading_breaks(text: &str) -> usize {
    let mut j = skip_space(text, 0);
    let mut any = false;
    while let Some(end) = break_at(text, j) {
        any = true;
        j = skip_space(text, end);
    }
    if any {
        j
    } else {
        0
    }
}

/// True when the text holds `<img` followed by no word character.
pub fn holds_image_tag(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower
        .match_indices("<img")
        .any(|(i, _)| !is_word(at(&lower, i + 4)))
}

/// True for a whole-line wrapper opener, `<p ...>` or `<div ...>`.
pub fn is_wrapper_open(line: &str) -> bool {
    open_tag_at(line, 0, &["p", "div"]).is_some_and(|(end, _)| end == line.len())
}

/// The name a whole-line wrapper closer closes, `</p>` or `</div>`.
pub fn wrapper_close(line: &str) -> Option<&'static str> {
    close_tag_at(line, 0, &["p", "div"])
        .filter(|(end, _)| *end == line.len())
        .map(|(_, name)| name)
}

/// True when a wrapper's align attribute says center.
pub fn is_centred(open: &str) -> bool {
    tag_head(open)
        .attributes
        .iter()
        .find(|attr| attr.name == "align")
        .is_some_and(|attr| {
            attr.value
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case("center"))
        })
}

// " 64", "64px" and "64 " are sizes; "50%", "6.4" and "abc" are not.
fn size_value(value: &str) -> Option<u32> {
    let text = trim(value);
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let rest = &text[digits..];
    let unit_ok = rest.is_empty() || rest.eq_ignore_ascii_case("px");
    (digits > 0 && digits <= 4 && unit_ok)
        .then(|| text[..digits].parse().ok())
        .flatten()
        .filter(|n| *n > 0)
}

/// A raw image tag as an image block with its width and height, a remote block, or None when the tag draws nothing.
pub fn raw_image(text: &str, dir: &str) -> Option<Block> {
    let (tag, _) = read_tag(text, 0, None)?;
    let head = tag_head(tag);
    if head.name != "img" || head.closing || !head.valid_attrs {
        return None;
    }
    // First attributes win even when their values are empty or absent.
    let first = |name: &str| {
        head.attributes
            .iter()
            .find(|attr| attr.name == name)
            .map(|attr| attr.value.clone().unwrap_or_default())
    };
    match classify_image(&first("src").unwrap_or_default(), dir) {
        Picture::Remote(host) => Some(Block::Remote { host, align: None }),
        Picture::Local(url) => Some(Block::Image(Image {
            url,
            alt: first("alt").unwrap_or_default(),
            width: first("width").as_deref().and_then(size_value),
            height: first("height").as_deref().and_then(size_value),
            ..Image::default()
        })),
        Picture::Dropped => None,
    }
}

/// The size a picture draws at: a width or height attribute is honoured past the natural size, both name the box, the limit caps the width.
pub fn picture_size(
    width: f64,
    height: f64,
    natural_w: f64,
    natural_h: f64,
    limit: f64,
) -> (f64, f64, bool) {
    let mut w = width.max(0.);
    let mut h = height.max(0.);
    let stretch = w > 0. && h > 0.;
    if w == 0. && h == 0. {
        w = natural_w;
        h = natural_h;
    } else if h == 0. {
        h = if natural_w > 0. {
            w * natural_h / natural_w
        } else {
            0.
        };
    } else if w == 0. {
        w = if natural_h > 0. {
            h * natural_w / natural_h
        } else {
            0.
        };
    }
    if w > limit && w > 0. {
        h = h * limit / w;
        w = limit;
    }
    (w, h, stretch)
}

/// The http, https or mailto target of a link opener, None for any other.
pub fn link_of(open: &str) -> Option<String> {
    let head = tag_head(open);
    let attr = head
        .attributes
        .iter()
        .find(|attr| attr.name == "href" && attr.value.is_some())?;
    let url = stripped_target(&canonical_url(attr.value.as_deref().unwrap_or("")));
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http:") || lower.starts_with("https:") || lower.starts_with("mailto:"))
        .then_some(url)
}

/// How many wrapper openers of a name a line opens minus how many it closes, counted tag by tag.
pub fn nesting_steps(line: &str, name: &str, mut step: impl FnMut(bool)) {
    let lower = line.to_ascii_lowercase();
    let mut i = 0;
    while let Some(lt) = lower[i..].find('<').map(|lt| lt + i) {
        let closing = at(&lower, lt + 1) == b'/';
        let from = lt + 1 + usize::from(closing);
        let after = super::markdown_text::char_at(&lower, from + name.len()).unwrap_or('\0');
        if lower[from..].starts_with(name) && (is_space(after) || after == '>' || after == '/') {
            if let Some(close) = lower[from..].find(['<', '>']).map(|c| c + from) {
                if at(&lower, close) == b'>' {
                    step(closing);
                    i = close + 1;
                    continue;
                }
            }
        }
        i = lt + 1;
    }
}

/// The lines after a wrapper's image up to its own closer, nested openers counted, and that closer's index.
pub fn wrapper_tail<S: AsRef<str>>(
    lines: &[S],
    from: usize,
    name: &str,
) -> Option<(Vec<String>, usize)> {
    let mut rest = Vec::new();
    let mut depth: i64 = 1;
    for (j, raw) in lines.iter().enumerate().skip(from) {
        let line = trim(raw.as_ref());
        if line.is_empty() || holds_image_tag(line) {
            return None;
        }
        if depth == 1 && wrapper_close(line).is_some_and(|close| close == name) {
            return Some((rest, j));
        }
        nesting_steps(line, name, |closing| depth += if closing { -1 } else { 1 });
        // A closer sharing its line with text, or one with no opener, ends the wrapper where no unit can follow.
        if depth < 1 {
            return None;
        }
        rest.push(line.to_string());
    }
    None
}

/// An image line's pieces: the wrapper opener, the link opener, the image tag, whether a link closer follows, and the wrapper closer.
pub struct ImageLine<'t> {
    pub open: Option<&'t str>,
    pub link: Option<&'t str>,
    pub image: &'t str,
    pub link_close: bool,
    pub close: Option<&'t str>,
}

/// A line read as `wrapper? link? image link-close? wrapper-close?` with whitespace between, or None.
pub fn image_line(text: &str) -> Option<ImageLine<'_>> {
    let mut i = 0;
    let open = open_tag_at(text, i, &["p", "div"]).map(|(end, _)| {
        let tag = &text[i..end];
        i = end;
        tag
    });
    i = skip_space(text, i);
    let link = open_tag_at(text, i, &["a"]).map(|(end, _)| {
        let tag = &text[i..end];
        i = end;
        tag
    });
    i = skip_space(text, i);
    let (end, _) = open_tag_at(text, i, &["img"])?;
    let image = &text[i..end];
    i = skip_space(text, end);
    let link_close = match close_tag_at(text, i, &["a"]) {
        Some((end, _)) => {
            i = skip_space(text, end);
            true
        }
        None => false,
    };
    let close = close_tag_at(text, i, &["p", "div"]).map(|(end, _)| {
        let tag = &text[i..end];
        i = end;
        tag
    });
    (i == text.len()).then_some(ImageLine {
        open,
        link,
        image,
        link_close,
        close,
    })
}

/// A wrapper's image unit at lines[at]: the block, the last line it takes and what its wrapper held under the image.
pub fn image_unit<S: AsRef<str>>(
    lines: &[S],
    at_line: usize,
    dir: &str,
) -> Option<(Block, usize, Vec<String>)> {
    let first_text = trim(lines[at_line].as_ref());
    let first = image_line(first_text);
    let (open, parts, mut last) = match first {
        Some(parts) if parts.open.is_some() => (parts.open.unwrap_or(""), parts, at_line),
        None if is_wrapper_open(first_text) && at_line + 1 < lines.len() => {
            let parts = image_line(trim(lines[at_line + 1].as_ref()))?;
            if parts.open.is_some() {
                return None;
            }
            (first_text, parts, at_line + 1)
        }
        _ => return None,
    };
    if parts.link.is_some() != parts.link_close {
        return None;
    }
    let name = tag_head(open).name;
    let mut rest = Vec::new();
    match parts.close {
        Some(close) => {
            if wrapper_close(close) != Some(name.as_str()) {
                return None;
            }
        }
        None => {
            let (tail, end) = wrapper_tail(lines, last + 1, &name)?;
            rest = tail;
            last = end;
        }
    }
    let mut block = raw_image(parts.image, dir)?;
    let centred = is_centred(open);
    match &mut block {
        Block::Image(image) => {
            if centred {
                image.align = Some(Align::Center);
            }
            if let Some(link) = parts.link.and_then(link_of) {
                image.link = Some(link);
            }
        }
        Block::Remote { align, .. } if centred => *align = Some(Align::Center),
        _ => {}
    }
    if let Some(first) = rest.first_mut() {
        *first = first[leading_breaks(first)..].to_string();
    }
    let shown: Vec<String> = rest.into_iter().filter(|line| !line.is_empty()).collect();
    let wrapper = if shown.is_empty() {
        Vec::new()
    } else {
        vec![format!("{open}{}</{name}>", shown.join("\n"))]
    };
    Some((block, last, wrapper))
}
