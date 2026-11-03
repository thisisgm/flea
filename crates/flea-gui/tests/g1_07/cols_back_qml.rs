//! tests/columnsback.qml: 31 Back landings over a serial backend, each asking only its new parent and drawn whole at once.
use super::check::Checks;
use super::cols_moves::{Dirs, Sim};
use super::fixture::{row, Pane, SETTINGS};
use flea_gui::columns::{geometry::Slot, Area, TrackRows};

const SOURCE: &str = "tests/columnsback.qml";
const WINDOW: usize = 35;
const ROWS_PER: usize = 300;

/// dirRows: a folder first when named, then text files prefix-f001.. up to 300 rows.
fn dir_rows(prefix: &str, dir: &str) -> Vec<(String, bool)> {
    let first = usize::from(!dir.is_empty());
    let mut out = Vec::new();
    if !dir.is_empty() {
        out.push((dir.to_owned(), true));
    }
    for i in first..ROWS_PER {
        out.push((format!("{prefix}-f{i:03}"), false));
    }
    out
}

fn back_dirs() -> Dirs {
    let mut dirs = Dirs::new();
    dirs.insert("/a".into(), dir_rows("a", "b"));
    dirs.insert("/a/b".into(), dir_rows("b", "c"));
    dirs.insert("/a/b/c".into(), dir_rows("c", "c-dir"));
    dirs.insert("/a/b/c/c-dir".into(), dir_rows("e", ""));
    dirs.insert("/".into(), dir_rows("r", "a"));
    dirs
}

/// One launch: a fresh 1300 by 600 area at /a/b/c with the cursor on a file, so its child column never drew a row.
pub fn launch() -> Sim {
    let dirs = back_dirs();
    let rows = dirs["/a/b/c"]
        .iter()
        .take(WINDOW)
        .map(|(name, dir)| row(name, *dir))
        .collect();
    let mut pane = Pane::at("/a/b/c", rows);
    pane.first = WINDOW;
    pane.cursor = 1;
    let area = Area::new(1300., 600., None, 0., SETTINGS);
    let mut sim = Sim::new(area, pane, dirs, Some(6));
    sim.total = ROWS_PER;
    sim
}

/// A column as fp() reads it: its directory, the names it draws, the lifted name and its scroll; None while it shows no folder.
type Column = Option<(String, Vec<String>, Option<String>, i32)>;

/// What a frame draws: the parent, active and child columns, and the cursor.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    columns: Vec<Column>,
    cursor: usize,
}

fn drawn(sim: &Sim) -> Drawn {
    let column = |slot: Slot| -> Column {
        let view = sim.track(slot)?;
        let names: Vec<String> = match &view.rows {
            TrackRows::Peek(rows) => rows.iter().map(|row| row.name.clone()).collect(),
            TrackRows::Listing => sim.pane.rows.iter().map(|row| row.name.clone()).collect(),
            TrackRows::Preview | TrackRows::Blank => return None,
        };
        let scroll = sim.area.scroll(slot).round() as i32;
        Some((view.path, names, view.lifted, scroll))
    };
    Drawn {
        columns: [Slot::Parent, Slot::Active, Slot::Child]
            .into_iter()
            .map(column)
            .collect(),
        cursor: sim.pane.cursor,
    }
}

/// whole(): the landed frame stands on /a/b with the cursor on c, each column drawing its directory's first window in order.
fn whole(sim: &Sim, frame: &Drawn) -> bool {
    let first = |path: &str| -> Vec<String> {
        sim.dirs[path]
            .iter()
            .take(WINDOW)
            .map(|(name, _)| name.clone())
            .collect()
    };
    let draws = |at: usize, path: &str| {
        frame.columns[at]
            .as_ref()
            .is_some_and(|(dir, names, _, _)| dir == path && *names == first(path))
    };
    sim.pane.here == "/a/b"
        && frame.cursor == 0
        && draws(0, "/a")
        && draws(1, "/a/b")
        && draws(2, "/a/b/c")
}

/// What one measured Back saw from its request to 320 ms on.
struct Land {
    shape: &'static str,
    parent_at_request: usize,
    child_pre: usize,
    child_peeks: usize,
    pre_changes: usize,
    late: usize,
    whole: bool,
}

/// measure() through finish(): Back to /a/b selecting /a/b/c, every state before the landing against the rest, every one after against the first.
fn measure(sim: &mut Sim, shape: &'static str) -> Option<Land> {
    let asked_from = sim.asked.len();
    let landed_from = sim.landings.len();
    let rest = drawn(sim);
    sim.open("/a/b", "/a/b/c");
    let mut pre_changes = usize::from(drawn(sim) != rest);
    let mut first: Option<Drawn> = None;
    let mut late = 0;
    sim.watch(320, |s| {
        let now = drawn(s);
        if s.landings.len() == landed_from {
            pre_changes += usize::from(now != rest);
        } else if let Some(first) = &first {
            late += usize::from(now != *first);
        } else {
            first = Some(now);
        }
    });
    let landing = *sim.landings.get(landed_from)?;
    let asked = &sim.asked[asked_from..];
    let count = |path: &str, pre: bool| {
        asked
            .iter()
            .filter(|(name, at)| name == path && (!pre || *at < landing))
            .count()
    };
    let whole = first.as_ref().is_some_and(|frame| whole(sim, frame));
    Some(Land {
        shape,
        parent_at_request: count("/a", true),
        child_pre: count("/a/b/c", true),
        child_peeks: count("/a/b/c", false),
        pre_changes,
        late,
        whole,
    })
}

#[test]
fn every_back_landing_asks_its_parent_once_and_draws_whole() {
    let mut c = Checks::new(SOURCE);
    let mut lands = Vec::new();
    let mut downs = 0;
    for at in 0..16 {
        let mut sim = launch();
        sim.run(200);
        if at > 0 {
            lands.extend(measure(&mut sim, "back1"));
        } else {
            sim.open("/a/b", "/a/b/c");
            sim.run(320);
        }
        sim.open("/a/b/c", "");
        sim.run(200);
        lands.extend(measure(&mut sim, "back2"));
        sim.open("/a/b/c", "");
        sim.run(200);
        // A Down 16 ms after the third Back's landing, as a key held through Back sends it.
        let landed_from = sim.landings.len();
        sim.open("/a/b", "/a/b/c");
        if sim.wait(|s| s.landings.len() > landed_from) {
            sim.run(16);
            sim.pane.cursor = 1;
            sim.sync();
            downs += 1;
        }
        sim.run(200);
    }
    c.eq(365, "ran <n> Back landings, want 31", lands.len(), 31);
    c.eq(366, "ran <n> Downs, want 16", downs, 16);
    for (i, land) in lands.iter().enumerate() {
        let shape = land.shape;
        c.eq(
            378,
            &format!("{shape} {i} asked the parent <n> times at the request, want 1"),
            land.parent_at_request,
            1,
        );
        c.eq(
            379,
            &format!("{shape} {i} asked the folder left <n> times before its landing"),
            land.child_pre,
            0,
        );
        c.ok(
            380,
            &format!("{shape} {i} asked the folder left <n> times, want at most its one watch"),
            land.child_peeks <= 1,
        );
        c.eq(
            382,
            &format!("{shape} {i} changed items in <n> frames before its landing"),
            land.pre_changes,
            0,
        );
        c.eq(
            383,
            &format!("{shape} {i} changed items in <n> frames after its first"),
            land.late,
            0,
        );
        c.ok(
            385,
            &format!("{shape} {i} first frame <first> is not its whole frame <whole>"),
            land.whole,
        );
    }
    // The failure notes, the frame hook, the Down sampling and the report are this test's own steps.
    c.covers(&[25, 46, 259, 268, 346, 349, 387]);
    c.finish();
}
