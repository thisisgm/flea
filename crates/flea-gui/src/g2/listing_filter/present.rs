//! What the filter shows, translated once per change and never in paint: the strip, the status, the no-match headline and the washed runs.
use super::filter::Filter;
use flea::i18n::I18n;
use flea::ui_logic_listing_filter::{Counts, Matcher, Refusal, Run, Scope};
use gpui::SharedString;
use std::collections::VecDeque;
use std::rc::Rc;

/// The strip's words, as FilterStrip.qml and the filter boards set them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StripText {
    pub query: SharedString,
    pub caret: bool,
    pub summary: SharedString,
    pub way_out: SharedString,
}

fn count(i18n: &I18n, value: usize) -> String {
    i18n.locale()
        .format_count(i64::try_from(value).unwrap_or(i64::MAX))
}

/// The strip's sentence: shown of tested, the subfolder scope naming its folder, close matches counted apart.
pub fn summary(counts: &Counts, folder: &str, i18n: &I18n) -> String {
    let (shown, tested) = (count(i18n, counts.shown), count(i18n, counts.tested));
    let close = i64::try_from(counts.close).unwrap_or(i64::MAX);
    match (counts.scope, counts.close) {
        (Scope::Folder, 0) => i18n.tr_args("filter.shown", &[&shown, &tested]),
        (Scope::Folder, _) => i18n.trn_args("filter.shown_close", close, &[&shown, &tested]),
        (Scope::Subfolders, 0) => {
            i18n.tr_args("filter.shown_subfolders", &[&shown, &tested, folder])
        }
        (Scope::Subfolders, _) => i18n.trn_args(
            "filter.shown_subfolders_close",
            close,
            &[&shown, &tested, folder],
        ),
    }
}

/// The status bar's count, the strip's sentence without its scope, which the strip above already names.
pub fn status(counts: &Counts, i18n: &I18n) -> String {
    summary(
        &Counts {
            scope: Scope::Folder,
            ..*counts
        },
        "",
        i18n,
    )
}

/// Search's no-match headline with the query as typed, drawn only when nothing exact and nothing close stands.
pub fn no_match(filter: &Filter, listing: usize, i18n: &I18n) -> Option<String> {
    no_match_query(filter, listing).map(|query| nothing_matches(i18n, query))
}

/// The query the no-match headline names: the one whose answer is drawn, so a key still out never relabels the old rows.
pub fn no_match_query(filter: &Filter, listing: usize) -> Option<&str> {
    let drawn = filter.drawn(listing)?;
    let counts = filter.counts(listing)?;
    counts.nothing().then_some(drawn.query.as_str())
}

/// The filter's no-match headline around a query, which the pane may have elided to fit.
pub fn nothing_matches(i18n: &I18n, query: &str) -> String {
    i18n.tr_args("filter.nothing_matches", &[query])
}

pub fn strip(filter: &Filter, listing: usize, folder: &str, i18n: &I18n) -> StripText {
    StripText {
        query: filter.query().to_owned().into(),
        caret: filter.typing(),
        summary: filter
            .counts(listing)
            .map(|counts| summary(&counts, folder, i18n))
            .unwrap_or_default()
            .into(),
        way_out: i18n.tr("filter.esc_clears").into(),
    }
}

/// The sentence an action that found no target says.
pub fn refusal(refusal: Refusal, i18n: &I18n) -> String {
    i18n.tr(match refusal {
        Refusal::NothingToActOn => "filter.nothing_to_act_on",
        Refusal::NotLoaded => "navigation.row_not_loaded",
        Refusal::HiddenByFilter => "filter.row_hidden",
    })
}

/// How a row draws its typed run: washed at rest, bold in the cursor row, and not at all on a marked row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStyle {
    Plain,
    Wash,
    Bold,
}

pub fn run_style(cursor: bool, marked: bool) -> RunStyle {
    match (marked, cursor) {
        (true, _) => RunStyle::Plain,
        (false, true) => RunStyle::Bold,
        (false, false) => RunStyle::Wash,
    }
}

/// Char runs turned into byte ranges of the name, the form a text run is cut by.
pub fn byte_ranges(name: &str, runs: &[Run]) -> Vec<std::ops::Range<usize>> {
    let offsets: Vec<usize> = name
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(name.len()))
        .collect();
    runs.iter()
        .filter(|run| run.len > 0 && run.start + run.len < offsets.len())
        .map(|run| offsets[run.start]..offsets[run.start + run.len])
        .collect()
}

/// Washed runs for the rows on screen, bounded to the held window and dropped whole when the query or numbering moves.
pub struct RunCache {
    key: (String, usize),
    capacity: usize,
    matcher: Option<Matcher>,
    order: VecDeque<usize>,
    runs: std::collections::HashMap<usize, Rc<[Run]>>,
}

impl RunCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            key: (String::new(), 0),
            capacity: capacity.max(1),
            matcher: None,
            order: VecDeque::new(),
            runs: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// The runs for one drawn row; `close` asks the close matcher, as a row drawn from the close list must.
    pub fn get(
        &mut self,
        query: &str,
        listing: usize,
        row: usize,
        name: &str,
        close: bool,
    ) -> Rc<[Run]> {
        if self.key.0 != query || self.key.1 != listing {
            self.key = (query.to_owned(), listing);
            self.matcher = Matcher::new(query);
            self.order.clear();
            self.runs.clear();
        }
        if let Some(found) = self.runs.get(&row) {
            return Rc::clone(found);
        }
        let runs: Rc<[Run]> = match &mut self.matcher {
            None => Rc::from([]),
            Some(matcher) if close => matcher
                .close(name)
                .map_or_else(|| Rc::from([]), |found| Rc::from(found.runs)),
            Some(matcher) => matcher
                .exact(name)
                .map_or_else(|| Rc::from([]), |run| Rc::from([run])),
        };
        if self.order.len() == self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.runs.remove(&oldest);
            }
        }
        self.order.push_back(row);
        self.runs.insert(row, Rc::clone(&runs));
        runs
    }
}
