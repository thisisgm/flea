//! A line's leaf: the fence, definition or code block it continues, or the heading, table, figure or paragraph line it is.

use super::super::markdown_container::{
    indentation_at, starts_block, take_indent, text_at, unindent, CODE_INDENT,
};
use super::super::markdown_event::{Event, LineKind};
use super::super::markdown_leaf::{atx_heading, delim_aligns, fence_open, is_thematic, split_row};
use super::super::markdown_math::{display_at, front_close_at};
use super::super::markdown_refs::{
    definition_at, hide_definition, hide_title, kill_definition, read_definition,
    read_definition_parts, read_footnote_definition, store_definition, Hidden,
};
use super::super::markdown_text::{blank, is_space, trim};
use super::{finish_note, heading_line, prefixes, Here, Leaf, Pass, Pending};

impl Pass<'_, '_, '_, '_> {
    /// Sends the line as the leaf it belongs to and answers the index of the next line to read.
    pub(super) fn leaf_line(&mut self, mut here: Here) -> usize {
        match self.held(&mut here) {
            Some(next) => next,
            None => self.opened(here),
        }
    }

    // A line an open fence or a definition being read takes, a hidden line, front matter, a fence opener or display math; None for any other.
    fn held(&mut self, here: &mut Here) -> Option<usize> {
        let Self {
            lines,
            refs,
            collect,
            state,
            nested,
            send,
            frames,
            leaf,
            pending,
            lead,
            ..
        } = self;
        let (lines, collect, nested) = (*lines, *collect, *nested);
        let (i, raw, owner) = (here.i, here.raw, here.owner);
        if let Some((tick, len, indent)) = here.fence {
            let text = std::mem::take(&mut here.text);
            let closed = fence_open(&text).is_some_and(|closer| {
                closer.info.is_empty() && closer.tick == tick && closer.len >= len
            });
            state.code.insert(i);
            let shown = if closed {
                text
            } else {
                unindent(&text, indent).to_string()
            };
            send.line(
                if closed {
                    LineKind::FenceClose
                } else {
                    LineKind::FenceBody
                },
                i,
                shown,
                "",
                frames,
                lead,
                raw,
                state,
            );
            if closed {
                *leaf = None;
            }
            return Some(i + 1);
        }
        if let Some(held) = pending
            .as_mut()
            .filter(|held| collect && held.owner() == owner)
        {
            match held {
                Pending::Note { body, .. }
                    if indentation_at(raw, &here.view, CODE_INDENT).1 >= CODE_INDENT =>
                {
                    let text = std::mem::take(&mut here.text);
                    body.push(text.trim_start_matches(is_space).to_string());
                    hide_definition(state, i, i);
                    send.line(LineKind::Hidden, i, text, "", frames, lead, raw, state);
                    return Some(i + 1);
                }
                Pending::Ref { key, index, .. } if fence_open(&here.text).is_none() => {
                    // '[cover].png "Title"' accepts a destination only when the complete line parses.
                    let (target, titled) = read_definition_parts(trim(&here.text));
                    if !target.is_empty() {
                        let (key, index) = (key.clone(), *index);
                        if let Some(refs) = refs.as_deref_mut() {
                            store_definition(&mut refs.defs, key, target);
                        }
                        hide_definition(state, index, i);
                        hide_title(lines, i + 1, state, !titled, &prefixes(frames));
                        // Replay the opener's paragraph state so its lazy destination keeps the same containers.
                        state.hidden.insert(index, Hidden::Paragraph);
                        *pending = None;
                        *leaf = None;
                        let text = std::mem::take(&mut here.text);
                        send.line(LineKind::Hidden, i, text, "", frames, lead, raw, state);
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
        }
        if pending.is_some() {
            finish_note(pending, refs);
        }
        if let Some(hidden) = state.hidden.get(&i).copied() {
            *leaf = (hidden == Hidden::Paragraph).then_some(Leaf::Paragraph { owner });
            let text = std::mem::take(&mut here.text);
            send.line(LineKind::Hidden, i, text, "", frames, lead, raw, state);
            return Some(i + 1);
        }
        if let Some(close) = (i == 0 && !nested).then(|| front_close_at(lines)).flatten() {
            send.line(
                LineKind::FenceOpen,
                0,
                String::new(),
                "",
                frames,
                lead,
                lines[0],
                state,
            );
            state.code.insert(0);
            for (k, body) in lines.iter().enumerate().take(close).skip(1) {
                state.code.insert(k);
                send.line(
                    LineKind::FenceBody,
                    k,
                    body.to_string(),
                    "",
                    frames,
                    lead,
                    body,
                    state,
                );
            }
            state.code.insert(close);
            send.line(
                LineKind::FenceClose,
                close,
                String::new(),
                "",
                frames,
                lead,
                lines[close],
                state,
            );
            *leaf = None;
            return Some(close + 1);
        }
        if let Some(open) = fence_open(&here.text) {
            *leaf = Some(Leaf::Fence {
                owner,
                tick: open.tick,
                len: open.len,
                indent: open.indent,
            });
            state.code.insert(i);
            let text = std::mem::take(&mut here.text);
            send.line(
                LineKind::FenceOpen,
                i,
                text,
                &open.info,
                frames,
                lead,
                raw,
                state,
            );
            return Some(i + 1);
        }
        // "$$x^2$$" and "$$\nx^2\n$$" emit figures, with trailing prose retained.
        let math = (!here.top)
            .then(|| display_at(lines, i, &here.text))
            .flatten()?;
        for k in i..=math.to {
            state.code.insert(k);
        }
        send.whole(Event::Math {
            source: math.source,
        });
        let i = math.to;
        let tail_blank = blank(&math.tail);
        if !math.tail.is_empty() {
            send.line(
                LineKind::Run,
                i,
                math.tail.clone(),
                "",
                frames,
                lead,
                &math.tail,
                state,
            );
        }
        *leaf = (!tail_blank).then_some(Leaf::Paragraph { owner });
        Some(i + 1)
    }

    // A line no open leaf takes: indented code, a definition, a heading, a table, or a paragraph line.
    fn opened(&mut self, here: Here) -> usize {
        let Here {
            i,
            raw,
            mut view,
            text,
            owner,
            top,
            setext,
            ..
        } = here;
        let Self {
            lines,
            refs,
            state,
            send,
            frames,
            leaf,
            pending,
            lead,
            ..
        } = self;
        let lines = *lines;
        let is_blank = blank(&text);
        let width = indentation_at(raw, &view, CODE_INDENT).1;
        if *leaf == Some(Leaf::Code) {
            if is_blank || width >= CODE_INDENT {
                state.code.insert(i);
                take_indent(raw, &mut view, CODE_INDENT);
                let code = text_at(raw, &view);
                send.line(LineKind::CodeLine, i, code, "", frames, lead, raw, state);
                return i + 1;
            }
            send.line(
                LineKind::CodeEnd,
                i,
                String::new(),
                "",
                frames,
                lead,
                raw,
                state,
            );
            *leaf = None;
        }
        if !matches!(*leaf, Some(Leaf::Paragraph { .. })) && !is_blank && width >= CODE_INDENT {
            *leaf = Some(Leaf::Code);
            state.code.insert(i);
            take_indent(raw, &mut view, CODE_INDENT);
            let code = text_at(raw, &view);
            send.line(LineKind::CodeLine, i, code, "", frames, lead, raw, state);
            return i + 1;
        }
        if let (Some(refs), None, false) = (refs.as_deref_mut(), *leaf, is_blank) {
            let note = read_footnote_definition(&text);
            // Outside a container a definition may span lines; inside one it is read a line at a time.
            let multi = if note.is_none() && !top {
                definition_at(lines, i)
            } else {
                None
            };
            let single = if note.is_none() && top {
                read_definition(&text)
            } else {
                None
            };
            if let Some((key, target, _, last)) = multi {
                store_definition(&mut refs.defs, key, target);
                hide_definition(state, i, last);
                return last + 1;
            }
            if let Some((id, body)) = note {
                let stored = !refs.notes.contains_key(&id);
                if stored {
                    refs.notes.insert(id.clone(), body.clone());
                    refs.numbers.insert(id.clone(), 0);
                }
                *pending = Some(Pending::Note {
                    owner,
                    id: stored.then_some(id),
                    body: vec![body],
                });
                hide_definition(state, i, i);
                send.line(LineKind::Hidden, i, text, "", frames, lead, raw, state);
                return i + 1;
            }
            if let Some(single) = single {
                if !single.target.is_empty() {
                    store_definition(&mut refs.defs, single.key, single.target);
                    hide_definition(state, i, i);
                    hide_title(lines, i + 1, state, !single.titled, &prefixes(frames));
                    send.line(LineKind::Hidden, i, text, "", frames, lead, raw, state);
                    return i + 1;
                }
                *pending = Some(Pending::Ref {
                    owner,
                    key: single.key,
                    index: i,
                });
            }
        }
        if let Some((level, heading)) = (!top).then(|| atx_heading(&text)).flatten() {
            send.whole(Event::Heading {
                level,
                text: heading.to_string(),
            });
            *leaf = None;
            return i + 1;
        }
        let aligns = (!top && text.contains('|') && i + 1 < lines.len())
            .then(|| delim_aligns(lines[i + 1]))
            .flatten();
        // The delimiter row must match the header's cell count; a row ends the table at a blank line or another block.
        if let Some(aligns) = aligns {
            let head = split_row(&text);
            if head.len() == aligns.len() {
                let mut rows = Vec::new();
                let mut i = i + 2;
                while i < lines.len() && !blank(lines[i]) && !starts_block(lines[i]) {
                    rows.push(split_row(lines[i]));
                    i += 1;
                }
                send.whole(Event::Table { head, aligns, rows });
                *leaf = None;
                return i;
            }
        }
        if kill_definition(&text) != text {
            state.escaped.insert(i);
        }
        let ends = is_blank || setext || is_thematic(&text) || heading_line(&text);
        send.line(
            if setext && !top {
                LineKind::Setext
            } else {
                LineKind::Run
            },
            i,
            text,
            "",
            frames,
            lead,
            raw,
            state,
        );
        *leaf = (!ends).then_some(Leaf::Paragraph { owner });
        i + 1
    }
}
