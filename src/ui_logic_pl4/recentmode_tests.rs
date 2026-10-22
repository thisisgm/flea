// Ported from tests/js/recentmode.js over a stub pane shaped like its pane() fixture.
use super::recentmode::*;

#[derive(Default)]
struct Pane {
    path: String,
    in_flight: bool,
    sort: (String, bool),
    said: Vec<&'static str>,
    listed: Vec<String>,
    opened: Vec<(String, bool)>,
    superseded: usize,
    walk: bool,
    walk_cancelled: usize,
    pending_select: String,
    rail: bool,
    rereads: usize,
    cursor: Option<String>,
}

fn pane(path: &str) -> Pane {
    Pane {
        path: path.into(),
        sort: ("name".into(), false),
        ..Pane::default()
    }
}

impl RecentHost for Pane {
    fn list_in_flight(&self) -> bool {
        self.in_flight
    }
    fn supersede_listing(&mut self) {
        self.superseded += 1;
    }
    fn sort(&self) -> (String, bool) {
        self.sort.clone()
    }
    fn set_sort(&mut self, by: &str, desc: bool) {
        self.sort = (by.into(), desc);
    }
    fn place(&self) -> String {
        self.path.clone()
    }
    fn leave_walk(&mut self) -> bool {
        if self.walk {
            self.walk = false;
            self.walk_cancelled += 1;
        }
        false
    }
    fn list_paths(&mut self, paths: &[String], _hold_walk: bool) {
        self.path = "/".into();
        self.in_flight = true;
        self.listed.push(format!("{}|200", paths.join(",")));
    }
    fn message(&mut self, key: &'static str) {
        self.said.push(key);
    }
    fn set_pending_select(&mut self, path: &str) {
        self.pending_select = path.into();
    }
    fn open_without_history(&mut self, path: &str, supersede: bool) {
        self.opened.push((path.into(), supersede));
        self.path = path.into();
    }
    fn reread_history(&mut self) -> bool {
        self.rereads += usize::from(self.rail);
        self.rail
    }
    fn cursor_path(&self) -> Option<String> {
        self.cursor.clone()
    }
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|p| (*p).to_owned()).collect()
}

#[test]
fn opening_recent_keeps_the_way_back_and_takes_used_order() {
    let (mut p, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    mode.run(
        &mut p,
        paths(&["/home/gm/a.txt", "/home/gm/b.txt"]),
        Visits::new(),
    );
    assert_eq!(p.path, "/", "opening Recent moves to the history's base");
    assert_eq!(mode.from, "/home/gm/Work", "and keeps where it stood");
    assert!(mode.on, "and the mode stands");
    assert_eq!(
        p.listed,
        ["/home/gm/a.txt,/home/gm/b.txt|200"],
        "and the base is asked for those paths"
    );
    assert_eq!(
        p.sort,
        ("mtime".into(), true),
        "and Used describes newest first"
    );
    mode.run(&mut p, paths(&["/home/gm/c.txt"]), Visits::new());
    assert!(
        p.said.is_empty(),
        "an open while one lands is taken, not refused"
    );
    assert_eq!(p.superseded, 1, "and supersedes the listing out");
    assert_eq!(
        p.listed.len(),
        2,
        "and asks the second open's paths behind the first"
    );
    assert_eq!(mode.from, "/home/gm/Work", "and keeps the way back");
    p.in_flight = false;
    mode.close(&mut p);
    assert_eq!(
        p.opened,
        [("/home/gm/Work".to_owned(), true)],
        "leaving hands back the folder, as the newest request"
    );
    assert!(!mode.on && mode.from.is_empty(), "and the mode is off");
    assert_eq!(
        p.sort,
        ("name".into(), false),
        "and the newest-first order does not follow it out"
    );
}

#[test]
fn a_walk_closes_and_used_stamps_only_held_rows() {
    let (mut walking, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    walking.walk = true;
    mode.run(&mut walking, paths(&["/home/gm/a.txt"]), Visits::new());
    assert_eq!(
        walking.walk_cancelled, 1,
        "a live session is closed before the history answers"
    );
    let mut visits = Visits::new();
    visits.insert("/a.txt".into(), Some(1_700_000_000));
    let mut stamped = RecentMode {
        on: true,
        visits,
        ..RecentMode::default()
    };
    assert_eq!(
        stamped.used("/a.txt"),
        Some(1_700_000_000),
        "Used reads the visit"
    );
    assert_eq!(
        stamped.used("/b.txt"),
        None,
        "an absent visit never displays an mtime"
    );
    stamped.on = false;
    assert_eq!(
        stamped.used("/a.txt"),
        None,
        "directory rows keep no Used stamp"
    );
}

#[test]
fn refresh_rereads_the_history_or_the_standing_paths() {
    let (mut p, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    mode.run(&mut p, paths(&["/home/gm/a.txt"]), Visits::new());
    p.in_flight = false;
    p.rail = true;
    mode.refresh(&mut p, "/home/gm/a.txt");
    assert_eq!(p.rereads, 1, "a refresh re-reads the history");
    assert_eq!(
        p.pending_select, "/home/gm/a.txt",
        "and holds the operated row"
    );
    assert_eq!(p.listed.len(), 1, "and lists nothing itself");
    p.rail = false;
    mode.refresh(&mut p, "");
    assert_eq!(
        p.listed,
        ["/home/gm/a.txt|200", "/home/gm/a.txt|200"],
        "with no rail it re-asks the standing paths"
    );
    mode.refresh(&mut p, "x");
    assert_eq!(
        p.said,
        [BUSY_KEY],
        "a refresh while a listing lands is refused"
    );
}

#[test]
fn reveal_opens_the_rows_folder_and_hands_the_order_back() {
    let (mut p, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    p.sort = ("kind".into(), true);
    mode.run(&mut p, paths(&["/home/gm/Docs/a.txt"]), Visits::new());
    p.in_flight = false;
    p.cursor = Some("/home/gm/Docs/a.txt".into());
    mode.reveal(&mut p);
    assert_eq!(
        p.opened,
        [("/home/gm/Docs".to_owned(), false)],
        "reveal opens the row's own folder"
    );
    assert_eq!(
        p.pending_select, "/home/gm/Docs/a.txt",
        "selecting the row it came from"
    );
    assert!(!mode.on, "and the mode is off");
    assert_eq!(
        p.sort,
        ("kind".into(), true),
        "and hands the standing order back"
    );
    let (mut root, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    mode.run(&mut root, paths(&["/a.txt"]), Visits::new());
    root.in_flight = false;
    root.cursor = Some("/a.txt".into());
    mode.reveal(&mut root);
    assert_eq!(root.opened[0].0, "/", "a root-level row reveals the root");
    let (mut busy, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    mode.run(&mut busy, paths(&["/home/gm/a.txt"]), Visits::new());
    busy.cursor = Some("/home/gm/a.txt".into());
    mode.reveal(&mut busy);
    assert_eq!(busy.said, [BUSY_KEY], "a reveal while one lands is refused");
    assert!(
        mode.on && mode.from == "/home/gm/Work" && busy.opened.is_empty(),
        "and keeps the mode and the way back"
    );
    let mut unloaded = pane("/x");
    let mut off = RecentMode {
        on: true,
        ..RecentMode::default()
    };
    off.reveal(&mut unloaded);
    assert!(
        unloaded.opened.is_empty() && off.on,
        "a cursor row not loaded reveals nothing"
    );
}

#[test]
fn every_way_out_restores_the_standing_order() {
    let (mut loading, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    mode.run(&mut loading, paths(&["/home/gm/a.txt"]), Visits::new());
    mode.close(&mut loading);
    assert!(loading.said.is_empty(), "a close while one lands is taken");
    assert_eq!(
        loading.opened,
        [("/home/gm/Work".to_owned(), true)],
        "and asks the folder as a navigation"
    );
    let (mut switching, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    switching.sort = ("kind".into(), true);
    mode.run(&mut switching, paths(&["/home/gm/a.txt"]), Visits::new());
    assert!(
        mode.drop_overlay(&mut switching) && switching.sort == ("kind".into(), true),
        "a tab switch hands the order back"
    );
    assert!(
        !mode.drop_overlay(&mut switching),
        "and a second drop finds no mode"
    );
    let (mut hopping, mut mode) = (pane("/home/gm/Work"), RecentMode::default());
    hopping.sort = ("size".into(), true);
    mode.run(&mut hopping, paths(&["/home/gm/a.txt"]), Visits::new());
    assert_eq!(
        hopping.sort,
        ("mtime".into(), true),
        "a hop in takes newest first"
    );
    mode.leave(&mut hopping);
    assert_eq!(
        hopping.sort,
        ("size".into(), true),
        "a plain hop out hands the order back"
    );
    assert!(
        !mode.on && mode.from.is_empty() && mode.paths.is_empty(),
        "and blanks the mode it left"
    );
    let mut settled = pane("/home/gm/Work");
    settled.sort = ("kind".into(), true);
    let mut outside = RecentMode {
        stashed: Some(("size".into(), true)),
        ..RecentMode::default()
    };
    outside.leave(&mut settled);
    assert_eq!(
        settled.sort,
        ("kind".into(), true),
        "leaving outside Recent keeps its own order"
    );
}

#[test]
fn paste_is_refused_only_in_recent_and_a_tab_rests_on_the_way_back() {
    let mut p = pane("/x");
    let on = RecentMode {
        on: true,
        from: "/home/gm/Work".into(),
        ..RecentMode::default()
    };
    assert!(on.refuse_paste(&mut p), "it refuses in Recent");
    assert_eq!(p.said, [PASTE_REFUSED_KEY], "and says why");
    let mut quiet = pane("/x");
    assert!(
        !RecentMode::default().refuse_paste(&mut quiet) && quiet.said.is_empty(),
        "and stays silent off Recent"
    );
    assert_eq!(on.resting_path(), Some("/home/gm/Work"));
    assert_eq!(RecentMode::default().resting_path(), None);
}
