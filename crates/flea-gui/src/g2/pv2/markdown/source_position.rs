//! Read the Source viewport's pixel position without changing its held rows or layout caches.
use super::*;

impl Source {
    pub fn scroll_y(&self) -> f32 {
        if !self.frame.wrap {
            let notice = usize::from(self.notice.is_some() && self.top > 0);
            return (self.top - notice) as f32 * self.frame.line
                + notice as f32 * self.frame.notice
                + self.offset;
        }
        let mut measure = None;
        let text_x = self.gutter() + 2. * self.frame.advance;
        let before: f32 = (0..self.top)
            .map(|row| {
                let Some(n) = self.line_of(row) else {
                    return self.frame.notice;
                };
                let lines = if let Some(lines) = self.laid.get(&n) {
                    lines.len().max(1)
                } else if let Some(runs) = self.row(n) {
                    let runs = pairs(
                        &runs
                            .iter()
                            .map(|(role, text)| (*role, text.as_str()))
                            .collect::<Vec<_>>(),
                    );
                    row_lines(
                        &runs,
                        (self.frame.width - text_x).max(0.),
                        true,
                        self.frame.body,
                        measure.get_or_insert_with(|| self.measure.reader()),
                    )
                    .len()
                    .max(1)
                } else {
                    1
                };
                lines as f32 * self.frame.line
            })
            .sum();
        before + self.offset
    }
}

#[gpui::test]
fn pb_scroll_source_position_survives_wrapped_row_cache_eviction(cx: &mut gpui::TestAppContext) {
    let text = std::sync::Arc::<str>::from("long words with repeated content ".repeat(12) + "\n");
    let text = std::sync::Arc::<str>::from(text.repeat(100));
    let mut source = cx.update(|cx| {
        Source::new(TextMeasure::new(
            cx.text_system().clone(),
            super::super::Faces::omarchy_default(),
        ))
    });
    source.set_text(
        text.clone(),
        std::sync::Arc::new(super::super::line_starts(&text)),
    );
    source.answer = Answer::Plain;
    source.total = 100;
    source.set_frame(
        Frame {
            width: 200.,
            height: 100.,
            body: 14.,
            caption: 12.,
            line: 20.,
            advance: 8.,
            notice: 30.,
            wrap: true,
        },
        Some("notice"),
    );
    source.scroll_by(157.5);
    assert!(source.top > 0);
    assert_eq!(source.scroll_y(), 157.5);
    source.laid.clear();
    assert_eq!(source.scroll_y(), 157.5);
    source.set_frame(
        Frame {
            wrap: false,
            ..source.frame
        },
        Some("notice"),
    );
    assert_eq!(source.scroll_y(), 30. + (source.top - 1) as f32 * 20.);
}
