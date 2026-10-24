//! The shipped jump ranker, held to tests/js/jump.js and tests/js/recent.js, and the rows an open bar draws with it.
use super::{
    inputs, js,
    view::{bar, bar_at},
    HOME,
};
use flea_gui::{
    ipc::{path::PathAdapter, Adapter},
    path::{
        history::{self, favourites, history_path, path_of, LIMIT},
        rank::{display, is_query, matched, run, Jump, SOURCE_ROWS},
        suggestions::{Ranker, Suggestion, Unranked},
        Answer, Jumped,
    },
};
use gpui::{point, px, Modifiers, TestAppContext};

fn sources(favourites: &[&str], zoxide: &[&str], recent: &[&str], frecency: &[(&str, f64)]) -> Jumped {
    let owned = |paths: &[&str]| paths.iter().map(|path| path.to_string()).collect();
    Jumped {
        id: 1,
        favourites: owned(favourites),
        zoxide: owned(zoxide),
        recent: owned(recent),
        frecency: frecency.iter().map(|(path, score)| (path.to_string(), *score)).collect(),
    }
}

// tests/js/jump.js BOARD: lib.FAVORITES, the board's FREQUENT and RECENT, and the fixture's zoxide scores.
fn board() -> Jumped {
    sources(
        &["/home/gm/Projects", "/home/gm/Documents/claude/flea", "/home/gm/Documents/claude/omarchy"],
        &["/home/gm/Documents", "/home/gm/Downloads"],
        &["/home/gm/Pictures/screenshots", "/home/gm/Work/field"],
        &[("/home/gm/Documents", 80.), ("/home/gm/Downloads", 20.)],
    )
}

fn rows(answer: &Jumped, line: &str) -> Vec<Suggestion> {
    let mut ranker = Jump::default();
    ranker.prepare(answer, HOME);
    let mut out = Vec::new();
    ranker.rows(line, &mut out);
    out
}

// jump.js drawn(): each row's text with its washed run in brackets.
fn drawn(rows: &[Suggestion]) -> String {
    let row = |row: &Suggestion| {
        let t = &row.text;
        if row.wash.is_empty() {
            return t.clone();
        }
        format!("{}[{}]{}", &t[..row.wash.start], &t[row.wash.clone()], &t[row.wash.end..])
    };
    rows.iter().map(row).collect::<Vec<_>>().join(" ")
}

fn score(hay: &str, query: &str) -> Option<i32> {
    matched(hay, query).map(|(score, _)| score)
}

#[test]
fn a_line_is_a_name_or_a_path_and_home_draws_as_a_tilde() {
    for (line, want, at) in [("o", true, 31), ("cl/fl", false, 32), ("Work/", false, 33), ("~", false, 34), ("..", false, 35)] {
        js("jump.js", at, is_query(line), want);
    }
    for (line, want) in [(".", false), (".config", true), ("v1.2", true), ("/usr/share", false), ("~/Work", false)] {
        js("jump.js", 37, is_query(line), want);
    }
    for (line, want) in [("../x", false), ("file:///etc", false), ("/home/gm/Documents/", false), ("  ", false)] {
        js("jump.js", 44, is_query(line), want);
    }
    js("jump.js", 40, drawn(&rows(&sources(&[], &["/home/gm/.config/hypr", "/home/gm/Work"], &[], &[]), ".config")), "~/[.config]/hypr".into());
    js("jump.js", 50, display("/home/gm/Work/field", HOME), "~/Work/field".into());
    js("jump.js", 51, display("/home/gm", HOME), "~".into());
    js("jump.js", 52, display("/home/gmx/a", HOME), "/home/gmx/a".into());
    js("jump.js", 53, display("/home/gm/a", ""), "/home/gm/a".into());
}

#[test]
fn the_board_and_each_clause_of_the_ruling_rank_as_jump_js_ranks_them() {
    let want = "~/D[o]cuments ~/D[o]wnloads ~/Pr[o]jects ~/Documents/claude/[o]marchy ~/Pictures/screensh[o]ts ~/D[o]cuments/claude/flea ~/W[o]rk/field";
    js("jump.js", 57, drawn(&rows(&board(), "o")), want.into());
    js("jump.js", 62, drawn(&rows(&board(), "shots")), "~/Pictures/screen[shots]".into());
    js("jump.js", 63, drawn(&rows(&sources(&[], &[], &["/home/gm/Pictures/screenshots", "/home/gm/Work/field"], &[]), "field")), "~/Work/[field]".into());
    js("jump.js", 64, rows(&Jumped::default(), "o").len(), 0);
    js("jump.js", 65, rows(&board(), "/home/gm").len(), 0);
    js("jump.js", 66, drawn(&rows(&board(), " field ")), "~/Work/[field]".into());
    let own = sources(&[], &["/home/gm/notes/archive", "/home/gm/Work/notes"], &[], &[("/home/gm/notes/archive", 90.)]);
    js("jump.js", 70, drawn(&rows(&own, "notes")), "~/Work/[notes] ~/[notes]/archive".into());
    let contiguous = sources(&[], &["/home/gm/t-a-x", "/home/gm/Documents/tax"], &[], &[("/home/gm/t-a-x", 90.)]);
    js("jump.js", 73, drawn(&rows(&contiguous, "tax")), "~/Documents/[tax] ~/[t]-a-x".into());
    let frecent = sources(&[], &["/home/gm/a/work", "/home/gm/b/work"], &[], &[("/home/gm/a/work", 2.), ("/home/gm/b/work", 9.)]);
    js("jump.js", 76, drawn(&rows(&frecent, "work")), "~/b/[work] ~/a/[work]".into());
    let tie = sources(&["/home/gm/a/work"], &["/home/gm/b/work"], &["/home/gm/c/work"], &[]);
    js("jump.js", 78, drawn(&rows(&tie, "work")), "~/a/[work] ~/b/[work] ~/c/[work]".into());
    let beaten = sources(&["/home/gm/a/work"], &["/home/gm/b/work"], &[], &[("/home/gm/b/work", 1.)]);
    js("jump.js", 80, drawn(&rows(&beaten, "work")), "~/b/[work] ~/a/[work]".into());
    let carried = sources(&["/home/gm/a/work"], &["/home/gm/b/work"], &[], &[("/home/gm/a/work", 5.), ("/home/gm/b/work", 1.)]);
    js("jump.js", 83, drawn(&rows(&carried, "work")), "~/a/[work] ~/b/[work]".into());
    let many = sources(&["/f/x1", "/f/x2"], &["/a/x1", "/a/x2", "/a/x3", "/a/x4", "/a/x5", "/a/x6", "/a/x7"], &[], &[]);
    js("jump.js", 86, rows(&many, "x").len(), 2 + SOURCE_ROWS);
    let paths: Vec<_> = rows(&many, "x").into_iter().map(|row| row.path).collect();
    js("jump.js", 87, paths.join(" "), "/f/x1 /f/x2 /a/x1 /a/x2 /a/x3 /a/x4 /a/x5".into());
}

#[test]
fn a_scattered_match_is_dropped_and_the_scorer_keeps_fuzzy_rs_scores() {
    let scattered = sources(&["/home/gm/Documents/claude/omarchy"], &["/home/gm/Work/claude/flea-013/src"], &[], &[]);
    js("jump.js", 92, drawn(&rows(&scattered, "src")), "~/Work/claude/flea-013/[src]".into());
    let buried = sources(&[], &["/home/gm/Projects/homelab/nix"], &["/home/gm/Documents/tax"], &[]);
    js("jump.js", 95, drawn(&rows(&buried, "tax")), "~/Documents/[tax]".into());
    let abbreviated = sources(&["/home/gm/Documents/claude/flea"], &["/home/gm/Downloads"], &[], &[]);
    js("jump.js", 97, drawn(&rows(&abbreviated, "dl")), "~/[D]ownloads".into());
    js("jump.js", 99, drawn(&rows(&sources(&[], &["/home/gm/Documents/claude/flea"], &[], &[]), "claude")), "~/Documents/[claude]/flea".into());
    js("jump.js", 101, rows(&sources(&[], &["/home/gm/Work/field"], &[], &[]), "o").len(), 1);
    js("jump.js", 104, score("downloads/helper.txt", "dwnhelp").is_some(), true);
    js("jump.js", 105, score("helper.txt", "pleh"), None);
    js("jump.js", 106, score("abc", "abcd"), None);
    js("jump.js", 108, score("CAFÉ.txt", "café").is_some() && score("café.txt", "CAFÉ").is_some(), true);
    js("jump.js", 109, score("anything", ""), Some(0));
    js("jump.js", 116, score(&"a".repeat(4096), "aa").is_some(), true);
    let bounded = format!("{}ab", "ax".repeat(16));
    let table = [
        ("report.txt", "rep", 34),
        ("raspberry-pie.txt", "rep", 16),
        ("my-notes.txt", "notes", 58),
        ("bignotes.txt", "notes", 52),
        ("SearchStrip.qml", "strip", 58),
        ("searchstrip.qml", "strip", 52),
        ("notes/bench.txt", "bench", 58),
        ("bench/notes.txt", "bench", 38),
        ("axxab", "ab", 16),
        ("axxb", "ab", 12),
        (bounded.as_str(), "ab", 6),
        ("downloads/helper.txt", "dwnhelp", 53),
        ("~/Documents/claude/omarchy", "o", 10),
        ("~/Documents/claude/omarchy", "src", 5),
        ("~/Projects/homelab/nix", "tax", -7),
    ];
    for (hay, query, want) in table {
        js("jump.js", 126, score(hay, query), Some(want));
    }
    js("jump.js", 132, matched("~/Documents/claude/omarchy", "o").unwrap().1[0], 19);
    js("jump.js", 133, run(&[2, 5, 6, 7, 9]), Some((5, 3)));
    js("jump.js", 134, run(&[1, 2, 5, 6]), Some((1, 2)));
    js("jump.js", 135, run(&[]), None);
}

#[test]
fn the_wash_and_the_leaf_are_byte_offsets_past_a_wide_character() {
    let row = &rows(&sources(&[], &["/srv/caf\u{e9}/\u{1F4C1}omarchy"], &[], &[]), "om")[0];
    assert_eq!(&row.text[row.leaf_start..], "\u{1F4C1}omarchy");
    assert_eq!(&row.text[row.wash.clone()], "om");
}

#[test]
fn the_recent_history_reads_as_recent_js_reads_it() {
    js("recent.js", 8, history_path(Some("/x/data"), HOME), "/x/data/recently-used.xbel".into());
    js("recent.js", 10, history_path(None, HOME), "/home/gm/.local/share/recently-used.xbel".into());
    js("recent.js", 12, history_path(Some("data"), HOME), "/home/gm/.local/share/recently-used.xbel".into());
    js("recent.js", 14, history_path(Some("/x/data/"), HOME), "/x/data/recently-used.xbel".into());
    let local = [
        ("file:///home/gm/a.png", "/home/gm/a.png"),
        ("file:///home/gm/a%20b%23c.png", "/home/gm/a b#c.png"),
        ("file:///home/gm/caf%C3%A9.txt", "/home/gm/café.txt"),
        ("file://localhost/etc/hostname", "/etc/hostname"),
        ("FILE:///etc/hostname", "/etc/hostname"),
        ("file:///home/gm/a%C2%85b", "/home/gm/a\u{85}b"),
    ];
    for (href, want) in local {
        js("recent.js", 18, path_of(href), Some(want.into()));
    }
    for href in ["file://nas.local/share/x.txt", "smb://nas/share/x.txt", "trash:///x.txt", "/home/gm/a.png", "", "file://", "file:///"] {
        js("recent.js", 24, path_of(href), None);
    }
    for href in ["file:///home/gm/a%00b", "file:///home/gm/a%0Ab", "file:///home/gm/a%7Fb", "file:///home/gm/%zz"] {
        js("recent.js", 33, path_of(href), None);
    }
    let mark = |href: &str, stamp: &str| format!("<bookmark href=\"{href}\" added=\"2026-01-01T00:00:00Z\" visited=\"{stamp}\">\n  <info/>\n</bookmark>");
    let file = [
        mark("file:///home/gm/old.txt", "2026-08-01T09:00:00Z"),
        mark("smb://nas/x", "2026-09-01T09:00:00Z"),
        mark("file:///home/gm/new.png", "2026-08-30T11:32:04Z"),
        mark("file:///home/gm/mid.md", "2026-08-15T09:00:00Z"),
        mark("file:///home/gm/new.png", "2026-07-01T09:00:00Z"),
        "<bookmark href='file:///home/gm/q&amp;a.txt'/>".into(),
    ];
    let text = format!("<?xml version=\"1.0\"?>\n<xbel version=\"1.0\">\n{}\n</xbel>\n", file.join("\n"));
    js("recent.js", 44, history::read(&text), vec!["/home/gm/new.png".into(), "/home/gm/mid.md".into(), "/home/gm/old.txt".into(), "/home/gm/q&a.txt".into()]);
    let tied = [mark("file:///home/gm/first.txt", "2026-08-01T00:00:00Z"), mark("file:///home/gm/second.txt", "2026-08-01T00:00:00Z")];
    js("recent.js", 61, history::read(&tied.join("")), vec!["/home/gm/first.txt".into(), "/home/gm/second.txt".into()]);
    let many: String = (0..550).map(|at| mark(&format!("file:///home/gm/g{at}.txt"), &format!("2026-08-01T00:{:02}:{:02}Z", at / 60, at % 60))).collect();
    let newest = history::read(&many);
    js("recent.js", 92, newest.len(), LIMIT);
    js("recent.js", 93, (newest[0].as_str(), newest[LIMIT - 1].as_str()), ("/home/gm/g549.txt", "/home/gm/g50.txt"));
    js("recent.js", 72, history::read("").len(), 0);
}

#[test]
fn the_favourites_are_the_places_paths_the_ui_state_keeps() {
    let state = flea::jsondoc::parse(r#"{"places":{"favourites":[{"label":"P","path":"/fx/j/Projects"},{"label":"x"},{"path":"rel"},{"path":"/fx/j/flea"}]}}"#).unwrap();
    assert_eq!(favourites(&state), ["/fx/j/Projects", "/fx/j/flea"]);
    assert!(favourites(&flea::jsondoc::parse("{}").unwrap()).is_empty());
}

fn suggest(cx: &mut TestAppContext, ranker: Box<dyn Ranker>, typed: &str) -> (String, Vec<String>) {
    let mut bar = bar(cx, ranker, inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input(typed);
    bar.cx.run_until_parked();
    let drawn = bar.read(|view| PathAdapter { view }.call("pathSuggestionRows", &[])).unwrap().stdout;
    let paths = bar.read(|view| view.jump.entries.iter().map(|row| row.text.clone()).collect());
    (drawn.trim_end().to_owned(), paths)
}

#[gpui::test]
fn the_shipped_ranker_draws_the_boards_seven_rows_for_o(cx: &mut TestAppContext) {
    let (drawn, paths) = suggest(cx, Box::new(Jump::default()), "o");
    let want = ["~/Documents", "~/Downloads", "~/Projects", "~/Documents/claude/omarchy", "~/Pictures/screenshots", "~/Documents/claude/flea", "~/Work/field"];
    assert_eq!(paths, want);
    assert_eq!(drawn, "7", "pathSuggestionRows reads the rows the popup laid and painted");
}

#[gpui::test]
fn the_seam_stand_in_draws_no_row_and_the_reader_says_so(cx: &mut TestAppContext) {
    let (drawn, paths) = suggest(cx, Box::new(Unranked), "o");
    assert!(paths.is_empty());
    assert_eq!(drawn, "0", "a bar that draws no suggestion reads 0, so a scene waiting on rows fails");
}

#[gpui::test]
fn the_reader_falls_to_zero_when_the_popup_goes(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Jump::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("s");
    bar.cx.run_until_parked();
    let rows = |bar: &mut super::view::Bar<'_>| bar.read(|view| PathAdapter { view }.call("pathSuggestionRows", &[])).unwrap().stdout;
    // Six board folders hold an s, and a 360 px window shows them all.
    assert_eq!(rows(&mut bar).trim_end(), "6");
    bar.cx.simulate_input("/");
    bar.cx.run_until_parked();
    assert_eq!(rows(&mut bar).trim_end(), "0", "a path line lists nothing");
    bar.cx.simulate_keystrokes("escape");
    bar.cx.run_until_parked();
    assert_eq!(rows(&mut bar).trim_end(), "0");
}

#[gpui::test]
fn the_cursor_reader_follows_the_highlight_and_reads_minus_one_with_no_popup(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Jump::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    let cursor = |bar: &mut super::view::Bar<'_>| bar.read(|view| PathAdapter { view }.call("pathSuggestionCursor", &[])).unwrap().stdout;
    assert_eq!(cursor(&mut bar).trim_end(), "0", "the first row stands highlighted");
    bar.cx.simulate_keystrokes("down down");
    assert_eq!(cursor(&mut bar).trim_end(), "2", "each Down steps the highlight one row");
    bar.cx.simulate_keystrokes("escape");
    bar.cx.run_until_parked();
    assert_eq!(cursor(&mut bar).trim_end(), "-1", "a bar with no popup reads -1");
}

#[gpui::test]
fn the_hover_takes_the_cursor_only_on_a_second_distinct_move(cx: &mut TestAppContext) {
    // The FIX6 hover scenes (200,92) then (200,128): row 1 is y 73 to 110, row 2 y 110 to 147.
    let mut bar = bar(cx, Box::new(Jump::default()), inputs(HOME));
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    let cursor = |bar: &mut super::view::Bar<'_>| bar.read(|view| PathAdapter { view }.call("pathSuggestionCursor", &[])).unwrap().stdout;
    assert_eq!(cursor(&mut bar).trim_end(), "0");
    assert!(bar.cx.update(|window, cx| window.simulate_next_frame(cx)) > 0, "the rows settle before the pointer arrives");
    bar.cx.simulate_mouse_move(point(px(200.), px(92.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(cursor(&mut bar).trim_end(), "0", "the entry sighting over row 1 takes nothing");
    bar.cx.simulate_mouse_move(point(px(200.), px(92.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(cursor(&mut bar).trim_end(), "0", "the same point again takes nothing");
    bar.cx.simulate_mouse_move(point(px(200.), px(128.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(cursor(&mut bar).trim_end(), "2", "a distinct point in row 2 takes the cursor");
}

#[gpui::test]
fn the_pointer_reader_reports_the_last_seen_point_or_none(cx: &mut TestAppContext) {
    // The popup stands at the chrome slot (80, 30), so the scene's entry (200, 92) reads (120, 62):
    // with the popup at x 80, dropping the x origin would read (200, 62) instead.
    let mut bar = bar_at(cx, Box::new(Jump::default()), inputs(HOME), 80.);
    let pointer = |bar: &mut super::view::Bar<'_>| bar.read(|view| PathAdapter { view }.call("pathSuggestionPointer", &[])).unwrap().stdout;
    assert_eq!(pointer(&mut bar).trim_end(), "none", "no popup, no sighting");
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    assert_eq!(pointer(&mut bar).trim_end(), "none", "the popup opens with no sighting");
    assert!(bar.cx.update(|window, cx| window.simulate_next_frame(cx)) > 0);
    assert_eq!(pointer(&mut bar).trim_end(), "none", "a settled popup still saw no pointer");
    bar.cx.simulate_mouse_move(point(px(200.), px(92.)), None, Modifiers::none());
    bar.cx.run_until_parked();
    assert_eq!(pointer(&mut bar).trim_end(), "120 62", "the entry point in popup coordinates");
    bar.cx.simulate_keystrokes("escape");
    bar.cx.run_until_parked();
    assert_eq!(pointer(&mut bar).trim_end(), "none", "closing the popup drops the sighting");
    bar.update(|view, window, cx| view.open(window, cx));
    bar.update(|view, window, cx| view.receive(Answer::Jumped(board()), window, cx));
    bar.cx.simulate_input("o");
    bar.cx.run_until_parked();
    assert_eq!(pointer(&mut bar).trim_end(), "none", "reopening starts with no sighting");
}
