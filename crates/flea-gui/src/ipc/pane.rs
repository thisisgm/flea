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
        let mark = &swap.mark;
        let value = match function {
            "listInFlight" => pane.busy().to_string(),
            // Ipc.qml mode() is the constant "browse", which v0.3.12 never reports as anything else.
            "mode" => "browse".into(),
            // Recent is the history listing under flea:recent; GPUI keeps no origin for it, so recentFrom reads idle.
            "recentFrom" => String::new(),
            "listRequests" => pane.list_requests.to_string(),
            "firstRowsAt" => pane.first_rows_at.to_string(),
            "inputToRows" => format!("{} {}", swap.input_at, swap.rows_at),
            "state" => pane.state.kind.name().into(),
            "stateMessage" => pane.state.text(self.catalog, pane.held.total).message,
            "listingWindowState" => format!(
                "{{\"held\":{},\"loaded\":{},\"windowSize\":{},\"total\":{},\"shownTotal\":{}}}",
                pane.held.start, pane.held.rows.len(), pane.first_count(), pane.held.total, pane.held.total),
            // Legacy frame names and paint aliases do not count terminal feedback.
            "swapState" => format!(
                "{{\"holding\":{},\"fellBack\":{},\"holds\":{},\"fallbacks\":{},\"waits\":{},\"blankFrames\":{},\"loadingFrames\":{},\"blinkBacks\":null,\"legacyFrameCounterSemantics\":\"paint-invocations\",\"legacyFrameCounterCoverage\":\"paint-only\",\"paintBlankFrames\":{},\"paintLoadingFrames\":{},\"last\":{{\"ms\":{},\"end\":\"{}\"}},\"shownMs\":{},\"commits\":{},\"mark\":{{\"shown\":{},\"offset\":{:.3},\"mode\":\"{}\",\"label\":\"{}\",\"frames\":{},\"writes\":{},\"requests\":{},\"others\":{}}},\"presentation\":{}}}",
                swap.holding, swap.fell_back, count.holds, count.fallbacks, count.waits,
                count.blank_frames + swap.frames.blank.get(), count.loading_frames + swap.frames.loading.get(),
                count.blank_frames + swap.frames.blank.get(), count.loading_frames + swap.frames.loading.get(), count.last_ms, escape(count.last_end),
                count.shown_ms, count.commits, mark.shown(), mark.offset(), if mark.is_spiral() { "spiral" } else { mark.mode().id() }, escape(mark.label()), mark.counters.frames.get(),
                mark.counters.writes.get(), mark.counters.requests.get(), mark.counters.others.get(),
                flea::jsondoc::render(&swap.observed.borrow().json()).trim()),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
