//! TreeRows is the one sanctioned depth step and uses the ordinary mark slot.
use super::{ListView, Metrics};
use crate::row::marks::Mark;
use flea::protocol::RowFacts;

pub fn indent(row: &RowFacts, metrics: &Metrics) -> f32 {
    row.depth as f32 * (metrics.slot + metrics.gap)
}

/// An open folder draws the open folder mark, pointer or not; a chooser never opens in place, so draws none.
pub fn mark(view: &ListView, row: &RowFacts) -> Option<Mark> {
    (view.picker.is_none() && row.dir && row.open).then_some(Mark::FolderOpen)
}

/// The right edge of a folder's mark target, from the row's edge to the gap before its name.
pub fn mark_target(view: &ListView, row: &RowFacts, metrics: &Metrics) -> Option<f32> {
    (view.picker.is_none() && row.dir && (row.open || row.link.is_none()))
        .then(|| metrics.pad + indent(row, metrics) + metrics.slot)
}

pub fn name_slot(
    mut bounds: gpui::Bounds<gpui::Pixels>,
    row: Option<&RowFacts>,
    metrics: &Metrics,
) -> gpui::Bounds<gpui::Pixels> {
    let step = gpui::px(row.map_or(0., |row| indent(row, metrics)));
    bounds.origin.x += step;
    bounds.size.width = (bounds.size.width - step).max(gpui::px(0.));
    bounds
}

pub fn shaping(
    source: &crate::row::cache::Shaping,
    row: &RowFacts,
    metrics: &Metrics,
    advance: f32,
) -> crate::row::cache::Shaping {
    let mut shaping = source.clone();
    let cells = (indent(row, metrics) / advance).ceil() as usize;
    shaping.name_cells = shaping.name_cells.saturating_sub(cells);
    shaping.clip_cells = shaping.clip_cells.saturating_sub(cells);
    shaping
}

pub fn display(row: &RowFacts) -> std::borrow::Cow<'_, RowFacts> {
    let mut display = std::borrow::Cow::Borrowed(row);
    if row.depth > 0 {
        display.to_mut().name = row.name.rsplit('/').next().unwrap_or(&row.name).to_owned();
    }
    // A name that is not UTF-8 draws its lossy spelling; its exact tail stays in the facts every path is built from.
    if flea::bytename::is_exact(&display.name) {
        let shown = flea::bytename::display(&display.name).into_owned();
        display.to_mut().name = shown;
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn tree_outline_open_folder_draws_the_open_mark_and_indents_by_exactly_one_slot() {
        let mut row = RowFacts {
            dir: true,
            ..Default::default()
        };
        let metrics = Metrics::at(px(14.));
        let view = ListView::default();
        assert!(
            mark(&view, &row).is_none(),
            "a closed folder keeps its own mark"
        );
        row.open = true;
        assert_eq!(mark(&view, &row), Some(Mark::FolderOpen));
        assert_eq!(Mark::FolderOpen.name(), "folder-open");
        row.depth = 2;
        assert_eq!(indent(&row, &metrics), 64.);
        let bounds = gpui::Bounds::new(gpui::point(px(46.), px(0.)), gpui::size(px(600.), px(31.)));
        let slot = name_slot(bounds, Some(&row), &metrics);
        assert_eq!(slot.origin.x, px(110.));
        assert_eq!(slot.right(), bounds.right());
        row.dir = false;
        assert!(mark(&view, &row).is_none());
        assert!(mark_target(&view, &row, &metrics).is_none());
        row.dir = true;
        row.open = false;
        row.link = Some("/elsewhere".into());
        assert!(
            mark_target(&view, &row, &metrics).is_none(),
            "a symlinked folder never opens in place"
        );
    }

    #[test]
    fn tree_outline_mark_target_runs_from_the_row_edge_to_the_gap_before_the_name() {
        let mut row = RowFacts {
            dir: true,
            ..Default::default()
        };
        let metrics = Metrics::at(px(14.));
        let mut view = ListView::default();
        assert_eq!(
            mark_target(&view, &row, &metrics),
            Some(37.),
            "37px at the first level"
        );
        assert_eq!(
            mark_target(&view, &row, &metrics),
            Some(metrics.label_x() - metrics.gap)
        );
        row.depth = 1;
        assert_eq!(mark_target(&view, &row, &metrics), Some(37. + 32.));
        row.open = true;
        view.picker = Some(true);
        assert!(
            mark(&view, &row).is_none(),
            "a chooser draws its folders closed"
        );
        assert!(
            mark_target(&view, &row, &metrics).is_none(),
            "and takes no mark click"
        );
    }
}
