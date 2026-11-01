//! Main-window Recent uses the existing metadata slot as fixed Location, and dates come from visits.
use super::{columns, ListState, ListView, Tokens};
use flea::ui_logic_pl4::{recent, recentmode::Visits};

// Theme.qml location in v0.3.12: Recent's fixed Location column, shared by the header and the rows.
const LOCATION_WIDTH: f32 = 150.;

// A Recent row names its file by absolute path; the visits key it with one leading slash.
fn visit_path(name: &str) -> String {
    format!("/{}", name.trim_start_matches('/'))
}

pub(crate) struct PlaceRecent {
    visits: Visits,
    home: String,
}

impl ListView {
    pub(crate) fn recent_location(&self, name: &str) -> String {
        self.place_recent
            .as_ref()
            .map_or_else(String::new, |recent| {
                recent::location_under(&visit_path(name), &recent.home)
            })
    }

    pub(crate) fn date_stamp(&self, facts: &flea::protocol::RowFacts) -> Option<i64> {
        match &self.place_recent {
            Some(recent) => recent
                .visits
                .get(&visit_path(&facts.name))
                .copied()
                .flatten(),
            None => Some(facts.mtime),
        }
    }
}

impl ListState {
    pub(crate) fn set_recent_place(&mut self, visits: Option<Visits>) {
        if visits.is_none() && self.view.place_recent.is_none() {
            return;
        }
        self.view.place_recent = visits.map(|visits| PlaceRecent {
            visits,
            home: std::env::var("HOME").unwrap_or_default(),
        });
        self.view.text += 1;
        self.invalidate_text();
        self.header.forget_titles();
        self.header.drag.cancel();
        self.header.hot = None;
    }
}

pub(super) fn layout(
    width: f32,
    tokens: &Tokens,
    switched: columns::Switched,
    stored: columns::StoredWidths,
    dual: bool,
) -> columns::Layout {
    use columns::Column;
    let hidden: &[&str] = match (switched.on(Column::Size), switched.on(Column::Date)) {
        (true, true) => &[],
        (true, false) => &["date"],
        (false, true) => &["size"],
        (false, false) => &["size", "date"],
    };
    let size = tokens.wanted(Column::Size, stored);
    let date = tokens.wanted(Column::Date, stored);
    let set = crate::columns::sets::recent_set(
        width,
        &crate::columns::sets::Tokens {
            row_padding_x: tokens.pad,
            icon_size: tokens.icon,
            gap: tokens.gap,
            name_min: tokens.name_min,
            mode: 0.,
            size,
            date,
            kind: 0.,
            location: LOCATION_WIDTH,
        },
        hidden,
        dual,
    );
    let mut geometry = *tokens;
    // Row.qml and Header.qml drop the metadata gaps in dual mode, as recentSet already fits them.
    if dual {
        geometry.gap = 0.;
    }
    let mut layout = columns::chain(
        width,
        &geometry,
        [set.location, set.size, set.date, false],
        [LOCATION_WIDTH, size, date, 0.],
    );
    layout.recent_location = true;
    layout
}
