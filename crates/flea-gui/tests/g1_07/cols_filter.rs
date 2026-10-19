//! tests/columnsfilter.qml: a filter over the active column moves no peek, and the third column follows the rows it keeps.
use super::check::Checks;
use super::fixture::{reply_with, row, Pane, Producers, SETTINGS};
use flea::protocol::RowFacts;
use flea_gui::columns::geometry::Slot;
use flea_gui::columns::peeks::{join_names, peek_key, Landed, Lens};
use flea_gui::columns::preview_host::{Cursor, Shown, ThirdSwap};
use flea_gui::columns::{Area, PaneView, Ports, TrackRows};
use flea_gui::navigation::history::join;
use std::rc::Rc;

pub const BASE: &str = "/probe/studio";

/// tests/columnsfilter-fixture.js buildRows: "ap" keeps rows 0, 2, 4, 6, 8 and 10 of these 12.
pub fn build_rows() -> Vec<RowFacts> {
    let folders = ["Apps", "Clips", "Maps"];
    let files = [
        "balsam.txt",
        "caption.png",
        "drift.txt",
        "gap.md",
        "hold.txt",
        "napkin.txt",
        "orbit.md",
        "paper.pdf",
        "quince.txt",
    ];
    let mut rows: Vec<RowFacts> = folders.iter().map(|name| row(name, true)).collect();
    rows.extend(files.iter().map(|name| row(name, false)));
    rows
}

/// The pane as a filter leaves it: Filter.shown's kept listing rows, None while no query stands.
pub struct Filtered {
    pub pane: Pane,
    pub kept: Option<Vec<usize>>,
}

impl Filtered {
    /// Filter.shown for this query: Match.run's case-insensitive substring over every held row.
    pub fn query(&mut self, query: &str) {
        let needle = query.to_lowercase();
        let kept = if query.is_empty() {
            None
        } else {
            let rows = self.pane.rows.iter().enumerate();
            Some(
                rows.filter(|(_, row)| row.name.to_lowercase().contains(&needle))
                    .map(|(at, _)| at)
                    .collect(),
            )
        };
        self.kept = kept;
    }

    /// Filter.cursorShown: no query shows every row, else the cursor's listing row must be kept.
    pub fn shows_cursor(&self) -> bool {
        let cursor = self.pane.cursor;
        self.kept.as_ref().is_none_or(|kept| kept.contains(&cursor))
    }

    /// One Columns call: a hidden cursor has no row, and is filter-hidden only while a row still stands.
    pub fn with<R>(&self, call: impl FnOnce(PaneView<'_>) -> R) -> R {
        let pane = &self.pane;
        let orders = |path: &str| pane.orders.get(path).copied();
        let shown = self.shows_cursor();
        let lens = Lens {
            here: &pane.here,
            hidden: pane.hidden,
            hidden_last: pane.hidden_last,
            first: pane.first,
            orders: &orders,
        };
        call(PaneView {
            lens,
            base: &pane.base,
            cursor_row: if shown {
                pane.rows.get(pane.cursor)
            } else {
                None
            },
            cursor: pane.cursor,
            listing: pane.listing,
            in_flight: pane.in_flight,
            filter_hid_cursor: !shown && self.kept.as_ref().is_some_and(|kept| !kept.is_empty()),
        })
    }
}

/// The area under test, its producers, the filtered pane, a clock and every change of the third column.
pub struct Columns {
    pub area: Area,
    pub producers: Producers,
    pub pane: Filtered,
    pub now: u64,
    /// One note per change of what the third column draws, in the probe's "row:dir:path" form.
    pub log: Vec<String>,
    last: Shown,
}

impl Columns {
    pub fn new(width: f32, height: f32, pane: Pane) -> Self {
        Self {
            area: Area::new(width, height, None, 0., SETTINGS),
            producers: Producers::default(),
            pane: Filtered { pane, kept: None },
            now: 0,
            log: Vec::new(),
            last: Shown::default(),
        }
    }

    /// The model changes the third column only inside a call, so a note after each call sees every landing.
    fn note(&mut self) {
        let shown = &self.area.host.shown;
        if *shown != self.last {
            self.log.push(format!(
                "{}:{}:{}",
                if shown.has_row { "row" } else { "empty" },
                if shown.is_dir { "dir" } else { "file" },
                shown.child
            ));
            self.last = shown.clone();
        }
    }

    /// One area call over the pane as it stands now.
    pub fn call<R>(
        &mut self,
        step: impl FnOnce(&mut Area, &PaneView<'_>, &mut Ports<'_>, u64) -> R,
    ) -> R {
        let Self {
            area,
            producers,
            pane,
            now,
            ..
        } = self;
        let out = pane.with(|view| step(area, &view, &mut producers.ports(), *now));
        self.note();
        out
    }

    pub fn sync(&mut self) {
        self.call(|area, view, ports, now| area.sync(view, ports, now));
    }

    pub fn tick(&mut self) {
        self.call(|area, view, ports, now| area.tick(view, ports, now));
    }

    pub fn cursor(&mut self, at: usize) {
        self.pane.pane.cursor = at;
        self.sync();
    }

    /// A scripted peeked reply under the default order, the window size echoed as the probe's stub does.
    pub fn answer(&mut self, path: &str, rows: &[(&str, bool)]) -> Landed {
        let reply = reply_with(path, rows, false, false, self.pane.pane.first, "");
        self.call(|area, view, ports, now| area.peeked(&reply, view, ports, now))
    }

    /// The swap's hold releasing as its preview lands: the owner shows the cursor row, then the area syncs.
    pub fn release(&mut self) {
        if self.producers.swap.idle() {
            return;
        }
        self.producers.swap.holding = false;
        self.call(|area, view, ports, now| {
            let child = view
                .cursor_row
                .filter(|row| row.dir)
                .map(|row| join(view.base, &row.name))
                .unwrap_or_default();
            let cursor = Cursor {
                row: view.cursor_row.is_some(),
                is_dir: view.cursor_row.is_some_and(|row| row.dir),
                child: &child,
            };
            let answered = area.peeks.answered(&view.lens.key(&child), &child);
            area.host.show_cursor_row(cursor, answered, false);
            area.sync(view, ports, now);
        });
    }

    /// shownHasRow, shownIsDir and shownChildPath.
    pub fn shown(&self) -> (bool, bool, String) {
        let shown = &self.area.host.shown;
        (shown.has_row, shown.is_dir, shown.child.clone())
    }

    /// Neither capturing nor holding, swapState()'s two flags.
    pub fn idle(&self) -> bool {
        self.producers.swap.idle()
    }

    pub fn blanks(&self) -> Vec<String> {
        let blank = |note: &&String| note.starts_with("empty:");
        self.log.iter().filter(blank).cloned().collect()
    }

    /// peekNames: rowsFor's names, the landing's stand-in first, else the rows the store holds.
    pub fn names(&self, path: &str) -> String {
        let pane = &self.pane.pane;
        let key = peek_key(path, pane.hidden, pane.hidden_last);
        let held = self
            .area
            .peeks
            .rows(&key)
            .cloned()
            .unwrap_or_else(|| Rc::from([]));
        join_names(&self.area.landing.rows(path, &pane.here, held))
    }

    /// One key that sets the query, with the stub's refresh behind it.
    pub fn key(&mut self, query: &str) {
        self.pane.query(query);
        self.sync();
    }
}

#[test]
fn a_filter_moves_no_peek_and_the_third_column_follows_the_rows_it_keeps() {
    let mut c = Checks::new("tests/columnsfilter.qml");
    // The stage timers, the failure list and the report are this test's own steps and finish.
    c.covers(&[
        202, 203, 204, 205, 206, 207, 208, 210, 231, 244, 284, 297, 311, 336, 346, 393, 400,
    ]);
    let mut pane = Pane::at(BASE, build_rows());
    pane.cursor = 4;
    pane.first = 35;
    // The area spans half of a 1500 px window plus 200 px, under the filter strip.
    let mut cols = Columns::new(950., 640., pane);
    cols.sync();

    // measureRest at 800 ms: three columns, and the parent's scripted reply lands.
    cols.now = 800;
    let parent_drawn = cols
        .area
        .tracks()
        .iter()
        .any(|track| track.slot == Slot::Parent);
    c.ok(
        230,
        "rest draws no parent column to hold still",
        parent_drawn,
    );
    cols.answer(
        "/probe",
        &[("studio", true), ("field", true), ("notes.txt", false)],
    );
    let parent_before = cols.names("/probe");
    c.eq(
        235,
        "rest answers no parent peek to hold still",
        parent_before.clone(),
        "studio|field|notes.txt".to_owned(),
    );
    // "/" opens the strip, then "a" and "p" keep caption.png under the cursor.
    for query in ["", "a", "ap"] {
        cols.key(query);
    }

    // measureCommit at 1400 ms: Return keeps the query standing over the same rows.
    cols.now = 1400;
    cols.sync();
    c.eq(
        279,
        "enter moved the parent column",
        cols.names("/probe"),
        parent_before.clone(),
    );

    // measureWalk at 2100 ms: j to gap.md and k back to caption.png, each a file the swap holds for.
    cols.now = 2100;
    cols.cursor(6);
    cols.cursor(4);
    // measurePreview at 2800 ms: the settle landed, then j to gap.md.
    cols.now = 2800;
    cols.release();
    cols.cursor(6);
    // measureEscape at 3600 ms: the settle landed, then Escape restores every row.
    cols.now = 3600;
    cols.release();
    cols.key("");

    // measureNavPrep at 4200 ms: ap typed again, then Nav.forget empties the listing and its rows land again.
    cols.now = 4200;
    for query in ["a", "ap"] {
        cols.key(query);
    }
    cols.pane.query("");
    cols.pane.pane.rows = Vec::new();
    cols.pane.pane.cursor = 0;
    cols.pane.pane.listing += 1;
    cols.sync();
    cols.pane.pane.rows = build_rows();
    cols.pane.pane.listing += 1;
    cols.sync();

    // measureNav at 4800 ms: the folder fallback for the unanswered Apps came due.
    cols.now = 4800;
    cols.tick();
    c.eq(
        369,
        "navigated moved the parent column",
        cols.names("/probe"),
        parent_before,
    );
    // "/" then "zzz": nothing matches, so the third column has no row to move to.
    for query in ["", "z", "zz", "zzz"] {
        cols.key(query);
    }
    let (has_row, _, child) = cols.shown();
    c.eq(
        375,
        "nomatch holds the child path, want empty",
        (has_row, child),
        (false, String::new()),
    );
    // Escape stands on Apps again, and its empty reply lands the Apps column.
    cols.key("");
    cols.answer("/probe/studio/Apps", &[]);
    let (has_row, _, child) = cols.shown();
    let apps = (true, "/probe/studio/Apps".to_owned());
    c.eq(
        379,
        "esc holds the child path, want the Apps column",
        (has_row, child),
        apps,
    );
    for query in ["", "a", "ap", "apz"] {
        cols.key(query);
    }
    c.eq(385, "apz holds a row", cols.shown().0, false);
    // Backspace back to ap: Apps is answered, so its column shows at once over the pane's own rows.
    cols.key("ap");
    let active = cols
        .area
        .tracks()
        .iter()
        .find(|track| track.slot == Slot::Active);
    let active = active.map(|track| track.rows.clone());
    c.eq(
        388,
        "backspace holds the rows, want 6 with a column",
        (cols.shown().0, active),
        (true, Some(TrackRows::Listing)),
    );
    c.finish();
}
