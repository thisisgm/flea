//! ViewportScrollBar.qml and ScrollOff.js arithmetic, shared by paint, input and IPC.
use super::Frame;

#[derive(Clone, Copy)]
pub struct Bar {
    pub handle: f32,
    pub offset: f32,
    pub travel: f32,
}
impl Bar {
    pub fn new(frame: Frame) -> Self {
        let track = frame.viewport();
        let handle = if frame.content <= track {
            track
        } else {
            (track * track / frame.content).max(24.).min(track)
        };
        let travel = (track - handle).max(0.);
        let offset = if frame.max_scroll() > 0. {
            travel * frame.scroll.clamp(0., frame.max_scroll()) / frame.max_scroll()
        } else {
            0.
        };
        Self {
            handle,
            offset,
            travel,
        }
    }
    pub fn position(self, frame: Frame, offset: f32) -> f32 {
        if self.travel > 0. {
            offset.clamp(0., self.travel) * frame.max_scroll() / self.travel
        } else {
            0.
        }
    }
}

pub fn reveal(frame: Frame, cursor: usize, context: usize) -> f32 {
    let cursor = cursor / frame.columns.max(1);
    let total = frame.total.div_ceil(frame.columns.max(1));
    let top = cursor as f32 * frame.row;
    if context == 0 {
        return if top < frame.scroll {
            top
        } else if top + frame.row > frame.scroll + frame.viewport() {
            top + frame.row - frame.viewport()
        } else {
            frame.scroll
        }
        .clamp(0., frame.max_scroll());
    }
    let visible = (frame.viewport() / frame.row).floor().max(1.) as usize;
    let margin = context.min(visible.saturating_sub(1) / 2);
    let first = (frame.scroll / frame.row).floor() as usize;
    let want = if cursor < first + margin {
        cursor.saturating_sub(margin)
    } else if cursor + margin >= first + visible {
        (cursor + margin + 1).saturating_sub(visible)
    } else {
        first
    };
    let want = want.min(total.saturating_sub(visible));
    if total > visible && want == total - visible {
        return frame
            .scroll
            .max((total as f32 * frame.row - frame.viewport()).max(0.))
            .min(frame.max_scroll());
    }
    if want == first && top >= frame.scroll && top + frame.row <= frame.scroll + frame.viewport() {
        return frame.scroll;
    }
    (want as f32 * frame.row).clamp(0., frame.max_scroll())
}
