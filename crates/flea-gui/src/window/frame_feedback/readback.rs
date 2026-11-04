//! Readback labels terminal coverage without treating paints as presentations.
use super::Presentation;

impl Presentation {
    pub fn json(&self) -> flea::jsondoc::Json {
        use crate::visual::{number, object};
        use flea::jsondoc::Json;
        let supported = self.observer.is_some();
        let observed = |count| {
            if supported {
                number(count)
            } else {
                Json::Null
            }
        };
        object([
            ("gpuReadiness", super::timing::gpu_readiness()),
            ("presentTiming", self.timing.json(supported)),
            (
                "coverage",
                Json::Str(
                    if supported {
                        "terminal-stream"
                    } else {
                        "unavailable"
                    }
                    .into(),
                ),
            ),
            ("observedFrames", observed(self.frames)),
            ("discardedFrames", observed(self.discarded)),
            ("exhaustive", Json::Bool(false)),
            ("terminalOutcomes", Json::Bool(supported)),
            ("chronology", Json::Str("unavailable".into())),
            (
                "surfaceCoverage",
                Json::Str("uncovered-list-panes-only".into()),
            ),
            ("footerCoverage", Json::Str("uncomposed".into())),
            ("pending", number(self.pending.len())),
            ("lostMetadata", number(self.lost_metadata)),
            ("unmatchedTerminals", number(self.unmatched_terminals)),
        ])
    }
}

impl Presentation {
    /// Report-only read of the actual submitted List scene, before or after its ordinary observer.
    pub(in crate::window) fn gpu_report_scene(&self, draw_id: u64, frame_id: u64) -> Option<f32> {
        let staged = self.staged.borrow();
        let draw = staged
            .as_ref()
            .filter(|draw| draw.id == draw_id)
            .or_else(|| {
                self.pending
                    .iter()
                    .find(|frame| frame.id == frame_id)
                    .map(|frame| &frame.draw)
            })?;
        let frame = draw.panes[0].as_ref()?;
        let list = frame.list.borrow();
        (frame.ready
            && !frame.loading
            && !frame.blank
            && frame.content.held.total == 10_000
            && !frame.content.painted_rows.is_empty()
            && frame.content.painted_rows.len() == list.visible().len())
        .then_some(list.drawn.scroll_y)
    }

    /// Lost submissions or terminals so far, and the oldest submission still awaiting its terminal.
    pub(crate) fn search_coverage(&self) -> (usize, Option<u64>) {
        (
            self.lost_metadata + self.unmatched_terminals,
            self.pending.front().map(|frame| frame.id),
        )
    }
}
