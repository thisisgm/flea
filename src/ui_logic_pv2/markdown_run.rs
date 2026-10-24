//! The inline scan: held code and math first, then links, images and HTML in one forward pass, emphasis paired at the end.

use super::markdown_break::{is_rule_line, line_break, quote_mark_at, Delims};
use super::markdown_emph::{process, run_at};
use super::markdown_entity::reference_at;
use super::markdown_html::{document_text, Dead};
use super::markdown_inline::{
    escape_decoded_text, escape_html_text, is_punct, span_intervals, SpanKind,
};
use super::markdown_piece::{LineState, Piece};
use super::markdown_refs::read_footnote_ref;
use super::markdown_resolve::{bare_at, parse_angle};
use super::markdown_text::{at, find_byte, is_colour};
use super::Ctx;
use std::cell::Cell;
use std::collections::HashMap;

#[path = "markdown_run_steps.rs"]
mod steps;
use steps::{close_bracket, hold, plain_fast};

thread_local! {
    /// Bracket frames visited on this thread, which the work gate reads to prove the scan linear.
    pub static FRAME_STEPS: Cell<usize> = const { Cell::new(0) };
}

fn count_frame_step() {
    FRAME_STEPS.with(|steps| steps.set(steps.get() + 1));
}

/// What an inline scan reads and writes beside its text: the definitions, the footnote numbers and the shared tokens.
pub struct Env<'e> {
    pub ctx: &'e Ctx<'e>,
    pub defs: &'e HashMap<String, String>,
    pub numbers: &'e mut HashMap<String, usize>,
    pub tokens: &'e mut Vec<String>,
    /// The notes cited so far in document order; None when this text numbers no citation.
    pub cited: Option<&'e mut Vec<String>>,
}

/// How a text sits: a table cell escapes everything literally, bare text is held by no container, a fragment starts mid-line.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shape {
    pub literal_plain: bool,
    pub bare_text: bool,
    pub fragment: bool,
}

struct Frame {
    bang: bool,
    mark: usize,
    raw_start: usize,
    active: bool,
    passthrough: bool,
}

struct LastSpan {
    start: Option<usize>,
    length: usize,
    open: usize,
    kind: SpanKind,
    token: usize,
}

struct Scan<'s, 'e> {
    env: &'s mut Env<'e>,
    out: Vec<Piece>,
    delims: Delims,
    citations: HashMap<usize, String>,
    frames: Vec<Frame>,
    open: Vec<usize>,
    active_links: Vec<usize>,
    numbered: usize,
    image_mark: Option<usize>,
    image_depth: Option<usize>,
}

/// One text's inline markup for the renderer.
pub fn parse_inline(text: &str, env: &mut Env, shape: Shape) -> String {
    let body = document_text(text);
    if plain_fast(&body, shape.literal_plain) {
        return if shape.literal_plain {
            escape_html_text(&body)
        } else {
            body
        };
    }
    let ctx = *env.ctx;
    let spans = span_intervals(&body);
    let style_links = is_colour(ctx.ink);
    let chrome_ok = is_colour(ctx.chrome);
    let mut style_cache = HashMap::new();
    let mut last = LastSpan {
        start: None,
        length: 0,
        open: 0,
        kind: SpanKind::Code,
        token: 0,
    };
    let mut dead = Dead::default();
    let mut inked = 0usize;
    let mut i = 0;
    let mut sp = 0;
    let mut last_quote: Option<usize> = None;
    let mut line = LineState::new(!shape.fragment);
    let mut scan = Scan {
        env,
        out: Vec::new(),
        delims: Delims {
            list: Vec::new(),
            bottom: 0,
        },
        citations: HashMap::new(),
        frames: Vec::new(),
        open: Vec::new(),
        active_links: Vec::new(),
        numbered: 0,
        image_mark: None,
        image_depth: None,
    };
    while i < body.len() {
        if sp < spans.len() && i == spans[sp].from {
            let span = spans[sp];
            sp += 1;
            if !chrome_ok && span.kind == SpanKind::Math {
                continue;
            }
            hold(
                &body,
                span,
                chrome_ok.then_some(ctx.chrome),
                &mut style_cache,
                &mut last,
                &mut scan,
            );
            i = span.to;
            continue;
        }
        if sp < spans.len() && i > spans[sp].from {
            sp += 1;
            continue;
        }
        let c = at(&body, i);
        if c == b'\\' && i + 1 < body.len() && is_punct(at(&body, i + 1)) {
            scan.out
                .push(Piece::Text(format!("&#{};", at(&body, i + 1))));
            i += 2;
            continue;
        }
        if c == b'\n' || (c == b'\\' && at(&body, i + 1) == b'\n') {
            let mut ended = false;
            i = line_break(&body, i, &mut scan.out, &mut scan.delims, &mut || {
                ended = true
            });
            if ended {
                scan.end_paragraph();
            }
            continue;
        }
        if c == b'`' || c == b'$' {
            scan.out.push(Piece::Ch(c as char));
            i += 1;
            continue;
        }
        if (c == b'*' || c == b'_') && is_rule_line(&body, i) {
            let end = find_byte(&body, b'\n', i).unwrap_or(body.len());
            scan.out
                .push(Piece::Text((c as char).to_string().repeat(3)));
            i = end;
            continue;
        }
        if c == b'*' || c == b'_' {
            let mark = run_at(&body, i, scan.out.len());
            i = mark.end;
            scan.out.push(Piece::Delim(scan.delims.list.len()));
            scan.delims.list.push(mark);
            continue;
        }
        if c == b'!' && at(&body, i + 1) == b'[' {
            scan.out.push(Piece::Ch('!'));
            if scan.image_mark.is_none() {
                scan.image_mark = Some(scan.out.len());
                scan.image_depth = Some(scan.open.len());
            }
            scan.open.push(scan.frames.len());
            scan.frames.push(Frame {
                bang: true,
                mark: scan.out.len(),
                raw_start: i + 2,
                active: true,
                passthrough: false,
            });
            scan.out.push(Piece::Lit("&#91;"));
            i += 2;
            continue;
        }
        if c == b'[' {
            if at(&body, i + 1) == b'^' {
                let cite = read_footnote_ref(&body, i).filter(|(id, _)| {
                    scan.env
                        .numbers
                        .get(id)
                        .is_some_and(|n| scan.env.cited.is_some() || *n > 0)
                });
                if let Some((id, end)) = cite {
                    let number = scan.env.numbers[&id];
                    scan.citations.insert(scan.env.tokens.len(), id);
                    scan.env.tokens.push(format!("<sup>{number}</sup>"));
                    scan.out.push(Piece::Token(scan.env.tokens.len() - 1));
                    i = end;
                    continue;
                }
                scan.out.push(Piece::Lit("&#91;"));
                i += 1;
                continue;
            }
            let index = scan.frames.len();
            scan.frames.push(Frame {
                bang: false,
                mark: scan.out.len(),
                raw_start: i + 1,
                active: style_links,
                passthrough: !style_links,
            });
            scan.open.push(index);
            if style_links {
                scan.active_links.push(index);
            }
            scan.out.push(Piece::Lit("&#91;"));
            i += 1;
            continue;
        }
        if c == b']' && !scan.open.is_empty() {
            i = close_bracket(&body, i, &ctx, &mut scan, &mut line);
            continue;
        }
        if c == b']' {
            scan.out.push(Piece::Lit("&#93;"));
            i += 1;
            continue;
        }
        if c == b'&' {
            let blank = line.blank(&scan.out);
            if let Some(run) =
                super::markdown_piece::space_run_at(&body, i, &scan.out, blank, !shape.fragment)
            {
                let keep = scan.out.len() - run.trim;
                scan.out.truncate(keep);
                scan.out.push(Piece::Lit(run.text));
                i = run.end;
                continue;
            }
            match reference_at(&body, i) {
                Some((decoded, end)) => {
                    scan.out.push(Piece::Text(escape_decoded_text(&decoded)));
                    i = end;
                }
                None => {
                    scan.out.push(Piece::Ch('&'));
                    i += 1;
                }
            }
            continue;
        }
        if c == b'<' {
            let Scan { env, out, .. } = &mut scan;
            i = parse_angle(
                &body,
                i,
                &ctx,
                style_links,
                &mut dead,
                env.tokens,
                out,
                &mut inked,
            );
            continue;
        }
        if c == b'h' || c == b'w' {
            let Scan { env, out, .. } = &mut scan;
            match bare_at(&body, i, &ctx, style_links, env.tokens, out) {
                Some(end) => i = end,
                None => {
                    out.push(Piece::Ch(c as char));
                    i += 1;
                }
            }
            continue;
        }
        if c == b'>' {
            // A mark that opens a quote inside a list item stays for the renderer; table cells never hold one.
            let quoting =
                !shape.literal_plain && !shape.bare_text && quote_mark_at(&body, i, last_quote);
            if quoting {
                last_quote = Some(i);
            }
            scan.out.push(if quoting {
                Piece::Ch('>')
            } else {
                Piece::Lit("&#62;")
            });
            i += 1;
            continue;
        }
        // A table cell's only pipe is an unescaped "\|", so it stays cell text like the bulk escaper writes it.
        if c == b'|' && shape.literal_plain {
            scan.out.push(Piece::Lit("&#124;"));
            i += 1;
            continue;
        }
        let ch = super::markdown_text::char_at(&body, i).unwrap_or('\u{fffd}');
        scan.out.push(Piece::Ch(ch));
        i += ch.len_utf8();
    }
    let bottom = scan.delims.bottom;
    process(&mut scan.delims.list, bottom);
    scan.render_from(0, false, false)
}
