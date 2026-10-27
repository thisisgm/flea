// Test-only measurements from the real Session::query execution path.
use std::time::Duration;

#[derive(Default, Clone, Copy)]
pub(crate) struct Phases {
    pub score: Duration,
    pub prior: Duration,
    // The boundary class, the head partition and the head's deferred DP.
    pub resolve: Duration,
    pub all: Duration,
    pub select: Duration,
    pub cut: Duration,
    pub rows: Duration,
    pub last_matches: Duration,
    pub select_raw: usize,
}
