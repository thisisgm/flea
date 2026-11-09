//! The footer detail line's height: its text wrapped at the window width, capped at a third of the window.
use crate::state::Shell;
use gpui::{px, Context, Window};

impl Shell {
    pub(crate) fn sync_footer_height(&mut self, window: &Window, cx: &mut Context<Self>) {
        use crate::g2::status_feedback::line::{DETAIL_LINE, DETAIL_PAD_X, DETAIL_PAD_Y};
        let detail = self.surfaces[0]
            .footer
            .as_ref()
            .map(|footer| {
                footer.read_with(cx, |footer, cx| {
                    footer
                        .message_line()
                        .read_with(cx, |line, _| line.drawn().detail.clone())
                })
            })
            .unwrap_or_default();
        let height = if detail.is_empty() {
            0.
        } else {
            let typeface = self.active_typeface();
            let run = gpui::TextRun {
                len: detail.len(),
                font: typeface.font,
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let width = (window.viewport_size().width - px(2. * DETAIL_PAD_X)).max(px(1.));
            let lines: usize = window
                .text_system()
                .shape_text(detail, typeface.caption, &[run], Some(width), None)
                .map_or(1, |lines| {
                    lines
                        .iter()
                        .map(|line| line.wrap_boundaries().len() + 1)
                        .sum()
                });
            (lines as f32 * DETAIL_LINE + 2. * DETAIL_PAD_Y)
                .min(f32::from(window.viewport_size().height) / 3.)
        };
        self.surfaces[0].detail_height = height;
    }
}
