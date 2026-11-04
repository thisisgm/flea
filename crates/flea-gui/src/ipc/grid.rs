//! Read-only Grid adapters answer from the last drawn frame, the way ui/Ipc.qml reads the live tiles.
use super::Adapter;
use crate::grid::{element::GridState, GridView, Shown};
use flea::{ipc::wire::Reply, json::escape};

pub struct GridAdapter<'a> {
    pub view: &'a GridView,
    pub state: &'a GridState,
}

impl GridAdapter<'_> {
    /// Ipc.qml gridCaptionState: "{}" for a row no tile draws; wrapAt counts UTF-16 units as the JavaScript seam did.
    pub fn caption_state(&self, listing: usize) -> String {
        let (Some(drawn), Some(layout)) =
            (self.state.drawn_row(listing), self.state.layout.as_ref())
        else {
            return "{}".into();
        };
        let name = drawn.caption_text().unwrap_or("");
        let wrap_at = name
            .find('\n')
            .map_or(-1, |at| name[..at].encode_utf16().count() as i64);
        let shaped = name.split('\n').count();
        let truncated = drawn.truncated();
        let text_height = drawn.text_height(layout.metrics.caption_line);
        let slot_height = drawn.caption.height.max(text_height.ceil());
        format!(
            "{{\"name\":\"{}\",\"wrapAt\":{},\"broken\":{},\"lines\":{},\"truncated\":{},\"textHeight\":{},\"slotHeight\":{},\"bottom\":{},\"tileHeight\":{},\"tip\":{}}}",
            escape(name),
            wrap_at,
            !truncated && drawn.lines == shaped,
            drawn.lines,
            truncated,
            text_height,
            slot_height,
            drawn.caption.y - drawn.cell.y + slot_height,
            drawn.cell.height,
            self.tip(listing),
        )
    }
}

impl GridAdapter<'_> {
    // GridTile.qml nameTip as the seam reads it: the whole name and its box while the tip shows on this row, else null.
    fn tip(&self, listing: usize) -> String {
        match self.state.tip().filter(|tip| tip.0 == listing) {
            Some((_, name, rect)) => format!(
                "{{\"text\":\"{}\",\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
                escape(name),
                rect.x,
                rect.y,
                rect.width,
                rect.height
            ),
            None => "null".into(),
        }
    }
}

impl Adapter for GridAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let value = match function {
            "gridColumns" => self
                .state
                .layout
                .map_or_else(
                    || self.view.layout(Shown::All(0)).columns,
                    |layout| layout.columns,
                )
                .to_string(),
            "gridCaptionState" => {
                self.caption_state(arguments.first()?.parse().unwrap_or(usize::MAX))
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
