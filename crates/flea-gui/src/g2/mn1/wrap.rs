//! Wrapped refusal captions prepared with the card, retaining glyph runs for every painted line.
use super::lines::{shape, Env, Shaped};
use flea::ui_logic_mn1::fit::Metrics;
use gpui::{
    point, px, size, Bounds, ContentMask, Hsla, IndentAdjustment, LineFragment, Pixels, Window,
};

pub struct Caption {
    pub width: f32,
    pub lines: Vec<Shaped>,
    line_height: Pixels,
    row_height: Pixels,
    #[cfg(test)]
    painted: std::cell::Cell<Option<Bounds<Pixels>>>,
}

impl Caption {
    pub fn new(word: &Shaped, room: f32, env: &Env, window: &Window) -> Self {
        let width = room.min(word.width());
        let mut lines = Vec::new();
        if width > 0. {
            let mut wrapper = window
                .text_system()
                .line_wrapper(env.typeface.font.clone(), env.typeface.caption);
            let fragments = [LineFragment::text(&word.text)];
            let mut start = 0;
            for end in wrapper
                .wrap_line(&fragments, px(width), IndentAdjustment::NoIndent)
                .map(|boundary| boundary.ix)
                .chain(std::iter::once(word.text.len()))
            {
                lines.push(shape(
                    word.text[start..end].trim().to_owned(),
                    env.typeface.caption,
                    env.typeface,
                    window,
                ));
                start = end;
            }
        }
        let layout = word.run.line();
        Self {
            width,
            lines,
            line_height: layout.ascent + layout.descent,
            row_height: px(Metrics::at(f32::from(env.typeface.body)).row as f32),
            #[cfg(test)]
            painted: std::cell::Cell::new(None),
        }
    }

    pub fn paint(&self, at: gpui::Point<Pixels>, color: Hsla, window: &mut Window) {
        let height = self.line_height * self.lines.len();
        let top = at.y + (self.row_height - height) / 2.;
        let bounds = Bounds::new(point(at.x, top), size(px(self.width), height));
        #[cfg(test)]
        self.painted.set(Some(bounds));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (index, line) in self.lines.iter().enumerate() {
                let x = at.x + px(self.width - line.width());
                let baseline =
                    window.pixel_snap(top + self.line_height * index + line.run.line().ascent);
                let _ = window.paint_glyph_run(point(x, baseline), &line.run, color);
            }
        });
    }

    #[cfg(test)]
    pub(super) fn painted_bounds(&self) -> Option<Bounds<Pixels>> {
        self.painted.get()
    }
}
