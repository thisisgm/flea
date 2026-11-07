//! The accepted raw thumbnail reply and the immutable request that owns it.
use super::*;
use crate::window::pane::PaneHost;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Observation {
    token: String,
    generation: u64,
    directory: String,
    listing: usize,
    index: usize,
    row: RowFacts,
    path: String,
    raw: Option<String>,
}

impl Observation {
    fn new(frame: &Frame) -> Self {
        // A 64-bit serial does not wrap within any process's lifetime, so a token is never reused.
        let serial = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        Self {
            token: serial.to_string(),
            generation: frame.generation,
            directory: frame.subject.directory.clone(),
            listing: frame.subject.listing,
            index: frame.subject.index,
            row: frame.subject.row.clone(),
            path: frame.subject.path(),
            raw: None,
        }
    }

    fn owns(&self, frame: &Frame) -> bool {
        self.generation == frame.generation
            && self.directory == frame.subject.directory
            && self.listing == frame.subject.listing
            && self.index == frame.subject.index
            && self.row == frame.subject.row
            && self.path == frame.subject.path()
    }

    fn accepts(&self, doc: &Json) -> bool {
        doc.get("thumbToken").and_then(Json::as_str) == Some(self.token.as_str())
            && doc.get("thumbPath").and_then(Json::as_str) == Some(self.path.as_str())
            && matches!(doc.get("thumbListing"), Some(Json::Num(value)) if value.parse::<usize>().ok() == Some(self.listing))
    }
}

impl Frame {
    pub(super) fn thumbnail_request(&mut self) -> String {
        let observation = Observation::new(self);
        let line = format!(
            r#"{{"c":"thumb","rows":[{}],"thumbToken":"{}","thumbPath":"{}","thumbListing":{}}}"#,
            observation.index,
            observation.token,
            flea::json::escape(&observation.path),
            observation.listing
        );
        self.thumb_observation = Some(observation);
        line
    }

    pub(super) fn accept_thumbnail(&mut self, doc: &Json) -> bool {
        if let Some(observation) = &self.thumb_observation {
            if !observation.owns(self)
                || !observation.accepts(doc)
                || doc.get("file").and_then(Json::as_str).is_none()
            {
                return false;
            }
        } else if ["thumbToken", "thumbPath", "thumbListing"]
            .iter()
            .any(|key| doc.get(key).is_some())
        {
            return false;
        }
        let raw = doc.get("file").and_then(Json::as_str).unwrap_or("");
        if let Some(observation) = &mut self.thumb_observation {
            observation.raw = Some(raw.to_owned());
        }
        self.thumb_answered = true;
        // PreviewColumn.qml frameSource: an image with no cache file decodes the original, never enlarged.
        let file = match raw {
            "" if self.kind == Kind::Image => self.subject.path(),
            file => file.to_owned(),
        };
        if file.starts_with('/') {
            self.source = Some(std::path::PathBuf::from(file).into());
        }
        true
    }
}

#[derive(Default)]
pub(crate) struct Snapshot {
    pub file: Option<(usize, Option<String>)>,
    pub index: Option<usize>,
    pub path: String,
    pub ready: bool,
}

impl readers::Reader<'_> {
    pub(crate) fn thumbnail_policy(&self, host: &PaneHost) -> Snapshot {
        let Some(view) = self.view.filter(|view| self.active && view.visible) else {
            return Snapshot::default();
        };
        let Some(frame) = view.pending.as_ref().or(view.drawn.as_ref()) else {
            return Snapshot::default();
        };
        let subject = &frame.subject;
        if host.stopped
            || host.backend.is_none()
            || host.pane.busy()
            || subject.directory != host.path
            || subject.listing != host.pane.held.listing
            || subject.index != host.pane.cursor
            || host.pane.held.row(subject.index) != Some(&subject.row)
        {
            return Snapshot::default();
        }
        let file = frame
            .thumb_observation
            .as_ref()
            .filter(|observation| observation.owns(frame))
            .map(|observation| (observation.index, observation.raw.clone()));
        let drawn = view.pending.is_none() && view.drawn.is_some();
        Snapshot {
            file,
            index: drawn.then_some(subject.index),
            path: if drawn { subject.path() } else { String::new() },
            ready: drawn && !frame.failed && frame.source.is_some() && frame.image.is_some(),
        }
    }
}
