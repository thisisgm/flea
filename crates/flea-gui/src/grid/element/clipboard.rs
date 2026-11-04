//! Clipboard marks share the reserved caption edge and keep the final caption line's baseline.
use super::{paint::bounds_of, Drawn, GridState};
use crate::{grid::geometry::Rect, row::marks::ClipMark};
use gpui::{point, px};

impl Drawn {
    /// The clipboard bounds paint uses, in the caption's reserved strip.
    pub fn clip_bounds(&self, state: &GridState) -> Option<Rect> {
        if self.editing {
            return None;
        }
        let clip = self.clip.as_ref()?;
        let metrics = state.layout.as_ref()?.metrics;
        let last = self.lines.checked_sub(1)?;
        let name = bounds_of(
            point(px(0.), px(0.)),
            Rect {
                y: self.caption.y + last as f32 * metrics.caption_line,
                height: metrics.caption_line,
                ..self.caption
            },
        );
        let mark = if clip.cut {
            ClipMark::Cut
        } else {
            ClipMark::Copy
        }
        .bounds(name, px(metrics.clip_mark), px(metrics.gap));
        Some(Rect {
            x: f32::from(mark.origin.x),
            y: f32::from(mark.origin.y),
            width: f32::from(mark.size.width),
            height: f32::from(mark.size.height),
        })
    }
}
