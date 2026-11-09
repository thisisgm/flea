// Folder sizes a live search answer carries in its rows line, from the session's cache and a walk under a tight budget.
use crate::backend::dirsize::{walk_until, DirSize};
use crate::backend::searchsession::Session;
use crate::backend::state::State;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

// The whole inline walk for one answer stays under this, so it never holds the answer's rows.
pub const BUDGET_MS: u64 = 3;

// What one session knows about folder sizes, and whether its scope may be walked inline at all.
pub struct AnswerSizes {
    // How many leading rows the client draws a Size cell for; zero asks for none.
    pub wish: usize,
    // A remote scope, or one whose walk once missed the budget, answers from the cache alone.
    walkable: bool,
    budget: Duration,
    // The folder walk itself; a test swaps in one that blocks, to show the answer never waits past the budget.
    walk: fn(&Path, Instant) -> DirSize,
    // Complete sizes by full path, learned from inline walks and from the client's own dirsize asks.
    known: HashMap<PathBuf, u64>,
    // Folder walks the last answer ran, the count the budget is proven by.
    pub walks: usize,
}

impl AnswerSizes {
    pub fn new(remote: bool) -> AnswerSizes {
        AnswerSizes { wish: 0, walkable: !remote, budget: Duration::from_millis(BUDGET_MS), walk: walk_until, known: HashMap::new(), walks: 0 }
    }

    // A write under the scope can change the size of every folder above it, so the session forgets them all.
    pub fn forget(&mut self) {
        self.known.clear();
    }
}

#[cfg(test)]
impl AnswerSizes {
    // A budget no loaded box misses, for a test of what an answer carries rather than when.
    pub(crate) fn unhurried(&mut self) {
        self.budget = Duration::from_secs(30);
    }
}

impl Session {
    // A query that names no wish keeps the one before it, so a burst the loop re-renders without it still carries sizes.
    pub(crate) fn wish_sizes(&mut self, rows: Option<usize>) -> &mut Session {
        if let Some(rows) = rows {
            self.sizes.wish = rows;
        }
        self
    }
}

// Fills st.dirsizes for the answer's leading folder rows from `start`: known paths at once, the rest walked until the budget runs out.
pub fn fill(st: &mut State, start: usize, count: usize) {
    let Some(session) = st.search_session.as_mut() else { return };
    let sizes = &mut session.sizes;
    sizes.walks = 0;
    let mut unknown = Vec::new();
    for row in start..(start + sizes.wish.min(count)).min(st.listing.len()) {
        if !st.listing.is_dir(row) {
            continue;
        }
        let path = st.listing.path(&st.base, row);
        match sizes.known.get(&path) {
            Some(&bytes) => {
                st.dirsizes.insert(row, (bytes, false));
            }
            None => unknown.push((row, path)),
        }
    }
    if unknown.is_empty() || !sizes.walkable {
        return;
    }
    let deadline = Instant::now() + sizes.budget;
    let (tx, rx) = channel::<(usize, PathBuf, DirSize)>();
    let wanted = unknown.len();
    let walk = sizes.walk;
    let begun = Arc::new(AtomicBool::new(false));
    let began = Arc::clone(&begun);
    // Its own thread, so a stat that blocks past the deadline holds that thread and never the read loop.
    let spawned = std::thread::Builder::new().name("answersizes".into()).spawn(move || {
        began.store(true, Ordering::Release);
        for (row, path) in unknown {
            let size = walk(&path, deadline);
            if tx.send((row, path, size)).is_err() || size.partial {
                return;
            }
        }
    });
    if spawned.is_err() {
        return;
    }
    let mut whole = 0;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok((row, path, size)) = rx.recv_timeout(left) else { break };
        sizes.walks += 1;
        if size.partial {
            break;
        }
        whole += 1;
        st.dirsizes.insert(row, (size.bytes, false));
        sizes.known.insert(path, size.bytes);
    }
    // A walk the scheduler never started before the deadline says nothing about the scope, so only a started one marks it slow.
    if whole < wanted && begun.load(Ordering::Acquire) {
        sizes.walkable = false;
    }
}

// A complete size the client asked for joins the cache, so the next answer that lists this folder carries it.
pub fn learn(st: &mut State, row: usize, size: DirSize) {
    if size.partial || row >= st.listing.len() {
        return;
    }
    if let Some(session) = st.search_session.as_mut() {
        session
            .sizes
            .known
            .insert(st.listing.path(&st.base, row), size.bytes);
    }
}

// Sample input: ({"t":"rows",...,"gen":3}, {0: (40, false), 2: (9, true)}, 0..3) answers {"t":"rows",...,"gen":3,"z":[[0,40]]}.
pub fn with_sizes(mut line: String, dirsizes: &std::collections::HashMap<usize, (u64, bool)>, rows: Range<usize>) -> String {
    let mut pairs: Vec<(usize, u64)> = dirsizes
        .iter()
        .filter(|(row, (_, partial))| rows.contains(row) && !partial)
        .map(|(&row, &(bytes, _))| (row, bytes))
        .collect();
    if pairs.is_empty() || !line.ends_with('}') {
        return line;
    }
    pairs.sort_unstable();
    let z: Vec<String> = pairs.iter().map(|(row, bytes)| format!("[{},{}]", row, bytes)).collect();
    line.insert_str(line.len() - 1, &format!(",\"z\":[{}]", z.join(",")));
    line
}

#[cfg(test)]
#[path = "answersizes_tests.rs"]
mod tests;
