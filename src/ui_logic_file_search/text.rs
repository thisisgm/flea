// The strip's and status bar's words for a search, each a whole catalog template under the search. prefix.
use super::matching::tilde;
use super::types::{Kind, Scope};
use crate::i18n::{substitute, I18n};

// The strip's count beside the scope: the count with its crawl while the walk runs, alone once landed, nothing for an empty query.
pub fn note(i18n: &I18n, total: usize, running: bool, empty: bool, shares_skipped: bool) -> String {
    if empty {
        return String::new();
    }
    let key = match (running, shares_skipped) {
        (true, false) => "search.note.walking",
        (true, true) => "search.note.walking_shares",
        (false, false) => "search.note.landed",
        (false, true) => "search.note.landed_shares",
    };
    i18n.trn(key, total as i64)
}

// The strip's right edge: while the walk runs esc stops it first, any other esc leaves.
pub fn way_out(i18n: &I18n, running: bool, empty: bool) -> String {
    i18n.tr(if running && !empty { "search.way.walking" } else { "search.way.done" })
}

// The status bar's search half; a landed search with no match says nothing, since the bar already counts 0 items.
pub fn status_line(i18n: &I18n, running: bool, total: usize, scanned: usize, ms: f64) -> String {
    if running {
        let scanned = i18n.locale().format_count(scanned as i64);
        return i18n.trn_args("search.status.searching", total as i64, &[&scanned]);
    }
    if total == 0 {
        return String::new();
    }
    let seconds = i18n.locale().format_number(ms / 1000., 1);
    i18n.trn_args("search.status.landed", total as i64, &[&seconds])
}

// The scope words the strip names, in /fx/v/work, in ~ or everywhere, with contents leading in contents mode.
pub fn scope_label(i18n: &I18n, root: Option<&str>, home: &str, scope: Scope, kind: Kind) -> String {
    let place = root.map(|root| tilde(root, home));
    match (kind, scope, place) {
        (Kind::Names, Scope::Everywhere, _) | (Kind::Names, _, None) => i18n.tr("search.scope.everywhere"),
        (Kind::Contents, Scope::Everywhere, _) | (Kind::Contents, _, None) => i18n.tr("search.scope.contents_everywhere"),
        (Kind::Names, _, Some(place)) => i18n.tr_args("search.scope.in", &[&place]),
        (Kind::Contents, _, Some(place)) => i18n.tr_args("search.scope.contents_in", &[&place]),
    }
}

// The no-match headline: the template in Omarchy's capitals, the query as typed since names are case-sensitive.
pub fn nothing_matches(i18n: &I18n, answered: &str) -> String {
    if answered.is_empty() {
        return String::new();
    }
    let template = i18n.tr("search.nothing_matches");
    let capitals: Vec<String> = template.split("%1").map(str::to_uppercase).collect();
    substitute(&capitals.join("%1"), &[answered])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Drawn,
    Blank,
    Loading,
}

// One frame's kind for the no-blank counter: a listing out drawn loading, or a live count beside no drawn row, is blank.
pub fn frame_kind(in_flight: bool, loading: bool, fell_back: bool, drawn: usize, named: usize, live: bool) -> Frame {
    if in_flight && loading {
        return if fell_back { Frame::Loading } else { Frame::Blank };
    }
    if live && drawn == 0 && named > 0 {
        return Frame::Blank;
    }
    Frame::Drawn
}
