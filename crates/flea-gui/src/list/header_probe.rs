//! Geometry receipts observe the same cached runs and masks the custom header paints.
use super::{words::CELLS, HeaderState};
use crate::list::{columns::Column, ListState};
use gpui::{canvas, point, px, size, Bounds, IntoElement, Styled};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn element(side: usize, state: Rc<RefCell<ListState>>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let state = state.borrow();
            let (Some(probe), Some(titles)) = (&state.probe, &state.header.titles) else {
                return;
            };
            let drawn = &state.header.drawn;
            // The paint's own metrics, so a Dual pane's slot and label column match the drawn titles.
            let metrics = state.file_metrics();
            let parent = format!("header-{side}");
            for column in CELLS {
                let Some(cell) =
                    HeaderState::title_cell(&drawn.layout, &metrics, column, state.view.dual)
                else {
                    continue;
                };
                let title = column.map_or(&titles.name, |column| &titles.columns[column.slot()]);
                let (run, text) = title.drawn();
                let line = run.line();
                let left = drawn.bounds.left();
                let top = drawn.bounds.top();
                let x = match column {
                    Some(Column::Size | Column::Date | Column::Kind) => {
                        left + px(cell.right()) - line.width
                    }
                    _ => left + px(cell.x),
                };
                let baseline = window.pixel_snap(
                    top + px(metrics.header_top)
                        + (px(metrics.header - metrics.header_top) - line.ascent - line.descent)
                            / 2.
                        + line.ascent,
                );
                let ink = Bounds::new(
                    point(x, baseline - line.ascent),
                    size(line.width, line.ascent + line.descent),
                );
                let mask = Bounds::new(
                    point(left + px(cell.x), top),
                    size(px(cell.width), drawn.bounds.size.height),
                );
                let key = match column {
                    None => "name",
                    Some(Column::Mode) => "mode",
                    Some(Column::Size) => "size",
                    Some(Column::Date) => "modified",
                    Some(Column::Kind) => "kind",
                };
                probe.push(
                    &format!("{parent}-{key}"),
                    &parent,
                    "label",
                    ink.intersect(&mask),
                    text,
                    window,
                );
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
