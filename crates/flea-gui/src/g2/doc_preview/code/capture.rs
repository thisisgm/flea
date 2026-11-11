//! Passive pixel measurements of the active code viewport, using only already prepared lines.
use super::CodeView;

pub(super) struct Syntax {
    pub text: std::rc::Rc<super::shape::Prepared>,
    pub span: std::ops::Range<usize>,
    pub ink: gpui::Hsla,
}

impl CodeView {
    pub(crate) fn language_capture(&self) -> flea::jsondoc::Json {
        use flea::jsondoc::Json;
        let pieces = self.painted_syntax.borrow();
        let mut colors = Vec::new();
        for piece in pieces.iter() {
            let color = Json::Str(crate::ipc::rows::colour(piece.ink));
            if !colors.contains(&color) {
                colors.push(color);
            }
        }
        crate::visual::object([
            ("path", Json::Str(self.lines.path.clone())),
            (
                "answer",
                Json::Str(format!("{:?}", self.lines.answer).to_lowercase()),
            ),
            ("firstScreen", Json::Bool(self.first_screen())),
            ("total", Json::Num(self.lines.total.to_string())),
            (
                "bodies",
                Json::Arr(
                    pieces
                        .iter()
                        .filter_map(|piece| piece.text.text.get(piece.span.clone()))
                        .map(|text| Json::Str(text.into()))
                        .collect(),
                ),
            ),
            ("colors", Json::Arr(colors)),
            (
                "inks",
                Json::Arr(
                    self.palette
                        .inks
                        .iter()
                        .map(|ink| Json::Str(crate::ipc::rows::colour(*ink)))
                        .collect(),
                ),
            ),
        ])
    }

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
