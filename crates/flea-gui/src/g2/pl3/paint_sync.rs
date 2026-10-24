//! Keep a complete row and summary snapshot until its matching interaction state can publish.
use super::*;
use crate::g2::pl3::list::Line;
use crate::row::marks::Mark;

impl TrashPane {
    pub fn interaction_held(&self) -> bool {
        self.presentation_loading
            || matches!(
                self.host.view.pending_op.as_str(),
                "list" | "summary" | "window"
            )
    }

    pub(super) fn sync_paint(&mut self) {
        let view = &self.host.view;
        if matches!(view.pending_op.as_str(), "list" | "summary" | "window") {
            return;
        }
        let mut paint = self.paint.borrow_mut();
        paint.source_request = self.rows_request;
        paint.bytes = view.bytes;
        paint.error_text = view.error_text.clone();
        let lines: Vec<Line> = view
            .rows
            .iter()
            .enumerate()
            .map(|(at, row)| Line {
                name: row.name().to_string(),
                location: words::location_text(&self.catalog, &row.original, &self.home),
                deleted: words::deleted_text(&self.catalog, row.deleted.as_deref()),
                mark: if row.directory {
                    Mark::Folder
                } else {
                    Mark::for_row(&row.icon, 0o100644)
                },
                marked: view.is_selected(&row.uri),
                thumb: Some(view.thumbs.file_for(view.first + at, view.policy))
                    .filter(|file| !file.is_empty())
                    .map(|file| Resource::Path(Arc::from(Path::new(file)))),
            })
            .collect();
        let same_text = lines.len() == paint.lines.len()
            && lines.iter().zip(&paint.lines).all(|(a, b)| {
                a.name == b.name && a.location == b.location && a.deleted == b.deleted
            });
        if !same_text || paint.first != view.first {
            paint.revision += 1;
        }
        if paint.lines != lines
            || paint.first != view.first
            || paint.total != view.total
            || paint.scroll_y != view.scroll_y
        {
            paint.committed += 1;
        }
        let live: HashSet<Resource> = lines.iter().filter_map(|line| line.thumb.clone()).collect();
        self.retired.extend(self.decoded.difference(&live).cloned());
        self.decoded = live;
        paint.lines = lines;
        paint.first = view.first;
        paint.total = view.total;
        paint.scroll_y = view.scroll_y;
        paint.cursor = view.cursor;
        paint.edge = !view.strip_focus && !self.menu_open && self.card.is_none();
    }
}
