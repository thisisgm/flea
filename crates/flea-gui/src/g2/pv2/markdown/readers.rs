//! IPC observes the table hits the mounted document paints.
use super::{paint::Hit, MarkdownView, View};
use flea::jsondoc::{self, Json};

impl MarkdownView {
    pub(crate) fn table_state(&self) -> String {
        if !self.active || self.shown_view() != View::Rendered || !self.doc.shown {
            return "{}".into();
        }
        let Some((rect, shift, row)) = self.hits.iter().find_map(|hit| match hit {
            Hit::Table {
                rect,
                shift,
                most,
                row,
                ..
            } if *most > 0. => Some((*rect, *shift, *row)),
            _ => None,
        }) else {
            return "{}".into();
        };
        let (x, y, w, h) = rect;
        let view = crate::ipc::rows::rect_of(self.bounds.0 + x, self.bounds.1 + y, w, h);
        let row = row
            .map(|(x, y, w, h)| {
                crate::ipc::rows::rect_of(self.bounds.0 + x, self.bounds.1 + y, w, h)
            })
            .unwrap_or_default();
        jsondoc::render(&crate::visual::object([
            ("scrollX", Json::Num(shift.round().to_string())),
            ("view", Json::Str(view)),
            ("row", Json::Str(row)),
        ]))
        .trim_end()
        .into()
    }
}
