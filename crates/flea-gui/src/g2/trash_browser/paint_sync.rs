//! Keep a complete row and summary snapshot until its matching interaction state can publish.
use super::*;
use crate::g2::trash_browser::list::Line;
use crate::row::marks::Mark;

impl TrashPane {
    pub fn interaction_held(&self) -> bool {
        self.presentation_loading || self.snapshot_held()
    }

    // A window read holds the drawn snapshot only while the viewport shows rows it does not hold yet.
    fn snapshot_held(&self) -> bool {
        let view = &self.host.view;
        match view.pending_op.as_str() {
            "list" | "summary" => true,
            "window" => !view.covers_viewport(),
            _ => false,
        }
    }

    pub(super) fn sync_paint(&mut self) {
        if self.snapshot_held() {
            return;
        }
        let view = &self.host.view;
        let mut paint = self.paint.borrow_mut();
        let face = self.pending_face.take();
        let face_changed = face
            .as_ref()
            .is_some_and(|face| paint.font != face.font || paint.board != Board::at(face));
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
            || face_changed
        {
            paint.committed += 1;
        }
        if let Some(face) = face {
            let board = Board::at(&face);
            paint.set_face(face.font, board);
        }
        let live: HashSet<Resource> = lines.iter().filter_map(|line| line.thumb.clone()).collect();
        self.retired.extend(self.decoded.difference(&live).cloned());
        self.decoded = live;
        paint.lines = lines;
        paint.first = view.first;
        paint.total = view.total;
        paint.scroll_y = view.scroll_y;
        paint.cursor = view.cursor;
        paint.edge = self.cursor_edge();
    }

    /// The cursor's accent edge and wash, only while the list itself holds the keyboard; the strip, a menu, a card or another window stop keep the marked plate.
    fn cursor_edge(&self) -> bool {
        !self.host.view.strip_focus && !self.keys_away && !self.menu_open && self.card.is_none()
    }

    /// The frame reads where the window's keyboard is, since the rail and the tab strip take it without telling the pane.
    pub(crate) fn sync_keys_away(&mut self, window: &Window, cx: &gpui::App) {
        let away = window.focused(cx).is_some() && !self.focus.contains_focused(window, cx);
        if away != self.keys_away {
            self.keys_away = away;
            self.paint.borrow_mut().edge = self.cursor_edge();
        }
    }
}
