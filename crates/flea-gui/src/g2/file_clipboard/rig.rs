//! The host over recording fakes: verbs, menu picks, marks and menu state, mirroring the oracle's pane with backend id 7 and listing id 11.
use super::seams::{Captured, Foreign, Owner, Publish, Sink, Wire};
use super::{Clipboard, Landing, Take};
use flea::i18n::{I18n, Paths};
use flea::ui_logic_file_clipboard::{PasteAsk, Stamp};
use std::cell::RefCell;
use std::path::PathBuf;

fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent/omarchy"),
        language_file: PathBuf::from("/nonexistent/language"),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

pub(super) struct FakeWire {
    pub sent: RefCell<Vec<String>>,
}

pub(super) struct FakeSink {
    pub asks: RefCell<Vec<(PasteAsk, Captured)>>,
}

pub(super) struct FakeOwner {
    pub published: RefCell<Vec<Publish>>,
    pub texts: RefCell<Vec<String>>,
    pub cleared: RefCell<Vec<(String, Vec<String>)>>,
    pub foreign: RefCell<Vec<Foreign>>,
}

impl FakeWire {
    fn new() -> Self {
        Self {
            sent: RefCell::new(Vec::new()),
        }
    }
}

impl Wire for FakeWire {
    fn send(&self, line: String) -> bool {
        self.sent.borrow_mut().push(line);
        true
    }
}

impl FakeSink {
    fn new() -> Self {
        Self {
            asks: RefCell::new(Vec::new()),
        }
    }
}

impl Sink for FakeSink {
    fn paste(&mut self, ask: PasteAsk, captured: Captured) {
        self.asks.borrow_mut().push((ask, captured));
    }
}

impl FakeOwner {
    fn new() -> Self {
        Self {
            published: RefCell::new(Vec::new()),
            texts: RefCell::new(Vec::new()),
            cleared: RefCell::new(Vec::new()),
            foreign: RefCell::new(Vec::new()),
        }
    }
}

impl Owner for FakeOwner {
    fn publish(&mut self, item: Publish) -> bool {
        self.published.borrow_mut().push(item);
        true
    }

    fn publish_text(&mut self, text: String) -> bool {
        self.texts.borrow_mut().push(text);
        true
    }

    fn clear_if_owned(&mut self, token: &str, cut_paths: &[String]) -> bool {
        self.cleared
            .borrow_mut()
            .push((token.into(), cut_paths.to_vec()));
        true
    }

    fn drain_foreign(&mut self) -> Vec<Foreign> {
        std::mem::take(&mut *self.foreign.borrow_mut())
    }
}

pub(super) struct Rig {
    pub board: Clipboard,
    pub wire: FakeWire,
    pub sink: FakeSink,
    pub owner: FakeOwner,
    pub en: I18n,
}

impl Rig {
    pub(super) fn new() -> Self {
        Self {
            board: Clipboard::new(),
            wire: FakeWire::new(),
            sink: FakeSink::new(),
            owner: FakeOwner::new(),
            en: english(),
        }
    }

    pub(super) fn take_facts(path: &str) -> Take<'_> {
        Take {
            path,
            listing: 11,
            paths_busy: false,
            list_in_flight: false,
        }
    }

    pub(super) fn landing(path: &str) -> Landing<'_> {
        Landing {
            path,
            list_in_flight: false,
            recent: false,
            stamp: Stamp {
                year: 2026,
                month: 9,
                day: 23,
                hour: 10,
                minute: 14,
                second: 32,
            },
        }
    }

    pub(super) fn take(
        &mut self,
        moving: bool,
        rows: Vec<usize>,
    ) -> Vec<flea::ui_logic_file_clipboard::Said> {
        let facts = Take {
            path: "/dest",
            listing: 11,
            paths_busy: false,
            list_in_flight: false,
        };
        self.board.take(
            &self.en,
            7,
            moving,
            None,
            rows,
            facts,
            &self.wire,
            &mut self.sink,
            &mut self.owner,
        )
    }

    pub(super) fn set_lines(&self) -> Vec<String> {
        self.wire.sent.borrow().clone()
    }
}
