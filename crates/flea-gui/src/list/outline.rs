//! TreeRows is the one sanctioned depth step and uses the ordinary mark slot.
use super::Metrics;
use flea::protocol::RowFacts;
use gpui::SharedString;

pub fn indent(row: &RowFacts, metrics: &Metrics) -> f32 {
    row.depth as f32 * (metrics.slot + metrics.gap)
}

pub fn glyph(row: &RowFacts, hovered: bool) -> Option<(&'static str, SharedString, &'static [u8])> {
    let (name, svg) = if row.dir && row.open {
        (
            "chevron-down",
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="square" stroke-linejoin="miter"><path d="M6 9l6 6 6-6"/></svg>"#,
        )
    } else if row.dir && row.link.is_none() && hovered {
        (
            "chevron-right",
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="square" stroke-linejoin="miter"><path d="M9 18l6-6-6-6"/></svg>"#,
        )
    } else {
        return None;
    };
    let key = if row.open {
        "flea-tree:chevron-down"
    } else {
        "flea-tree:chevron-right"
    };
    Some((name, SharedString::new_static(key), svg.as_bytes()))
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
    display
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn tree_outline_mark_replaces_folder_and_indents_by_exactly_one_slot() {
        let mut row = RowFacts {
            dir: true,
            ..Default::default()
        };
        let metrics = Metrics::at(px(14.));
        assert!(glyph(&row, false).is_none());
        assert_eq!(glyph(&row, true).unwrap().0, "chevron-right");
        row.open = true;
        assert_eq!(glyph(&row, false).unwrap().0, "chevron-down");
        row.depth = 2;
        assert_eq!(indent(&row, &metrics), 64.);
        let bounds = gpui::Bounds::new(gpui::point(px(46.), px(0.)), gpui::size(px(600.), px(31.)));
        let slot = name_slot(bounds, Some(&row), &metrics);
        assert_eq!(slot.origin.x, px(110.));
        assert_eq!(slot.right(), bounds.right());
        row.dir = false;
        assert!(glyph(&row, true).is_none());
        row.dir = true;
        row.open = false;
        row.link = Some("/elsewhere".into());
        assert!(glyph(&row, true).is_none(), "a symlinked folder never opens in place");
    }
}
