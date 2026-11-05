// Everything the read loop holds: the tables it parses once, and the listing state it mutates.
use crate::backend::aliases::Aliases;
use crate::backend::archive::Formats;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::lazy::Lazy;
use crate::backend::mime::Db;
use crate::backend::rowguard::FIRST_LISTING;
use crate::backend::search::Search;
use crate::backend::searchsession::Session;
use crate::backend::thumbspec::Thumbnailers;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

// Loaded once on first demand and shared for the rest of the process.
pub struct Tables {
    pub mime: Lazy<Arc<Db>>,
    pub icons: Lazy<Arc<Names>>,
    pub aliases: Lazy<Arc<Aliases>>,
    pub thumbs: Lazy<Arc<Thumbnailers>>,
    // MIME comments are cached by type and selected in the current language; the loop is single-threaded.
    pub kinds: RefCell<Kinds>,
    // Probed once when an archive request first needs the installed formats.
    pub formats: Lazy<Arc<Formats>>,
}

// Which listing the pane holds, so a re-read counts only over its own kind.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Held {
    List,
    ListPaths,
    Walk,
}

// Everything the loop mutates, gathered so a handler takes one borrow instead of ten arguments.
pub struct State {
    pub outline: super::outline::Session,
    pub contents: super::contents::Slot,
    pub git: Option<super::gitreq::Worker>,
    pub listing: Listing,
    pub base: PathBuf,
    // Only the rows a client named, so this never grows with the directory; see AGENTS.md "Thumbnail requests".
    pub asked: Vec<(PathBuf, usize)>,
    // The submitted job's mtime stays fixed when a later window refreshes its metadata.
    pub thumb_mtimes: HashMap<PathBuf, i64>,
    pub thumb_replies: super::thumbreq::identity::Registry,
    pub trash_asked: Vec<super::trashthumb::Ask>,
    pub outstanding: usize,
    // Rows the pool has no room for yet, in request order; each is also in asked, so its answer still follows its row.
    pub deferred: crate::backend::thumbdefer::Deferred,
    // The last window's mode, mtime and link-dir flag per row, so a thumb request reuses them with no stat.
    pub window_meta: HashMap<usize, (u32, i64, bool, u64)>,
    // Answered directory rows, kept until the next list or sort reassigns what a row index names.
    pub dirsizes: HashMap<usize, (u64, bool)>,
    // Rows still to walk, one at a time; dirsizecancel empties this without touching dirsizes.
    pub dirsize_queue: Vec<usize>,
    pub dirsize_worker: super::dirsizeworker::Worker,
    // The subtree walk the loop ticks; None means no search is running.
    pub search: Option<Search>,
    // The live search session; a list, sort or close drops it and frees its arena.
    pub search_session: Option<Session>,
    // When the running walk last announced its count, so SEARCH_REPORT can throttle the stream.
    pub search_reported: Instant,
    // Which numbering the rows are in; forget_rows moves it, see src/backend/rowguard.rs.
    pub generation: u64,
    // The source of the held listing, set by the arm that answered it.
    pub held: Held,
    // The clipboard watcher runs once per backend; a second clipWatch is a no-op.
    pub clip_watching: bool,
    pub nearby: super::nearby::Discovery,
}

impl Tables {
    // Empty windows use the same envelope without demanding any row services.
    pub(crate) fn rows_line(&self, base: &std::path::Path, listing: &Listing, metas: &[super::meta::Meta], start: usize, ms: f64) -> String {
        if metas.is_empty() {
            return super::rows::empty_line(start, ms);
        }
        super::rows::rows_line(base, listing, metas, start, ms, &self.mime, &self.icons, &self.aliases, &self.thumbs, &mut self.kinds.borrow_mut())
    }

    pub fn load() -> Tables {
        let aliases = Lazy::new(|| Arc::new(Aliases::load()));
        let shared = aliases.clone();
        let thumbs = Lazy::new(move || Arc::new(Thumbnailers::load(&shared)));
        let mime = Lazy::new(|| Arc::new(Db::load()));
        let icons = Lazy::new(|| Arc::new(Names::load()));
        let formats = Lazy::new(|| Arc::new(Formats::probe()));
        Tables { mime, icons, aliases, thumbs, kinds: RefCell::new(Kinds::new()), formats }
    }
}

impl State {
    // Every window clamps against the full known result count without adopting its held remainder.
    pub(crate) fn window_start(&self, start: usize) -> usize {
        let held = self.search_session.as_ref().and_then(|session| session.pending_rest.as_ref()).map_or(0, |rest| rest.len());
        start.min(self.listing.len().saturating_add(held))
    }

    // One below FIRST_LISTING, so the forget_rows a first list runs numbers its rows FIRST_LISTING.
    pub fn new(dirsize_worker: super::dirsizeworker::Worker) -> State {
        State {
            outline: Default::default(),
            contents: Default::default(),
            git: None,
            listing: Listing::new(), base: PathBuf::new(), asked: Vec::new(), outstanding: 0, deferred: Default::default(),
            trash_asked: Vec::new(),
            thumb_mtimes: HashMap::new(),
            thumb_replies: Default::default(),
            window_meta: HashMap::new(),
            dirsizes: HashMap::new(), dirsize_queue: Vec::new(), dirsize_worker,
            search: None, search_reported: Instant::now(), generation: FIRST_LISTING - 1, held: Held::List,
            search_session: None,
            clip_watching: false,
            nearby: Default::default(),
        }
    }
}

