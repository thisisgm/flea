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
    /// Lost submissions or terminals so far, and the oldest submission still awaiting its terminal.
    pub(crate) fn search_coverage(&self) -> (usize, Option<u64>) {
        (
            self.lost_metadata + self.unmatched_terminals,
            self.pending.front().map(|frame| frame.id),
        )
    }
}
