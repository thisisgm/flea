//! File loading and validation of the bounded parse prepared before Space.
use super::*;
use super::{
    doc::PictureState,
    style::hex,
    worker::{Parse, Reply},
};
use flea::ui_logic_doc_preview::markdown::markdown_prepared;

impl MarkdownView {
    /// The document shown: a new path reads and parses off the UI thread and draws nothing until its first screen is ready.
    pub fn set_path(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.path == path {
            return;
        }
        self.path = path;
        self.scroll = 0.;
        self.close_focused = false;
        self.load(cx);
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active != active {
            self.active = active;
            self.load(cx);
        }
    }

    /// Taken back from the closed card: it stops asking that card's host and keeps what it drew.
    pub fn park(&mut self) {
        self.code = None;
    }

    /// A view taken back from the closed card opens again as a fresh preparation would: at the top, Rendered, unselected.
    pub fn rewind(&mut self, cx: &mut Context<Self>) {
        self.selection.clear();
        self.view = View::Rendered;
        self.close_focused = false;
        self.pointer = Default::default();
        self.scroll = 0.;
        self.source.top = 0;
        self.source.offset = 0.;
        cx.notify();
    }

    pub fn configure_prepare(&mut self, frame: (f32, f32)) {
        self.preparing = true;
        self.read_limit = markdown_prepared::MAX_BYTES;
        self.frame = frame;
    }

    pub fn finish_prepare(&mut self, cx: &mut Context<Self>) {
        self.preparing = false;
        self.read_limit = MAX_BYTES;
        if !self.doc.complete || self.doc.text.is_none() {
            self.load(cx);
            return;
        }
        self.doc.seq += 1;
        self.doc.shown = false;
        self.doc.status = Status::Loading;
        self.confirming = true;
        for (at, asked) in self.doc.asked.iter_mut().enumerate() {
            if self.doc.laid[at].is_none() {
                *asked = (0, 0);
            }
        }
        self.send_read(self.doc.text.clone());
        cx.notify();
    }

    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        self.selection.clear();
        let seq = self.doc.seq + 1;
        self.confirming = false;
        self.doc.reset(seq);
        self.counts.first = None;
        self.source.reset();
        self.tables.clear();
        if self.path.is_none() || !self.active {
            cx.notify();
            return;
        }
        self.doc.status = Status::Loading;
        self.doc.loaded_at = self.now(cx);
        self.send_read(None);
        cx.notify();
    }

    fn send_read(&self, previous: Option<Arc<str>>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let parse = Parse {
            dir: path
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default(),
            chrome: hex(self.palette.code),
            ink: hex(self.palette.foreground),
            words: flea::ui_logic_doc_preview::markdown::Words {
                remote: self.words.remote.to_string(),
            },
            head: true,
            max_bytes: self.read_limit,
            previous,
        };
        self.workers
            .load(self.doc.seq, path, parse, self.source.truncate);
    }

    pub(super) fn prepare_answer(&mut self, reply: &Reply) {
        if matches!(reply, Reply::Read { seq, .. } if self.confirming && *seq == self.doc.seq) {
            let seq = self.doc.seq;
            self.confirming = false;
            self.doc.reset(seq);
            self.source.reset();
            self.tables.clear();
            self.counts.first = None;
        }
    }

    pub(super) fn resume_prepared(&mut self, cx: &mut Context<Self>) {
        self.doc
            .pictures
            .retain(|_, picture| matches!(picture, PictureState::Drawn(_)));
        let laid: Vec<_> = self
            .doc
            .laid
            .iter()
            .enumerate()
            .filter_map(|(i, laid)| laid.as_ref().map(|laid| (i, laid.geometry.clone())))
            .collect();
        for (i, geometry) in laid {
            self.note_pictures(i, &geometry);
        }
        self.warm(cx);
    }
}
