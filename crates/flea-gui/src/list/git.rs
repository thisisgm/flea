//! Git marks use the same viewport and content mask as the list's other cells.
use super::{
    columns::{Column, Layout},
    pane::Held,
    ListState,
};
use crate::{
    palette::Theme,
    path::paint::{line, Size},
};
use gpui::{fill, point, px, size, Bounds, Pixels, Window};

pub(super) fn reserve(mut layout: Layout, on: bool, gap: f32) -> Layout {
    if on {
        let width = (14. + gap).min(layout.name.width);
        layout.name.width -= width;
        if let Some(mode) = &mut layout.cells[Column::Mode.slot()] {
            mode.x -= width;
        }
    }
    layout
}

impl ListState {
    pub fn visible(&self) -> &[super::Shown] {
        &self.visible
    }
}

pub(super) fn paint(
    state: &ListState,
    held: &Held,
    bounds: Bounds<Pixels>,
    theme: &Theme,
    window: &mut Window,
) {
    let git = &state.view.git;
    #[cfg(test)]
    git.painted.set(0);
    if git.label.is_empty() {
        return;
    }
    let drawn = &state.drawn;
    let metrics = &drawn.metrics;
    let right = Column::ALL
        .into_iter()
        .filter(|column| *column != Column::Mode)
        .filter_map(|column| drawn.layout.cell(column))
        .map(|cell| cell.x - metrics.gap)
        .fold(
            f32::from(bounds.size.width) - metrics.lane - metrics.pad,
            f32::min,
        );
    let left = bounds.origin.x + px(right - 14.);
    let mut runs = git.runs.borrow_mut();
    if runs
        .as_ref()
        .is_none_or(|(face, _)| face != &state.typeface)
    {
        *runs = Some((state.typeface.clone(), crate::path::paint::Shapes::new(8)));
    }
    let (_, runs) = runs.as_mut().unwrap();
    for shown in &state.visible {
        let Some(mark) = held
            .row(shown.index)
            .and_then(|row| git.marks.get(&row.name))
        else {
            continue;
        };
        let top = bounds.origin.y + px(drawn.grow.top(shown.view, metrics.row) - drawn.scroll_y);
        let ink = match mark.letter {
            'A' => theme.code_inks[1],
            'D' => theme.error,
            '?' => theme.muted,
            _ => theme.code_inks[2],
        };
        #[cfg(test)]
        if mark.folder || mark.letter != '\0' {
            git.painted.set(git.painted.get() + 1);
        }
        if mark.folder {
            window.paint_quad(fill(
                Bounds::new(
                    point(left + px(4.), top + px((metrics.row - 6.) / 2.)),
                    size(px(6.), px(6.)),
                ),
                theme.code_inks[2],
            ));
        } else if mark.letter != '\0' {
            let text = mark.letter.to_string();
            let run = runs.get(&text, Size::Caption, false, || {
                line(&state.typeface, &text, Size::Caption, false, window)
            });
            super::element::paint_line(
                &run,
                left + (px(14.) - run.line().width) / 2.,
                top,
                metrics.row,
                ink,
                window,
            );
        }
    }
}
