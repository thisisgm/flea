//! The Markdown a run still holds between its tags: escapes, references, struck text, pictures, code spans and line ends.

use super::markdown_entity::reference_at;
use super::markdown_escape::is_ascii_punct;
use super::markdown_rich::{InlineImage, Piece, Style};
use std::collections::HashMap;

/// What a stretch of text adds: a piece, the end of a paragraph, or a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Piece(Piece),
    Para,
    Rule,
}

/// A line that is a thematic break: three or more of one of "*", "-" or "_", spaces between, at most three spaces before.
pub fn is_rule_line(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let body = line.trim();
    let Some(mark) = body.chars().next().filter(|c| matches!(c, '*' | '-' | '_')) else {
        return false;
    };
    indent <= 3
        && body.chars().filter(|c| *c == mark).count() >= 3
        && body.chars().all(|c| c == mark || c == ' ' || c == '\t')
}

/// The struck stretches of a markup: each opening run of one or two tildes paired with the next closing run of its length.
#[derive(Debug, Default)]
pub struct Tildes {
    edges: HashMap<usize, (usize, bool)>,
    pub depth: u32,
}

impl Tildes {
    pub fn pair(markup: &str) -> Self {
        let bytes = markup.as_bytes();
        let mut open: Vec<(usize, usize)> = Vec::new();
        let mut edges = HashMap::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'<' => i = markup[i..].find('>').map_or(bytes.len(), |e| i + e + 1),
                b'\n' if bytes.get(i + 1) == Some(&b'\n') => {
                    open.clear();
                    i += 1;
                }
                b'~' => {
                    let run = bytes[i..].iter().take_while(|b| **b == b'~').count();
                    let before = markup[..i]
                        .chars()
                        .next_back()
                        .map_or(true, char::is_whitespace);
                    let after = markup[i + run..]
                        .chars()
                        .next()
                        .map_or(true, char::is_whitespace);
                    if run <= 2 {
                        if let Some(at) = open
                            .iter()
                            .rposition(|(_, len)| *len == run)
                            .filter(|_| !before)
                        {
                            edges.insert(open[at].0, (run, true));
                            edges.insert(i, (run, false));
                            open.truncate(at);
                        } else if !after {
                            open.push((i, run));
                        }
                    }
                    i += run;
                }
                _ => i += 1,
            }
        }
        Tildes { edges, depth: 0 }
    }
}

struct Out<'s> {
    events: Vec<Event>,
    text: String,
    style: &'s Style,
    strike: bool,
}

impl Out<'_> {
    fn flush(&mut self) {
        if !self.text.is_empty() {
            let mut style = self.style.clone();
            style.strike |= self.strike;
            self.events.push(Event::Piece(Piece::Text(
                std::mem::take(&mut self.text),
                style,
            )));
        }
    }

    fn push(&mut self, c: char) {
        if c == ' ' && (self.text.ends_with(' ') || (self.text.is_empty() && self.collapsed())) {
            return;
        }
        self.text.push(c);
    }

    // The last piece already ends in a space, so a space that follows it would draw two.
    fn collapsed(&self) -> bool {
        matches!(self.events.last(), Some(Event::Piece(Piece::Text(text, _))) if text.ends_with(' '))
    }
}

// "![alt](file:///p.png "title")" at i: the picture and the index after it.
fn image_at(text: &str, i: usize) -> Option<(InlineImage, usize)> {
    let rest = text.get(i..)?.strip_prefix("![")?;
    let close = rest.find(']')?;
    let tail = rest[close + 1..].strip_prefix('(')?;
    let end = tail.find(')')?;
    let inside = tail[..end].trim();
    let url = inside.split_whitespace().next().unwrap_or_default();
    let alt = super::markdown_entity::decode_references(&rest[..close]);
    let image = InlineImage {
        url: url.to_string(),
        alt,
        width: None,
        height: None,
        link: None,
    };
    Some((image, i + 2 + close + 2 + end + 1))
}

// "[![alt](file:///p.png)](https://x/)" at i: the picture with its link, and the index after it.
fn linked_image_at(text: &str, i: usize) -> Option<(InlineImage, usize)> {
    let (mut image, end) = image_at(text, i + 1)?;
    let tail = text.get(end..)?.strip_prefix("](")?;
    let close = tail.find(')')?;
    image.link = Some(tail[..close].trim().to_string());
    Some((image, end + 2 + close + 1))
}

// A code span at i: its text, spaces at both ends trimmed once, and the index after the closing run.
fn code_at(text: &str, i: usize) -> Option<(String, usize)> {
    let run = text[i..].bytes().take_while(|b| *b == b'`').count();
    let fence = &text[i..i + run];
    let mut from = i + run;
    while let Some(at) = text[from..].find(fence).map(|at| from + at) {
        let len = text[at..].bytes().take_while(|b| *b == b'`').count();
        if len == run {
            let inner = text[i + run..at].replace('\n', " ");
            let trimmed = if inner.len() > 2 && inner.starts_with(' ') && inner.ends_with(' ') {
                inner[1..inner.len() - 1].to_string()
            } else {
                inner
            };
            return Some((trimmed, at + run));
        }
        from = at + len;
    }
    None
}

/// The events a stretch of text at `base` in its markup adds; line_start carries across stretches, `last` ends the markup.
pub fn text_events(
    text: &str,
    base: usize,
    style: &Style,
    pre: bool,
    tildes: &mut Tildes,
    line_start: &mut bool,
    last: bool,
) -> Vec<Event> {
    let mut out = Out {
        events: Vec::new(),
        text: String::new(),
        style,
        strike: tildes.depth > 0,
    };
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if *line_start && !pre {
            let line_end = text[i..].find('\n').map(|e| i + e);
            let line = &text[i..line_end.unwrap_or(text.len())];
            if (line_end.is_some() || last) && is_rule_line(line) {
                out.flush();
                out.events.push(Event::Rule);
                i = line_end.map_or(text.len(), |e| e + 1);
                continue;
            }
            if bytes[i] == b' ' || bytes[i] == b'\t' {
                i += 1;
                continue;
            }
            *line_start = false;
        }
        let c = text[i..].chars().next().unwrap_or(' ');
        match c {
            '\n' if pre => {
                out.flush();
                out.events.push(Event::Piece(Piece::Break));
                i += 1;
            }
            '\n' => {
                let next = text[i + 1..].find('\n').map(|e| i + 1 + e);
                let blank = next.is_some_and(|n| text[i + 1..n].trim().is_empty());
                if blank {
                    out.flush();
                    out.events.push(Event::Para);
                    i = next.unwrap_or(i + 1);
                } else {
                    out.push(' ');
                    i += 1;
                }
                *line_start = true;
            }
            ' ' | '\t' if !pre => {
                out.push(' ');
                i += 1;
            }
            '\\' if bytes
                .get(i + 1)
                .is_some_and(|b| is_ascii_punct(u32::from(*b))) =>
            {
                out.push(bytes[i + 1] as char);
                i += 2;
            }
            '&' => match reference_at(text, i) {
                Some((value, end)) => {
                    out.text.push_str(&value);
                    i = end;
                }
                None => {
                    out.push('&');
                    i += 1;
                }
            },
            '~' if tildes.edges.contains_key(&(base + i)) => {
                out.flush();
                let (run, opens) = tildes.edges[&(base + i)];
                tildes.depth = if opens {
                    tildes.depth + 1
                } else {
                    tildes.depth.saturating_sub(1)
                };
                out.strike = tildes.depth > 0;
                i += run;
            }
            '!' | '[' if image_at(text, i + usize::from(c == '[')).is_some() => {
                let found = if c == '[' {
                    linked_image_at(text, i)
                } else {
                    image_at(text, i)
                };
                match found {
                    Some((image, end)) => {
                        out.flush();
                        out.events.push(Event::Piece(Piece::Image(image)));
                        i = end;
                    }
                    None => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            '`' if !style.code => match code_at(text, i) {
                Some((code, end)) => {
                    out.flush();
                    let mut chip = style.clone();
                    chip.code = true;
                    out.events.push(Event::Piece(Piece::Text(code, chip)));
                    i = end;
                }
                None => {
                    let run = text[i..].bytes().take_while(|b| *b == b'`').count();
                    out.text.push_str(&text[i..i + run]);
                    i += run;
                }
            },
            _ => {
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out.flush();
    out.events
}
