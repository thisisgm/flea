//! Immutable composed facts and explicitly partial terminal-feedback coverage.
use flea::jsondoc::Json;
use std::{rc::Rc, time::Duration};

#[derive(Clone)]
pub(crate) struct Content {
    pub identity: u64,
    pub generation: usize,
    pub search_generation: usize,
    /// The search generation whose own rows this scene holds, and whether its answer had settled.
    pub search_scene: Option<(u64, bool)>,
    pub held: Rc<super::Held>,
    pub path: gpui::SharedString,
    pub cursor: usize,
    pub drawn_cursor: Option<usize>,
    pub painted_rows: Vec<usize>,
    pub viewport: [f32; 3],
    pub font: gpui::SharedString,
    pub text_size: f32,
    pub theme: crate::palette::Theme,
    pub selected: Rc<[usize]>,
    pub status: Option<gpui::SharedString>,
}

impl Content {
    pub fn painted(&self, list: &crate::list::ListState, ready: bool) -> Self {
        let mut content = self.clone();
        if ready {
            content.drawn_cursor = list.drawn_cursor;
            content.painted_rows = list
                .visible()
                .iter()
                .filter(|row| row.prepared.is_some())
                .map(|row| row.index)
                .collect();
        }
        content
    }

    pub(crate) fn json(&self) -> Json {
        use crate::visual::{number, object};
        object([
            ("pane", number(self.identity)),
            ("generation", number(self.generation)),
            ("searchGeneration", number(self.search_generation)),
            ("path", Json::Str(self.path.to_string())),
            ("listing", number(self.held.listing)),
            ("total", number(self.held.total)),
            ("rowsStart", number(self.held.start)),
            (
                "heldRows",
                Json::Arr(
                    self.held
                        .rows
                        .iter()
                        .map(|row| Json::Str(row.name.clone()))
                        .collect(),
                ),
            ),
            (
                "rows",
                Json::Arr(
                    self.painted_rows
                        .iter()
                        .filter_map(|index| self.held.row(*index))
                        .map(|row| Json::Str(row.name.clone()))
                        .collect(),
                ),
            ),
            (
                "rowIndices",
                Json::Arr(self.painted_rows.iter().map(number).collect()),
            ),
            ("modelCursor", number(self.cursor)),
            ("cursor", self.drawn_cursor.map_or(Json::Null, number)),
            (
                "viewport",
                Json::Arr(self.viewport.iter().map(number).collect()),
            ),
            ("font", Json::Str(self.font.to_string())),
            ("textSize", number(self.text_size)),
            ("theme", Json::Str(format!("{:?}", self.theme))),
            (
                "modelSelection",
                Json::Arr(self.selected.iter().map(number).collect()),
            ),
            ("selection", Json::Null),
            ("selectionCoverage", Json::Str("uncomposed".into())),
            ("status", Json::Null),
            ("statusCoverage", Json::Str("uncomposed-footer".into())),
            ("surfaceCoverage", Json::Str("list-pane-only".into())),
            (
                "modelStatus",
                self.status
                    .as_ref()
                    .map_or(Json::Null, |status| Json::Str(status.to_string())),
            ),
        ])
    }
}

#[derive(Default)]
pub(crate) struct Observed {
    pub available: bool,
    pub frames: u64,
    pub blank: u64,
    pub loading: u64,
    pub loading_submissions: u64,
    pub outstanding_loading: usize,
    pub wait_unavailable: bool,
    pub discarded: u64,
    pub unavailable: u64,
    pub observer_ms: Option<u64>,
    pub native_at: Option<Duration>,
    pub clock_id: Option<u32>,
    pub unobserved_releases: u64,
    pub last: Option<Rc<Content>>,
    pub last_id: u64,
}

impl Observed {
    pub fn json(&self) -> Json {
        use crate::visual::{number, object};
        let observed = |value| {
            if self.available {
                number(value)
            } else {
                Json::Null
            }
        };
        object([
            (
                "coverage",
                Json::Str(
                    if self.available {
                        "terminal-stream"
                    } else {
                        "unavailable"
                    }
                    .into(),
                ),
            ),
            ("exhaustive", Json::Bool(false)),
            ("frames", observed(self.frames)),
            ("blankFrames", observed(self.blank)),
            ("loadingFrames", observed(self.loading)),
            ("submittedLoadingFrames", observed(self.loading_submissions)),
            (
                "outstandingLoadingFrames",
                observed(self.outstanding_loading as u64),
            ),
            ("discardedFrames", observed(self.discarded)),
            ("loadingWaitUnavailable", Json::Bool(self.wait_unavailable)),
            ("unavailableFrames", observed(self.unavailable)),
            ("blinkBacks", Json::Null),
            ("chronology", Json::Str("unavailable".into())),
            ("observerMs", self.observer_ms.map_or(Json::Null, number)),
            ("minimumClock", Json::Str("app-observer".into())),
            (
                "nativePresentedNs",
                self.native_at
                    .map_or(Json::Null, |at| Json::Str(at.as_nanos().to_string())),
            ),
            ("nativeClockId", self.clock_id.map_or(Json::Null, number)),
            ("unobservedReleases", number(self.unobserved_releases)),
            (
                "highestObservedSubmission",
                self.last
                    .as_ref()
                    .map_or(Json::Null, |content| content.json()),
            ),
            (
                "highestObservedSubmissionFrame",
                self.last
                    .as_ref()
                    .map_or(Json::Null, |_| number(self.last_id)),
            ),
        ])
    }
}
