//! Clipboard mark geometry follows the final painted caption line.
use super::{
    paint::{bounds_of, caption_line},
    Drawn, GridState,
};
use crate::{grid::geometry::Rect, row::marks::ClipMark};
use gpui::{point, px};

impl Drawn {
    /// The clipboard bounds paint uses, after the actual last drawn caption line, including its fallback face.
    pub fn clip_bounds(&self, state: &GridState) -> Option<Rect> {
        if self.editing {
            return None;
        }
        let clip = self.clip.as_ref()?;
        let metrics = state.layout.as_ref()?.metrics;
        let last = self.lines.checked_sub(1)?;
        let width = caption_line(state, self, last)?.centred;
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
        .bounds(
            name,
            px((self.caption.width + width) / 2.),
            px(metrics.clip_mark),
            px(metrics.gap),
        );
        Some(Rect {
            x: f32::from(mark.origin.x),
            y: f32::from(mark.origin.y),
            width: f32::from(mark.size.width),
            height: f32::from(mark.size.height),
        })
    }
}
