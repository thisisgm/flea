//! Frozen v0.3.12 schema projected from the current lender and accepted selection request.
use super::*;
use flea::json::escape;

impl Thumbnails<'_> {
    pub(super) fn policy_state(&self) -> String {
        let mut files = if self.host.drawn_view.is_some()
            && self.host.backend.is_some()
            && !self.host.stopped
            && !self.host.pane.busy()
        {
            self.host.list.borrow().pictures.policy_files(
                &self.host.pane.held,
                &self.host.path,
                self.policy,
            )
        } else {
            Default::default()
        };
        if let Some((row, raw)) = &self.preview.file {
            // A pending column request cannot erase a source the current row lender already accepted.
            if raw.is_some() || !files.contains_key(row) {
                files.insert(*row, raw.clone());
            }
        }
        let pending = files
            .iter()
            .filter(|(_, file)| file.as_deref().is_none_or(|file| file == "cache-asked"))
            .map(|(row, _)| row.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let files = files
            .iter()
            .map(|(row, file)| {
                let value = file
                    .as_ref()
                    .map_or("null".into(), |file| format!("\"{}\"", escape(file)));
                format!("\"{row}\":{value}")
            })
            .collect::<Vec<_>>()
            .join(",");
        let content_y = match self.host.drawn_view {
            Some(View::List | View::Dual) => self.host.list.borrow().drawn.scroll_y.round() as i64,
            Some(View::Grid) => self
                .host
                .grid
                .as_ref()
                .map_or(0, |grid| grid.state.borrow().scroll.round() as i64),
            Some(View::Columns) => self.host.pane.scroll_y.round() as i64,
            None => 0,
        };
        let index = self.preview.index.map_or(-1, |index| index as i64);
        format!(
            r#"{{"view":"{}","mode":"{}","files":{{{files}}},"pending":[{pending}],"contentY":{content_y},"cursor":{},"previewIndex":{index},"previewPath":"{}","previewReady":{}}}"#,
            view_name(self.host.drawn_view),
            mode_name(self.policy.mode),
            self.host.pane.cursor,
            escape(&self.preview.path),
            self.preview.ready
        )
    }
}
