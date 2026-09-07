// Everything the read loop holds: the tables it parses once, and the listing state it mutates.
use crate::backend::aliases::Aliases;
use crate::backend::archive::Formats;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::mime::Db;
use crate::backend::search::Search;
use crate::backend::reclaim::Reclaim;
use crate::backend::thumbspec::Thumbnailers;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

// Read once for the process: a per-window load would put a file read inside the viewport path.
pub struct Tables {
    pub mime: Db,
    pub icons: Names,
    pub aliases: Arc<Aliases>,
    pub thumbs: Arc<Thumbnailers>,
    // A type's Kind text never changes for the process's life, unlike State's per-listing caches; RefCell because the loop is single-threaded.
    pub kinds: RefCell<Kinds>,
    // Probed once at startup, the same discipline thumbspec.rs applies to a thumbnailer's program.
    pub formats: Arc<Formats>,
}

// Everything the loop mutates, gathered so a handler takes one borrow instead of ten arguments.
pub struct State {
    pub listing: Listing,
    pub base: PathBuf,
    // Only the rows a client named, so this never grows with the directory; see AGENTS.md "Thumbnail requests".
    pub asked: Vec<(PathBuf, usize)>,
    pub outstanding: usize,
    // Answered directory rows, kept until the next list or sort reassigns what a row index names.
    pub dirsizes: HashMap<usize, (u64, bool)>,
    // Rows still to walk, one at a time; dirsizecancel empties this without touching dirsizes.
    pub dirsize_queue: Vec<usize>,
    // The subtree walk the loop ticks; None means no search is running.
    pub search: Option<Search>,
    // When the running walk last announced its count, so SEARCH_REPORT can throttle the stream.
    pub search_reported: Instant,
    // The reclaim walk the loop ticks; None means no reclaim is running. At most one of the two
    // walks exists: each request ends the other before it touches the listing.
    pub reclaim: Option<Reclaim>,
    // When the running reclaim last announced its progress, throttled the same way a search's is.
    pub reclaim_reported: Instant,
    // True from a reclaim's terminal line until the next list or search: a window's directory rows
    // then report the walk's measured bytes in s, the one reading every view of the scan needs.
    pub reclaim_sizes: bool,
}

