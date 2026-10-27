// Everything the read loop holds: the tables it parses once, and the listing state it mutates.
use crate::backend::aliases::Aliases;
use crate::backend::archive::Formats;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
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

// Read once for the process: a per-window load would put a file read inside the viewport path.
pub struct Tables {
    pub mime: Arc<Db>,
    pub icons: Arc<Names>,
    pub aliases: Arc<Aliases>,
    pub thumbs: Arc<Thumbnailers>,
    // A type's Kind text never changes for the process's life, unlike State's per-listing caches; RefCell because the loop is single-threaded.
    pub kinds: RefCell<Kinds>,
    // Probed once at startup, the same discipline thumbspec.rs applies to a thumbnailer's program.
    pub formats: Arc<Formats>,
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
    pub listing: Listing,
    pub base: PathBuf,
    // Only the rows a client named, so this never grows with the directory; see AGENTS.md "Thumbnail requests".
    pub asked: Vec<(PathBuf, usize)>,
    pub outstanding: usize,
    // Rows the pool has no room for yet, in request order; each is also in asked, so its answer still follows its row.
    pub deferred: crate::backend::thumbdefer::Deferred,
    // The last window's mode, mtime and link-dir flag per row, so a thumb request reuses them with no stat.
    pub window_meta: HashMap<usize, (u32, i64, bool)>,
    // Answered directory rows, kept until the next list or sort reassigns what a row index names.
    pub dirsizes: HashMap<usize, (u64, bool)>,
    // Rows still to walk, one at a time; dirsizecancel empties this without touching dirsizes.
    pub dirsize_queue: Vec<usize>,
    pub dirsize_worker: super::dirsizeworker::Worker,
    // The subtree walk the loop ticks; None means no search is running.
    pub search: Option<Search>,
    // The live search session; a list, sort or close drops it and frees its arena.
    pub search_session: Option<Session>,
    // The filter's standing query, issue 315: its matches over this numbering; a filterclose drops it and another numbering rebuilds it.
    pub filter: Option<super::filter::Standing>,
    // When the running walk last announced its count, so SEARCH_REPORT can throttle the stream.
    pub search_reported: Instant,
    // Which numbering the rows are in; forget_rows moves it, see src/backend/rowguard.rs.
    pub generation: u64,
    // The source of the held listing, set by the arm that answered it.
    pub held: Held,
    // The clipboard watcher runs once per backend; a second clipWatch is a no-op.
    pub clip_watching: bool,
}

impl Tables {
    pub fn load() -> Tables {
        let aliases = Arc::new(Aliases::load());
        let thumbs = Arc::new(Thumbnailers::load(&aliases));
        let (mime, icons, formats) = (Arc::new(Db::load()), Arc::new(Names::load()), Arc::new(Formats::probe()));
        Tables { mime, icons, aliases, thumbs, kinds: RefCell::new(Kinds::new()), formats }
    }
}

impl State {
    // One below FIRST_LISTING, so the forget_rows a first list runs numbers its rows FIRST_LISTING.
    pub fn new(dirsize_worker: super::dirsizeworker::Worker) -> State {
        State {
            listing: Listing::new(), base: PathBuf::new(), asked: Vec::new(), outstanding: 0, deferred: Default::default(),
            window_meta: HashMap::new(),
            dirsizes: HashMap::new(), dirsize_queue: Vec::new(), dirsize_worker,
            search: None, search_reported: Instant::now(), generation: FIRST_LISTING - 1, held: Held::List,
            search_session: None,
            filter: None,
            clip_watching: false,
        }
    }
}

