//! Read-only pane adapters expose the real transition and bounded-window counters.
use super::Adapter;
use crate::pane::Pane;
use flea::{i18n::I18n, ipc::wire::Reply, json::escape};

pub struct PaneAdapter<'a> {
    pub pane: &'a Pane,
    pub catalog: &'a I18n,
}

impl Adapter for PaneAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let pane = self.pane;
        let swap = &pane.swap;
        let count = &swap.counters;
        let value = match function {
            "listInFlight" => pane.busy().to_string(),
            "listRequests" => pane.list_requests.to_string(),
            "inputToRows" => format!("{} {}", swap.input_at, swap.rows_at),
            "state" => pane.state.kind.name().into(),
            "stateMessage" => pane.state.text(self.catalog, pane.held.total).message,
            "listingWindowState" => format!(
                "{{\"held\":{},\"loaded\":{},\"windowSize\":{},\"total\":{},\"shownTotal\":{}}}",
                pane.held.start, pane.held.rows.len(), pane.first_count(), pane.held.total, pane.held.total),
            "swapState" => format!(
                "{{\"holding\":{},\"fellBack\":{},\"holds\":{},\"fallbacks\":{},\"waits\":{},\"blankFrames\":{},\"loadingFrames\":{},\"last\":{{\"ms\":{},\"end\":\"{}\"}},\"shownMs\":{},\"commits\":{}}}",
                swap.holding, swap.fell_back, count.holds, count.fallbacks, count.waits,
                count.blank_frames + swap.frames.blank.get(), count.loading_frames + swap.frames.loading.get(), count.last_ms, escape(count.last_end),
                count.shown_ms, count.commits),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
