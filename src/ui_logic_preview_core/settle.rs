// The cursor preview's settle: a move after quiet loads at once, a walk trails, a repeat keeps its timer.

// ui/SelectionPreview.qml and ui/Preview.qml followSettle: a held key settles this long before it loads.
pub const SETTLE_MS: i64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    Same,
    Now,
    Later,
}

// Sample input: due(200, 0, 120) and due(100, 200, 120) are due; due(50, 0, 120) trails.
pub fn due(now: i64, last: i64, interval: i64) -> bool {
    let elapsed = now - last;
    elapsed >= interval || elapsed < 0
}

// Sample input: plan(50, 0, 120, "b", "b", true) keeps the pending timer.
pub fn plan(now: i64, last: i64, interval: i64, key: &str, last_key: &str, armed_for_key: bool) -> Plan {
    if key == last_key && armed_for_key {
        return Plan::Same;
    }
    if due(now, last, interval) {
        Plan::Now
    } else {
        Plan::Later
    }
}

// A PDF opens at once only when the cursor is idle and no page render is in flight.
pub fn pdf_open_now(now: i64, last_request_at: i64, interval: i64, render_loading: bool) -> bool {
    !render_loading && due(now, last_request_at, interval)
}

// A lone selection never keys on the version a plain move bumps.
pub fn selection_key(count: usize, version: u64) -> String {
    if count <= 1 {
        return String::new();
    }
    format!("{count}:{version}")
}
