//! tests/js/columnskeep.js: every peek names the drawn columns, a returning column is asked again.
use super::check::Checks;
use super::fixture::{row, Pane, Producers, SETTINGS};
use flea::jsondoc::Json;
use flea_gui::columns::geometry::{keep_asks, neighbour_asks};
use flea_gui::columns::peeks::{peek_key, Facts, PeekRow, Peeked};
use flea_gui::columns::Area;
use std::rc::Rc;

const SOURCE: &str = "tests/js/columnskeep.js";
const WIDTH: f32 = 1300.;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

fn plan(list: &[(&str, bool)]) -> Vec<(String, bool)> {
    list.iter()
        .map(|(path, again)| ((*path).to_owned(), *again))
        .collect()
}

/// One peek line as the wire carries it: the path, the watch flag and the keep set.
fn wire(line: &str) -> (String, Option<bool>, Option<Vec<String>>) {
    let doc = flea::jsondoc::parse(line).expect("peek line is JSON");
    let path = doc
        .get("path")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_owned();
    let watch = doc.get("watch").and_then(Json::as_bool);
    let keep = doc.get("keep").and_then(Json::as_array).map(|keep| {
        keep.iter()
            .filter_map(|one| one.as_str().map(str::to_owned))
            .collect()
    });
    (path, watch, keep)
}

/// A columns view at /a/b over child folders c and d, and a stub backend that holds its replies.
struct View {
    area: Area,
    pane: Pane,
    producers: Producers,
    lines: Vec<String>,
    sent: Vec<String>,
    held: Vec<String>,
    replies: Vec<(String, Vec<PeekRow>)>,
    now: u64,
}

impl View {
    fn new() -> Self {
        let mut pane = Pane::at("/a/b", vec![row("c", true), row("d", true)]);
        pane.first = 35;
        Self {
            area: Area::new(WIDTH, 800., None, 0., SETTINGS),
            pane,
            producers: Producers::default(),
            lines: Vec::new(),
            sent: Vec::new(),
            held: Vec::new(),
            replies: Vec::new(),
            now: 0,
        }
    }

    /// The backend's rule: a watching peek keeps only the drawn folders, then arms its own.
    fn take(&mut self) {
        let lines = std::mem::take(&mut self.area.outbox);
        for line in lines {
            let (path, _, keep) = wire(&line);
            let keep = keep.unwrap_or_default();
            if !keep.is_empty() {
                self.held.retain(|one| keep.contains(one));
            }
            if keep.contains(&path) && !self.held.contains(&path) {
                self.held.push(path.clone());
            }
            let scan = PeekRow {
                name: format!("scan{}", self.sent.len()),
                dir: false,
                icon: String::new(),
            };
            self.replies.push((path.clone(), vec![scan]));
            self.sent.push(path);
            self.lines.push(line);
        }
    }

    /// Moves the cursor to a child folder and runs the pass the view runs for that pane change.
    fn cursor_on(&mut self, name: &str) {
        self.pane.cursor = self
            .pane
            .rows
            .iter()
            .position(|one| one.name == name)
            .expect("a child row");
        self.now += 1;
        let Self {
            area,
            pane,
            producers,
            now,
            ..
        } = &mut *self;
        pane.with(|view| area.sync(&view, &mut producers.ports(), *now));
        self.take();
    }

    /// Delivers the held replies in order, keeping back the ones for one path.
    fn deliver(&mut self, skip: &str) {
        for (path, rows) in std::mem::take(&mut self.replies) {
            if path == skip {
                self.replies.push((path, rows));
                continue;
            }
            let reply = Peeked {
                path,
                hidden: false,
                hidden_last: false,
                first: 35,
                order: String::new(),
                total: rows.len(),
                facts: Facts {
                    dev: 0,
                    writable: true,
                },
                denied: None,
                rows: Rc::from(rows),
            };
            self.now += 1;
            let Self {
                area,
                pane,
                producers,
                now,
                ..
            } = &mut *self;
            pane.with(|view| area.peeked(&reply, &view, &mut producers.ports(), *now));
            self.take();
        }
    }

    fn rows_of(&self, path: &str) -> String {
        self.area.peeks.names(&peek_key(path, false, false))
    }

    fn asked(&self, path: &str) -> usize {
        self.sent.iter().filter(|one| *one == path).count()
    }
}

#[test]
fn keep_asks_names_the_drawn_columns() {
    let mut c = Checks::new(SOURCE);
    let mut drawn: Vec<String> = Vec::new();
    let ancestors = paths(&["/a", "/a/b"]);
    c.eq(
        8,
        "a first refresh asks every column, ancestors then the child",
        keep_asks(&mut drawn, &ancestors, "/a/b/c"),
        plan(&[("/a", true), ("/a/b", true), ("/a/b/c", true)]),
    );
    c.eq(
        9,
        "and draws exactly those",
        drawn.clone(),
        paths(&["/a", "/a/b", "/a/b/c"]),
    );
    c.eq(
        10,
        "a column still drawn is asked as before",
        keep_asks(&mut drawn, &ancestors, "/a/b/d"),
        plan(&[("/a", false), ("/a/b", false), ("/a/b/d", true)]),
    );
    c.eq(
        11,
        "the child that left is no longer drawn",
        drawn.clone(),
        paths(&["/a", "/a/b", "/a/b/d"]),
    );
    c.eq(
        12,
        "a file under the cursor draws no child column",
        keep_asks(&mut drawn, &ancestors, ""),
        plan(&[("/a", false), ("/a/b", false)]),
    );
    c.eq(
        13,
        "a column that scrolled away and returns is asked again",
        keep_asks(&mut drawn, &ancestors, "/a/b/d"),
        plan(&[("/a", false), ("/a/b", false), ("/a/b/d", true)]),
    );
    c.eq(
        14,
        "a narrow window with no folder under the cursor draws nothing",
        keep_asks(&mut Vec::new(), &[], "").len(),
        0,
    );
    c.eq(
        15,
        "a child that is also an ancestor is named once",
        keep_asks(&mut Vec::new(), &paths(&["/a"]), "/a").len(),
        1,
    );
    c.finish();
}

#[test]
fn a_refresh_asks_the_keep_plan_and_every_peek_names_the_drawn_set() {
    let mut c = Checks::new(SOURCE);
    let mut view = View::new();
    view.cursor_on("c");
    let keep_plan = keep_asks(
        &mut Vec::new(),
        &neighbour_asks("/a/b", WIDTH, None),
        "/a/b/c",
    );
    let again: Vec<String> = keep_plan
        .iter()
        .filter(|(_, again)| *again)
        .map(|(path, _)| path.clone())
        .collect();
    let drawn: Vec<String> = keep_plan.iter().map(|(path, _)| path.clone()).collect();
    c.ok(
        18,
        "refresh asks from the keep plan",
        view.sent == again && view.area.drawn == drawn,
    );
    let wires: Vec<_> = view.lines.iter().map(|line| wire(line)).collect();
    c.ok(
        20,
        "every column peek carries the drawn set",
        !wires.is_empty()
            && wires
                .iter()
                .all(|(_, _, keep)| keep.as_ref() == Some(&view.area.drawn)),
    );
    c.ok(
        22,
        "the wire carries it as keep",
        wires
            .iter()
            .all(|(_, watch, keep)| *watch == Some(true) && keep.is_some()),
    );
    c.finish();
}

/// A column that leaves and returns before its first reply ends watched, and both replies land.
#[test]
fn a_column_that_comes_back_is_asked_again_and_watched() {
    let mut c = Checks::new(SOURCE);
    c.covers(&[23]);
    let (cc, d) = ("/a/b/c", "/a/b/d");
    let mut view = View::new();
    view.cursor_on("c");
    view.deliver(cc);
    view.cursor_on("d");
    view.cursor_on("c");
    c.eq(
        78,
        "the returning column is asked again with its first ask still out",
        view.asked(cc),
        2,
    );
    c.ok(
        79,
        "the backend then watches the column that came back",
        view.held.iter().any(|one| one == cc),
    );
    c.ok(
        80,
        "and no longer the one that left",
        !view.held.iter().any(|one| one == d),
    );
    view.deliver("");
    c.ok(
        82,
        "both replies for the returning column are stored",
        !view.rows_of(cc).is_empty(),
    );
    let last = format!("scan{}", view.sent.len() - 1);
    c.ok(
        83,
        "the later reply is the one kept",
        view.rows_of(cc).starts_with("scan") && view.rows_of(cc) == last,
    );
    let before = view.sent.clone();
    {
        let View {
            area,
            pane,
            producers,
            ..
        } = &mut view;
        pane.with(|snapshot| area.refresh(&snapshot, &mut producers.ports()));
    }
    view.take();
    c.ok(
        84,
        "a refresh with nothing moved asks nothing more",
        view.sent == before,
    );
    let mut calm = View::new();
    calm.cursor_on("c");
    calm.deliver("");
    calm.cursor_on("d");
    calm.deliver("");
    calm.cursor_on("c");
    c.eq(
        91,
        "a column that returns after its reply is asked once more",
        calm.asked(cc),
        2,
    );
    c.ok(92, "and watched", calm.held.iter().any(|one| one == cc));
    // The store writes an order only with its rows, so no rows held is the case named.
    let mut bare = View::new();
    {
        let View { area, pane, .. } = &mut bare;
        pane.with(|view| area.ask(&view.lens, cc, false, false));
    }
    bare.take();
    c.eq(
        97,
        "a column with no rows held asks whatever order it last landed under",
        bare.sent.clone(),
        paths(&[cc]),
    );
    c.finish();
}
