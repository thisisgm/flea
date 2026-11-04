//! Shape one contents result using the listing's shared file marks.
use super::{right_elide, Block, ContentsState, Prepared};
use crate::g2::sr1::run::{run_in, text_runs, wash_span};
use crate::{columns::cache::size_text, row};
use gpui::{Font, FontFeatures, GlyphRun, Pixels, TextRun, Window};

pub(super) fn prepare(
    block: &Block,
    state: &ContentsState,
    lifted: bool,
    cells: usize,
    window: &Window,
) -> Prepared {
    let face = &state.typeface;
    let text = window.text_system();
    let shaped = |value: &str, size: Pixels, runs: Vec<TextRun>| {
        GlyphRun::new(text.layout_line(value, size, &runs, None))
    };
    let plain =
        |value: &str, size: Pixels| shaped(value, size, text_runs(value, None, false, &face.font));
    // The match is in the file, so the name carries no run; its trailing space is the gap before the folder.
    let name = format!("{} ", row::decorated_name(&block.row));
    let line = right_elide(&block.line, cells);
    let line_run = run_in(&line, &state.query);
    let unjoined = Font {
        features: FontFeatures::disable_ligatures(),
        ..face.font.clone()
    };
    let line_glyphs = shaped(
        &line,
        face.caption,
        text_runs(&line, line_run, lifted, &unjoined),
    );
    Prepared {
        name: plain(&name, face.body),
        location: plain(&block.location, face.caption),
        size: plain(&size_text(&block.row, None, &state.words), face.caption),
        number: plain(&block.line_no.to_string(), face.caption),
        wash: wash_span(line_glyphs.line(), line_run, lifted),
        line: line_glyphs,
        mark: row::marks::Mark::for_facts(&block.row, state.languages),
    }
}
