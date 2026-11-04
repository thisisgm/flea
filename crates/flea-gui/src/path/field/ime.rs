//! Where a field tells the input method its text stands: the rectangle its candidate window opens against.
use super::super::entry::Entry;
use super::Line;
use gpui::{point, px, size, Bounds, Pixels};
use std::ops::Range;

// Board lang-ja-rename: the candidate window opens 3 px clear of the field.
const CLEAR: f32 = 3.;

/// The box a field's input-method rectangles span: its frame and the candidate window's clearance, from where unscrolled text starts.
pub fn ime_box(frame: Bounds<Pixels>, text_left: Pixels) -> Bounds<Pixels> {
    Bounds::new(
        point(text_left, frame.top() - px(CLEAR)),
        size(frame.size.width, frame.size.height + px(2. * CLEAR)),
    )
}

/// The input method's rectangle for a UTF-16 range of the entry, measured in the box `text` from where unscrolled text starts; one collapsed inside the composition is the caret.
pub fn ime_bounds(
    line: &Line,
    entry: &Entry,
    range: &Range<usize>,
    text: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let range = entry.ime_range(range);
    let composing = range.is_empty()
        && (entry.marked.as_ref())
            .is_some_and(|marked| (marked.start..=marked.end).contains(&range.start));
    let range = if composing {
        entry.cursor()..entry.cursor()
    } else {
        range
    };
    let (from, to) = line.entry_span(entry, &range);
    let left = text.left() - line.scroll;
    Bounds::from_corners(
        point(left + from, text.top()),
        point(left + to, text.bottom()),
    )
}

/// The UTF-16 offset nearest x, measured from where unscrolled text starts.
pub fn ime_index(line: &Line, entry: &Entry, x: Pixels) -> usize {
    entry.offset_to_utf16(entry.from_display(line.index_for(x + line.scroll)))
}
