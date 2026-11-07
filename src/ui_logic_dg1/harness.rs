//! The JS tests' pane double: a shared window session, one queue per pane, sent and said lines, transfer asks, text publishes and paths asks; it mirrors tests/js/clipboard.js pane() and fork shares the session for the dual-pane races.
use super::clipboard::ReadOut;
use super::paste::{copy_as_resolved, copy_as_text, plan, CopyAsOut, PendingPaste, Stamp};
use super::{words, Clip, EntryHandle, Out, PasteAsk, Queue, Said, Session, TakeOut, View};
use crate::i18n::{I18n, Paths};
use crate::ui_logic_mn1::copyas::Format;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: vec![crate::i18n::test_catalogs::english_only()],
    })
}

pub struct Pane {
    pub session: Rc<RefCell<Session>>,
    pub queue: Queue,
    pub backend: u64,
    pub path: String,
    pub listing: u64,
    pub list_in_flight: bool,
    pub recent: bool,
    pub sent: Vec<String>,
    pub said: Vec<Said>,
    pub asked: Vec<PasteAsk>,
    pub texts: Vec<String>,
    pub rows_asked: Vec<Vec<usize>>,
    pub copy_as_pending: Option<(Format, String)>,
}

impl Pane {
    pub fn new() -> Self {
        Self {
            session: Rc::new(RefCell::new(Session::new())),
            queue: Queue::new(),
            backend: 1,
            path: "/dest".into(),
            listing: 1,
            list_in_flight: false,
            recent: false,
            sent: Vec::new(),
            said: Vec::new(),
            asked: Vec::new(),
            texts: Vec::new(),
            rows_asked: Vec::new(),
            copy_as_pending: None,
        }
    }

    pub fn fork(&self) -> Self {
        Self { session: Rc::clone(&self.session), queue: Queue::new(), ..Self::new() }
    }

    pub fn on(&mut self, backend: u64) -> &mut Self {
        self.backend = backend;
        self
    }

    pub fn apply(&mut self, outs: Vec<Out>) {
        for out in outs {
            match out {
                Out::Send(line) => self.sent.push(line),
                Out::Say(said) => self.said.push(said),
                Out::AskPaths(rows) => self.rows_asked.push(rows),
                Out::Ask(ask) => self.asked.push(ask),
                Out::Text(text) => self.texts.push(text),
            }
        }
    }

    // Ops.clip: rows when paths is None, immediate paths when the menu took them; a rows take refuses while another asker holds the shared paths slot.
    pub fn clip(&mut self, en: &I18n, moving: bool, paths: Option<Vec<String>>, rows: Vec<usize>) {
        let path = self.path.clone();
        let busy = paths.is_none() && self.copy_as_pending.is_some();
        let view = View { path: &path, listing: self.listing, paths_busy: busy, list_in_flight: self.list_in_flight };
        let out = self.queue.take(en, moving, paths, rows, view);
        match out {
            TakeOut::Publish(paths, moving) => {
                let outs = self.session.borrow_mut().set(self.backend, paths.clone(), moving);
                self.apply(outs);
                self.said.push(words::landed(en, moving, paths.len()));
            }
            TakeOut::AskPaths(rows) => {
                if !rows.is_empty() {
                    self.rows_asked.push(rows);
                }
            }
            TakeOut::Wait => {}
            TakeOut::Busy(said) => self.said.push(said),
        }
    }

    pub fn clip_failed(&mut self, en: &I18n) {
        self.queue.failed();
        let path = self.path.clone();
        let view = View { path: &path, listing: self.listing, paths_busy: false, list_in_flight: self.list_in_flight };
        match self.queue.step(en, view) {
            TakeOut::AskPaths(rows) if !rows.is_empty() => self.rows_asked.push(rows),
            TakeOut::Busy(said) => self.said.push(said),
            _ => {}
        }
    }

    pub fn resolved(&mut self, en: &I18n, list: Vec<String>) {
        let path = self.path.clone();
        let view = View { path: &path, listing: self.listing, paths_busy: false, list_in_flight: self.list_in_flight };
        match self.queue.resolved(en, list, view) {
            Some(Ok(publish)) => {
                let paths = publish.paths.clone();
                let outs = self.session.borrow_mut().set(self.backend, paths.clone(), publish.moving);
                self.apply(outs);
                self.said.push(words::landed(en, publish.moving, paths.len()));
            }
            Some(Err(stale)) => self.said.push(stale),
            None => {}
        }
        match self.queue.step(en, view) {
            TakeOut::AskPaths(rows) if !rows.is_empty() => self.rows_asked.push(rows),
            TakeOut::Busy(said) => self.said.push(said),
            _ => {}
        }
    }

    pub fn set_ok(&mut self, token: &str) {
        let backend = self.backend;
        let (outs, _validated) = self.session.borrow_mut().on_set(&english(), backend, true, token, "");
        self.apply(outs);
    }

    pub fn changed(&mut self, clip: &str, paths: Vec<String>, token: &str) {
        let backend = self.backend;
        let outs = self.session.borrow_mut().on_changed(backend, clip, paths, token.into(), "");
        self.apply(outs);
    }

    pub fn watch_error(&mut self) {
        let backend = self.backend;
        let outs =
            self.session.borrow_mut().on_changed(backend, "none", Vec::new(), String::new(), "no data-control");
        self.apply(outs);
    }

    pub fn get_ok(&mut self, en: &I18n, clip: &str, paths: Vec<String>, token: &str) {
        let backend = self.backend;
        let (_, pendings) =
            self.session.borrow_mut().on_get(backend, true, clip, paths, token.to_string());
        for pending in pendings {
            self.fire(en, &pending);
        }
    }

    pub fn get_err(&mut self, en: &I18n) {
        let backend = self.backend;
        let (_, pendings) = self.session.borrow_mut().on_get(backend, false, "", Vec::new(), String::new());
        for pending in pendings {
            self.fire(en, &pending);
        }
    }

    pub fn paste(&mut self, en: &I18n, pending: PendingPaste) {
        let backend = self.backend;
        let (outs, read) = self.session.borrow_mut().read_for_paste(en, backend, pending.clone());
        self.apply(outs);
        if read == ReadOut::Now {
            self.fire(en, &pending);
        }
    }

    fn fire(&mut self, en: &I18n, pending: &PendingPaste) {
        let stamp = Stamp { year: 2026, month: 9, day: 23, hour: 10, minute: 14, second: 32 };
        let path = self.path.clone();
        let outs = plan(&self.session.borrow().current().clone(), en, pending, &path, self.list_in_flight, self.recent, stamp);
        self.apply(outs);
    }

    pub fn pasted(&self) -> String {
        match self.asked.last() {
            Some(PasteAsk::Transfer { op, paths, .. }) => format!("{op}:{}", paths.join(",")),
            Some(PasteAsk::Link { kind, paths, .. }) => format!("link:{}:{}", kind.op(), paths.join(",")),
            Some(PasteAsk::Image { .. }) => "image".into(),
            None => "nothing".into(),
        }
    }

    pub fn copy_as(&mut self, format: Format, paths: Option<Vec<String>>, rows: Vec<usize>) {
        match paths {
            Some(paths) => {
                if let Some(text) = copy_as_text(&paths, format, &self.path) {
                    self.texts.push(text);
                }
            }
            None => {
                if self.copy_as_pending.is_some() || self.queue.pending() {
                    self.said.push(Said { text: words::resolving(&english()), error: false, paste_hint: false });
                    return;
                }
                self.copy_as_pending = Some((format, self.path.clone()));
                self.rows_asked.push(rows);
            }
        }
    }

    pub fn copy_as_resolved(&mut self, list: Vec<String>) {
        if let Some((format, base)) = self.copy_as_pending.take() {
            if let CopyAsOut::Text(text) = copy_as_resolved(&list, format, &base) {
                self.texts.push(text);
            }
        }
    }

    pub fn current(&self) -> Clip {
        self.session.borrow().current().clone()
    }

    // The snapshot and shared entry identity a paste holds until completion or cancellation.
    pub fn captured(&self) -> (Clip, EntryHandle) {
        (self.current(), self.session.borrow().capture())
    }

    pub fn watch_failed(&self) -> bool {
        self.session.borrow().watch_failed(self.backend)
    }
}
