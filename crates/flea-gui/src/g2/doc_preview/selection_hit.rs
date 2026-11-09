use super::{Position, Selection, Span};
use gpui::{px, Pixels, Point};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

impl Selection {
    fn nearest(&self, at: Point<Pixels>) -> Option<&Span> {
        self.spans
            .iter()
            .filter(|span| span.clip.size.width > px(0.) && span.clip.size.height > px(0.))
            .min_by(|a, b| {
                let distance = |s: &Span| {
                    let dy =
                        (s.clip.top() - at.y).max(px(0.)) + (at.y - s.clip.bottom()).max(px(0.));
                    let dx =
                        (s.clip.left() - at.x).max(px(0.)) + (at.x - s.clip.right()).max(px(0.));
                    (f32::from(dy), f32::from(dx))
                };
                distance(a)
                    .partial_cmp(&distance(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    pub fn hit(&self, at: Point<Pixels>) -> Option<Position> {
        let span = self.nearest(at)?;
        let byte = span
            .line
            .closest_index_for_x(
                at.x.clamp(span.clip.left(), span.clip.right()) - span.bounds.left(),
            )
            .min(span.len);
        Some(Position {
            chunk: span.chunk,
            byte: span.start + byte,
        })
    }

    /// GTK's text view: a double-click takes the word under the pointer, and nothing on blank space or past a line's end.
    pub(super) fn word_under(&self, at: Point<Pixels>, hit: Position) -> Range<Position> {
        let under = self
            .nearest(at)
            .filter(|span| span.clip.contains(&at))
            .and_then(|span| {
                let x = at.x - span.bounds.left();
                let byte = span.line.index_for_x(x)?.min(span.len);
                Some(Position {
                    chunk: span.chunk,
                    byte: span.start + byte,
                })
            });
        let Some(at) = under else {
            return hit..hit;
        };
        let blank = self.chunk(at.chunk).is_none_or(|chunk| {
            chunk
                .text
                .get(at.byte..)
                .and_then(|rest| rest.chars().next())
                .is_none_or(char::is_whitespace)
        });
        if blank {
            return hit..hit;
        }
        self.word(at)
    }

    pub(super) fn word(&self, hit: Position) -> Range<Position> {
        let Some(chunk) = self.chunk(hit.chunk) else {
            return hit..hit;
        };
        let at = hit.byte.min(chunk.text.len().saturating_sub(1));
        let word = chunk
            .text
            .split_word_bound_indices()
            .find(|(i, word)| *i <= at && at < i + word.len());
        let (start, end) = word.map_or((hit.byte, hit.byte), |(i, w)| (i, i + w.len()));
        Position { byte: start, ..hit }..Position { byte: end, ..hit }
    }
}
