//! A line's containers: the open quotes and items it continues, lazily or not, and the ones it opens.

use super::super::markdown_container::{
    indentation_at, list_at, quote_at, take_indent, take_list, take_quote, text_at, View,
};
use super::super::markdown_event::{Frame, Lead};
use super::super::markdown_leaf::{fence_open, is_setext, is_thematic};
use super::super::markdown_text::blank;
use super::super::NESTING_LIMIT;
use super::{
    escape_setext, heading_line, rule_suffix, Here, Leaf, Pass, LIST_INTERRUPT_START,
    MAX_RULE_INDENT, MIN_RULE_MARKS,
};

impl<'p> Pass<'p, '_, '_, '_> {
    /// Matches the line against the open containers and opens those it starts; None once they nest past the limit.
    pub(super) fn containers(&mut self, i: usize) -> Option<Here<'p>> {
        let raw = self.lines[i];
        let Self {
            frames,
            lead,
            last_quote,
            leaf,
            serial,
            state,
            ..
        } = self;
        let mut view = View::default();
        let raw_blank = blank(raw);
        let mut matched = 0;
        let mut lead_here = 0;
        *lead = Lead {
            n: lead.n,
            ..Lead::default()
        };
        let (rule_start, rule_count) = rule_suffix(raw);
        let previous_top = frames.first().map(|top| (top.quote, top.mark, top.group));
        while matched < frames.len() {
            let frame = &mut frames[matched];
            if frame.quote {
                match quote_at(raw, &view) {
                    Some(quote) => take_quote(raw, &mut view, quote),
                    None => break,
                }
            } else {
                let indent = indentation_at(raw, &view, frame.content_col).1;
                if indent >= frame.content_col && !frame.sealed {
                    take_indent(raw, &mut view, frame.content_col);
                } else if raw_blank || blank(&raw[view.at.min(raw.len())..]) {
                    // An item opened on a blank line holds at most that one blank line, so a second seals it against more content.
                    if i > 0 && frame.bare_at == Some(i - 1) {
                        frame.sealed = true;
                    }
                    if last_quote.map_or(true, |quote| matched > quote) {
                        matched = frames.len();
                        break;
                    }
                } else {
                    break;
                }
            }
            if frames[matched].quote == frames[0].quote && lead_here == matched {
                lead_here += 1;
                lead.here = lead_here;
                lead.at = view.at;
                lead.pad = view.padding;
                lead.col = view.column;
            }
            matched += 1;
        }
        let unmatched_text = text_at(raw, &view);
        let starts_quote = quote_at(raw, &view).is_some();
        let starts_list = list_at(raw, &view);
        let sibling_list =
            matched < frames.len() && !frames[matched].quote && starts_list.is_some();
        let list_interrupts = starts_list.is_some_and(|marker| {
            sibling_list
                || ((!marker.ordered || marker.start == LIST_INTERRUPT_START)
                    && !blank(&raw[marker.marker_end..]))
        });
        let starts_fence = fence_open(&unmatched_text);
        let thematic = is_thematic(&unmatched_text);
        let paragraph = matches!(*leaf, Some(Leaf::Paragraph { .. }));
        let setext = paragraph && matched == frames.len() && is_setext(&unmatched_text);
        let lazy = paragraph
            && !blank(&unmatched_text)
            && !starts_quote
            && !list_interrupts
            && starts_fence.is_none()
            && !thematic
            && !heading_line(&unmatched_text);
        if matched < frames.len() && !lazy {
            frames.truncate(matched);
            lead.n = lead.n.min(matched);
            *last_quote = frames.iter().rposition(|frame| frame.quote);
            *leaf = None;
        }
        let mut owner = frames.last().map_or(0, |frame| frame.id);
        let fence = match *leaf {
            Some(Leaf::Fence {
                owner: held,
                tick,
                len,
                indent,
            }) if held == owner => Some((tick, len, indent)),
            _ => None,
        };
        lead.retained = frames.len();
        lead.lazy = lazy && matched < frames.len();
        if fence.is_none() && !lazy && !setext {
            loop {
                let quote_indent = quote_at(raw, &view);
                let marker = if quote_indent.is_none() {
                    list_at(raw, &view)
                } else {
                    None
                };
                let rule_here = rule_count >= MIN_RULE_MARKS
                    && view.at >= rule_start
                    && marker.is_some_and(|marker| marker.indent <= MAX_RULE_INDENT);
                if quote_indent.is_none() && (marker.is_none() || thematic || rule_here) {
                    break;
                }
                *serial += 1;
                let mut next = Frame {
                    id: *serial,
                    quote: quote_indent.is_some(),
                    content_col: 0,
                    ordered: false,
                    mark: 0,
                    start: 0,
                    bare_at: None,
                    group: *serial,
                    sealed: false,
                };
                if let Some(quote) = quote_indent {
                    take_quote(raw, &mut view, quote);
                    *last_quote = Some(frames.len());
                } else if let Some(marker) = marker {
                    next.content_col = marker.content_col;
                    next.ordered = marker.ordered;
                    next.mark = marker.mark;
                    next.start = marker.start;
                    next.bare_at = blank(&raw[marker.marker_end..]).then_some(i);
                    if let (true, Some((false, mark, group))) = (frames.is_empty(), previous_top) {
                        if mark == marker.mark {
                            next.group = group;
                        }
                    }
                    take_list(raw, &mut view, &marker);
                }
                if lead.n == frames.len()
                    && frames
                        .first()
                        .map_or(true, |first| next.quote == first.quote)
                {
                    lead.n += 1;
                    lead.here = lead.n;
                    lead.at = view.at;
                    lead.pad = view.padding;
                    lead.col = view.column;
                }
                frames.push(next);
                if frames.len() > NESTING_LIMIT {
                    state.deep = true;
                    return None;
                }
                owner = *serial;
                *leaf = None;
            }
        }
        let mut text = text_at(raw, &view);
        // "> foo\nbar\n===" keeps its lazy "===" as text, so the renderer cannot take it for a setext underline.
        if lazy && matched < frames.len() && is_setext(&unmatched_text) {
            text = escape_setext(&text);
        }
        Some(Here {
            i,
            raw,
            view,
            text,
            owner,
            top: !frames.is_empty(),
            setext,
            fence,
        })
    }
}
