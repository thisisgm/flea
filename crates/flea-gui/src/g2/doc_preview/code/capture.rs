//! Passive pixel measurements of the active code viewport, using only already prepared lines.
use super::CodeView;

impl CodeView {
    pub(crate) fn viewport_metrics(&self) -> (f32, f32, f32) {
        let row = self.metrics.row.max(1.);
        let (mut before, mut total) = (self.top, self.lines.shown());
        if self.wrap {
            // Unheld lines occupy one row, as rows_of() does, without scanning or preparing them.
            for (line, prepared) in self.prepared.borrow().1.iter() {
                let extra = prepared.rows.len().saturating_sub(1);
                if *line < self.lines.shown() {
                    total += extra;
                    if *line < self.top {
                        before += extra;
                    }
                }
            }
        }
        (
            before as f32 * row + self.offset,
            self.frame.1,
            total as f32 * row,
        )
    }
}
