//! A compare card's patch rows, drawn at the view's scroll position.
use super::look::{self, Look};
use flea::ui_logic_pv1::compare::{DiffRow, Kind};
use gpui::{div, prelude::*, px, AnyElement, SharedString};

/// One patch row at the view's position: the two muted numbers, the marker and the text in the row's ink; a hunk header sits at the text's column alone.
pub(super) fn diff_row(
    index: usize,
    row: &DiffRow,
    look: &Look,
    (x, y): (f64, f64),
    width: f64,
) -> AnyElement {
    let at = f64::from(look::DIFF_MARGIN) + index as f64 * f64::from(look::DIFF_LINE) - y;
    let top = px(at as f32);
    let ink = match row.kind {
        Kind::Removed => look.error,
        Kind::Added => look.green,
        Kind::Hunk => look.cyan,
        Kind::Note => look.muted,
        Kind::Context => look.foreground,
    };
    let cell = |left: f32| div().absolute().left(px(left)).top_0();
    let mut line = div()
        .debug_selector(move || format!("pv1-diff-{index}"))
        .absolute()
        .left(px(-x as f32))
        .w(px(width as f32))
        .top(top)
        .h(px(look::DIFF_LINE))
        .line_height(px(look::DIFF_LINE))
        .text_size(px(look::BODY))
        .whitespace_nowrap()
        .text_color(ink);
    if matches!(row.kind, Kind::Context | Kind::Removed | Kind::Added) {
        let number = |n: u64, left: f32| {
            cell(left)
                .w(px(look::DIFF_NUMBER_W))
                .flex()
                .justify_end()
                .text_color(look.muted)
                .child(if n > 0 { n.to_string() } else { String::new() })
        };
        let marker = match row.kind {
            Kind::Removed => "-",
            Kind::Added => "+",
            _ => " ",
        };
        line = line
            .child(number(row.old_no, look::DIFF_OLD))
            .child(number(row.new_no, look::DIFF_NEW))
            .child(cell(look::DIFF_MARKER).child(marker));
    }
    line.child(cell(look::DIFF_TEXT).child(SharedString::from(row.text.clone())))
        .into_any_element()
}
