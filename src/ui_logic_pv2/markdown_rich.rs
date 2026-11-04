//! The parser's markup as drawn paragraphs: the sanitized tags and the Markdown left in a run, read into styled pieces.

use super::markdown_html::{tag_head, TagHead};
use super::markdown_lists::bullet_at;
use super::markdown_model::Align;
use super::markdown_rich_text::{text_events, Event, Tildes};

#[path = "markdown_rich_types.rs"]
mod types;

pub use types::{InlineImage, Para, Piece, Style};

struct Builder {
    paras: Vec<Para>,
    cur: Para,
    styles: Vec<(String, Style)>,
    aligns: Vec<(String, Option<Align>)>,
    lists: Vec<(bool, u64)>,
    quotes: u8,
    pre: u8,
    heading: u8,
    row_cells: usize,
    formulas: usize,
    tildes: Tildes,
    line_start: bool,
}

impl Builder {
    fn style(&self) -> Style {
        self.styles
            .last()
            .map(|(_, style)| style.clone())
            .unwrap_or_default()
    }

    fn align(&self) -> Option<Align> {
        self.aligns.iter().rev().find_map(|(_, align)| *align)
    }

    // Ends the paragraph in progress; one with nothing to draw is dropped.
    fn flush(&mut self) {
        let mut para = std::mem::take(&mut self.cur);
        trim_edges(&mut para.pieces);
        if para.rule || !para.pieces.is_empty() {
            self.paras.push(para);
        }
        self.cur = Para {
            align: self.align(),
            heading: self.heading,
            pre: self.pre > 0,
            indent: self.lists.len() as u8 + self.quotes,
            ..Para::default()
        };
    }

    fn rule(&mut self) {
        self.flush();
        self.paras.push(Para {
            rule: true,
            ..Para::default()
        });
    }

    fn push_style(&mut self, name: &str, change: impl FnOnce(&mut Style)) {
        let mut style = self.style();
        change(&mut style);
        self.styles.push((name.to_string(), style));
    }

    fn pop_style(&mut self, name: &str) {
        if let Some(at) = self.styles.iter().rposition(|(open, _)| open == name) {
            self.styles.truncate(at);
        }
    }

    fn text(&mut self, text: &str, base: usize, last: bool) {
        let style = self.style();
        if style.pad || text.is_empty() {
            return;
        }
        let pre = self.pre > 0;
        for event in text_events(
            text,
            base,
            &style,
            pre,
            &mut self.tildes,
            &mut self.line_start,
            last,
        ) {
            match event {
                Event::Para => self.flush(),
                Event::Rule => self.rule(),
                Event::Piece(Piece::Text(text, style))
                    if style.face && matches!(text.as_str(), "\u{2610}" | "\u{2611}") =>
                {
                    self.cur.pieces.push(Piece::TaskBox(text == "\u{2611}"))
                }
                Event::Piece(Piece::Text(mut text, style)) => {
                    // A space after a piece that already ends in one, or at a paragraph's start, draws nothing.
                    let spaced = match self.cur.pieces.last() {
                        Some(Piece::Text(before, _)) => before.ends_with(' '),
                        None | Some(Piece::Break) => true,
                        _ => false,
                    };
                    if spaced && !pre && text.starts_with(' ') {
                        text.remove(0);
                    }
                    if !text.is_empty() {
                        self.cur.pieces.push(Piece::Text(text, style));
                    }
                }
                Event::Piece(piece) => self.cur.pieces.push(piece),
            }
        }
    }

    fn open(&mut self, head: &TagHead, raw: &str) {
        let name = head.name.as_str();
        let attr = |key: &str| {
            head.attributes
                .iter()
                .find(|a| a.name == key)
                .and_then(|a| a.value.clone())
        };
        let align = || match attr("align").map(|a| a.to_ascii_lowercase()).as_deref() {
            Some("center") => Some(Align::Center),
            Some("right") => Some(Align::Right),
            _ => None,
        };
        match name {
            "b" | "strong" => self.push_style(name, |s| s.bold = true),
            "i" | "em" => self.push_style(name, |s| s.italic = true),
            "u" => self.push_style(name, |s| s.underline = true),
            "s" | "del" | "strike" => self.push_style(name, |s| s.strike = true),
            "sub" => self.push_style(name, |s| s.sub = true),
            "sup" => self.push_style(name, |s| s.sup = true),
            "small" => self.push_style(name, |s| s.small = true),
            "mark" => self.push_style(name, |s| s.mark = true),
            "kbd" => self.push_style(name, |s| s.kbd = true),
            "code" => {
                let chip = raw
                    .find("background-color:")
                    .map(|at| raw[at + 17..].chars().take_while(|c| *c != '"').collect());
                let formula = raw.contains("data-math=\"inline\"");
                self.push_style(name, |s| {
                    s.code = true;
                    s.chip = chip;
                });
                if formula {
                    if let Some((open, _)) = self.styles.last_mut() {
                        *open = "code-math".to_string();
                    }
                }
            }
            "span" => {
                let pad = raw.contains("font-size:chippad");
                self.push_style(name, |s| s.pad = pad);
            }
            "font" => {
                let (colour, face) = (attr("color"), attr("face").is_some());
                self.push_style(name, |s| {
                    s.colour = colour.or(s.colour.take());
                    s.face = face;
                });
            }
            "a" => {
                let href = attr("href");
                self.push_style(name, |s| {
                    s.link = href.or(s.link.take());
                    s.underline = s.link.is_some() || s.underline;
                });
            }
            "br" => self.cur.pieces.push(Piece::Break),
            "hr" => self.rule(),
            "img" => {
                let url = attr("src").unwrap_or_default();
                let size = |key: &str| {
                    attr(key).and_then(|v| v.trim().trim_end_matches("px").parse().ok())
                };
                let image = InlineImage {
                    alt: attr("alt").unwrap_or_default(),
                    width: size("width"),
                    height: size("height"),
                    link: self.style().link,
                    url,
                };
                self.cur.pieces.push(Piece::Image(image));
            }
            "p" | "div" | "details" | "summary" | "center" => {
                self.flush();
                self.aligns.push((name.to_string(), align()));
                self.cur.align = self.align();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush();
                self.heading = name.as_bytes()[1] - b'0';
                self.aligns.push((name.to_string(), align()));
                self.cur.align = self.align();
                self.cur.heading = self.heading;
            }
            "pre" => {
                self.flush();
                self.pre += 1;
                self.cur.pre = true;
            }
            "blockquote" => {
                self.flush();
                self.quotes += 1;
                self.cur.indent += 1;
            }
            "ul" | "ol" => {
                self.flush();
                let start = attr("start").and_then(|s| s.parse().ok()).unwrap_or(1);
                self.lists.push((name == "ol", start));
                self.cur.indent = self.lists.len() as u8 + self.quotes;
            }
            "li" => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                self.cur.marker = match self.lists.last_mut() {
                    Some((true, next)) => {
                        *next += 1;
                        format!("{}.", *next - 1)
                    }
                    _ => bullet_at(depth).to_string(),
                };
            }
            "tr" => {
                self.flush();
                self.row_cells = 0;
            }
            "td" | "th" => {
                if self.row_cells > 0 {
                    self.cur.pieces.push(Piece::CellGap);
                }
                self.row_cells += 1;
                self.push_style(name, |s| s.bold = s.bold || name == "th");
            }
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "p" | "div" | "details" | "summary" | "center" | "h1" | "h2" | "h3" | "h4" | "h5"
            | "h6" => {
                if name.starts_with('h') {
                    self.heading = 0;
                }
                if let Some(at) = self.aligns.iter().rposition(|(open, _)| open == name) {
                    self.aligns.truncate(at);
                }
                self.flush();
            }
            "pre" => {
                self.flush();
                self.pre = self.pre.saturating_sub(1);
                self.cur.pre = self.pre > 0;
            }
            "blockquote" => {
                self.flush();
                self.quotes = self.quotes.saturating_sub(1);
                self.cur.indent = self.lists.len() as u8 + self.quotes;
            }
            "ul" | "ol" => {
                self.flush();
                self.lists.pop();
                self.cur.indent = self.lists.len() as u8 + self.quotes;
            }
            "li" | "tr" | "table" => self.flush(),
            "code" => {
                if self
                    .styles
                    .last()
                    .is_some_and(|(open, _)| open == "code-math")
                {
                    self.formula();
                } else {
                    self.pop_style("code");
                }
            }
            _ => self.pop_style(name),
        }
    }

    // An inline formula's chip becomes one piece with its index, so the host can draw the formula's picture in its place.
    fn formula(&mut self) {
        let style = self.style();
        self.styles.pop();
        let mut text = String::new();
        while let Some(Piece::Text(piece, piece_style)) = self.cur.pieces.last() {
            if *piece_style != style {
                break;
            }
            text.insert_str(0, piece);
            self.cur.pieces.pop();
        }
        self.cur.pieces.push(Piece::Formula {
            index: self.formulas,
            text,
            style,
        });
        self.formulas += 1;
    }
}

// Spaces that open or close a paragraph draw nothing.
fn trim_edges(pieces: &mut Vec<Piece>) {
    while let Some(Piece::Text(text, _)) = pieces.first_mut() {
        let trimmed = text.trim_start_matches([' ', '\n']).to_string();
        if !trimmed.is_empty() {
            *text = trimmed;
            break;
        }
        pieces.remove(0);
    }
    while let Some(Piece::Text(text, _)) = pieces.last_mut() {
        let trimmed = text.trim_end_matches([' ', '\n']).to_string();
        if !trimmed.is_empty() {
            *text = trimmed;
            break;
        }
        pieces.pop();
    }
    while matches!(pieces.last(), Some(Piece::Break)) {
        pieces.pop();
    }
}

/// The paragraphs a block's markup draws: 'A <b>bold</b> claim' is one paragraph of three pieces; "a\n***" is a paragraph and a rule.
pub fn rich(markup: &str) -> Vec<Para> {
    let mut builder = Builder {
        paras: Vec::new(),
        cur: Para::default(),
        styles: Vec::new(),
        aligns: Vec::new(),
        lists: Vec::new(),
        quotes: 0,
        pre: 0,
        heading: 0,
        row_cells: 0,
        formulas: 0,
        tildes: Tildes::pair(markup),
        line_start: true,
    };
    let mut i = 0;
    let mut plain = 0;
    let bytes = markup.as_bytes();
    while i < bytes.len() {
        let tag_start = bytes[i] == b'<'
            && bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'/');
        let end = if tag_start {
            markup[i..].find('>').map(|e| i + e + 1)
        } else {
            None
        };
        let Some(end) = end else {
            i += 1;
            continue;
        };
        builder.text(&markup[plain..i], plain, false);
        let raw = &markup[i..end];
        let head = tag_head(raw);
        if !head.name.is_empty() {
            if head.closing {
                builder.close(&head.name);
            } else {
                builder.open(&head, raw);
            }
        }
        i = end;
        plain = end;
    }
    builder.text(&markup[plain..], plain, true);
    builder.flush();
    builder.paras
}
