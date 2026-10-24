//! The shipped jump ranker, held to tests/js/jump.js, and the rows it ranks for each typed line.
use super::{js, HOME};
use flea_gui::path::{
    rank::{display, is_query, matched, run, Jump, SOURCE_ROWS},
    suggestions::{Ranker, Suggestion},
    Jumped,
};

fn sources(
    favourites: &[&str],
    zoxide: &[&str],
    recent: &[&str],
    frecency: &[(&str, f64)],
) -> Jumped {
    let owned = |paths: &[&str]| paths.iter().map(|path| path.to_string()).collect();
    Jumped {
        id: 1,
        favourites: owned(favourites),
        zoxide: owned(zoxide),
        recent: owned(recent),
        frecency: frecency
            .iter()
            .map(|(path, score)| (path.to_string(), *score))
            .collect(),
    }
}

// tests/js/jump.js BOARD: lib.FAVORITES, the board's FREQUENT and RECENT, and the fixture's zoxide scores.
pub fn board() -> Jumped {
    sources(
        &[
            "/home/gm/Projects",
            "/home/gm/Documents/claude/flea",
            "/home/gm/Documents/claude/omarchy",
        ],
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
        format!(
            "{}[{}]{}",
            &t[..row.wash.start],
            &t[row.wash.clone()],
            &t[row.wash.end..]
        )
    };
    rows.iter().map(row).collect::<Vec<_>>().join(" ")
}

fn score(hay: &str, query: &str) -> Option<i32> {
    matched(hay, query).map(|(score, _)| score)
}

#[test]
fn a_line_is_a_name_or_a_path_and_home_draws_as_a_tilde() {
    for (line, want, at) in [
        ("o", true, 31),
        ("cl/fl", false, 32),
        ("Work/", false, 33),
        ("~", false, 34),
        ("..", false, 35),
    ] {
        js("jump.js", at, is_query(line), want);
    }
    for (line, want) in [
        (".", false),
        (".config", true),
        ("v1.2", true),
        ("/usr/share", false),
        ("~/Work", false),
    ] {
        js("jump.js", 37, is_query(line), want);
    }
    for (line, want) in [
        ("../x", false),
        ("file:///etc", false),
        ("/home/gm/Documents/", false),
        ("  ", false),
    ] {
        js("jump.js", 44, is_query(line), want);
    }
    js(
        "jump.js",
        40,
        drawn(&rows(
            &sources(&[], &["/home/gm/.config/hypr", "/home/gm/Work"], &[], &[]),
            ".config",
        )),
        "~/[.config]/hypr".into(),
    );
    js(
        "jump.js",
        50,
        display("/home/gm/Work/field", HOME),
        "~/Work/field".into(),
    );
    js("jump.js", 51, display("/home/gm", HOME), "~".into());
    js(
        "jump.js",
        52,
        display("/home/gmx/a", HOME),
        "/home/gmx/a".into(),
    );
    js(
        "jump.js",
        53,
        display("/home/gm/a", ""),
        "/home/gm/a".into(),
    );
}

#[test]
fn the_board_and_each_clause_of_the_ruling_rank_as_jump_js_ranks_them() {
    let want = "~/D[o]cuments ~/D[o]wnloads ~/Pr[o]jects ~/Documents/claude/[o]marchy ~/Pictures/screensh[o]ts ~/D[o]cuments/claude/flea ~/W[o]rk/field";
    js("jump.js", 57, drawn(&rows(&board(), "o")), want.into());
    js(
        "jump.js",
        62,
        drawn(&rows(&board(), "shots")),
        "~/Pictures/screen[shots]".into(),
    );
    js(
        "jump.js",
        63,
        drawn(&rows(
            &sources(
                &[],
                &[],
                &["/home/gm/Pictures/screenshots", "/home/gm/Work/field"],
                &[],
            ),
            "field",
        )),
        "~/Work/[field]".into(),
    );
    js("jump.js", 64, rows(&Jumped::default(), "o").len(), 0);
    js("jump.js", 65, rows(&board(), "/home/gm").len(), 0);
    js(
        "jump.js",
        66,
        drawn(&rows(&board(), " field ")),
        "~/Work/[field]".into(),
    );
    let own = sources(
        &[],
        &["/home/gm/notes/archive", "/home/gm/Work/notes"],
        &[],
        &[("/home/gm/notes/archive", 90.)],
    );
    js(
        "jump.js",
        70,
        drawn(&rows(&own, "notes")),
        "~/Work/[notes] ~/[notes]/archive".into(),
    );
    let contiguous = sources(
        &[],
        &["/home/gm/t-a-x", "/home/gm/Documents/tax"],
        &[],
        &[("/home/gm/t-a-x", 90.)],
    );
    js(
        "jump.js",
        73,
        drawn(&rows(&contiguous, "tax")),
        "~/Documents/[tax] ~/[t]-a-x".into(),
    );
    let frecent = sources(
        &[],
        &["/home/gm/a/work", "/home/gm/b/work"],
        &[],
        &[("/home/gm/a/work", 2.), ("/home/gm/b/work", 9.)],
    );
    js(
        "jump.js",
        76,
        drawn(&rows(&frecent, "work")),
        "~/b/[work] ~/a/[work]".into(),
    );
    let tie = sources(
        &["/home/gm/a/work"],
        &["/home/gm/b/work"],
        &["/home/gm/c/work"],
        &[],
    );
    js(
        "jump.js",
        78,
        drawn(&rows(&tie, "work")),
        "~/a/[work] ~/b/[work] ~/c/[work]".into(),
    );
    let beaten = sources(
        &["/home/gm/a/work"],
        &["/home/gm/b/work"],
        &[],
        &[("/home/gm/b/work", 1.)],
    );
    js(
        "jump.js",
        80,
        drawn(&rows(&beaten, "work")),
        "~/b/[work] ~/a/[work]".into(),
    );
    let carried = sources(
        &["/home/gm/a/work"],
        &["/home/gm/b/work"],
        &[],
        &[("/home/gm/a/work", 5.), ("/home/gm/b/work", 1.)],
    );
    js(
        "jump.js",
        83,
        drawn(&rows(&carried, "work")),
        "~/a/[work] ~/b/[work]".into(),
    );
    let many = sources(
        &["/f/x1", "/f/x2"],
        &[
            "/a/x1", "/a/x2", "/a/x3", "/a/x4", "/a/x5", "/a/x6", "/a/x7",
        ],
        &[],
        &[],
    );
    js("jump.js", 86, rows(&many, "x").len(), 2 + SOURCE_ROWS);
    let paths: Vec<_> = rows(&many, "x").into_iter().map(|row| row.path).collect();
    js(
        "jump.js",
        87,
        paths.join(" "),
        "/f/x1 /f/x2 /a/x1 /a/x2 /a/x3 /a/x4 /a/x5".into(),
    );
}

#[test]
fn a_scattered_match_is_dropped_and_the_scorer_keeps_fuzzy_rs_scores() {
    let scattered = sources(
        &["/home/gm/Documents/claude/omarchy"],
        &["/home/gm/Work/claude/flea-013/src"],
        &[],
        &[],
    );
    js(
        "jump.js",
        92,
        drawn(&rows(&scattered, "src")),
        "~/Work/claude/flea-013/[src]".into(),
    );
    let buried = sources(
        &[],
        &["/home/gm/Projects/homelab/nix"],
        &["/home/gm/Documents/tax"],
        &[],
    );
    js(
        "jump.js",
        95,
        drawn(&rows(&buried, "tax")),
        "~/Documents/[tax]".into(),
    );
    let abbreviated = sources(
        &["/home/gm/Documents/claude/flea"],
        &["/home/gm/Downloads"],
        &[],
        &[],
    );
    js(
        "jump.js",
        97,
        drawn(&rows(&abbreviated, "dl")),
        "~/[D]ownloads".into(),
    );
    js(
        "jump.js",
        99,
        drawn(&rows(
            &sources(&[], &["/home/gm/Documents/claude/flea"], &[], &[]),
            "claude",
        )),
        "~/Documents/[claude]/flea".into(),
    );
    js(
        "jump.js",
        101,
        rows(&sources(&[], &["/home/gm/Work/field"], &[], &[]), "o").len(),
        1,
    );
    js(
        "jump.js",
        104,
        score("downloads/helper.txt", "dwnhelp").is_some(),
        true,
    );
    js("jump.js", 105, score("helper.txt", "pleh"), None);
    js("jump.js", 106, score("abc", "abcd"), None);
    js(
        "jump.js",
        108,
        score("CAFÉ.txt", "café").is_some() && score("café.txt", "CAFÉ").is_some(),
        true,
    );
    js("jump.js", 109, score("anything", ""), Some(0));
    js(
        "jump.js",
        116,
        score(&"a".repeat(4096), "aa").is_some(),
        true,
    );
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
    js(
        "jump.js",
        132,
        matched("~/Documents/claude/omarchy", "o").unwrap().1[0],
        19,
    );
    js("jump.js", 133, run(&[2, 5, 6, 7, 9]), Some((5, 3)));
    js("jump.js", 134, run(&[1, 2, 5, 6]), Some((1, 2)));
    js("jump.js", 135, run(&[]), None);
}

#[test]
fn the_wash_and_the_leaf_are_byte_offsets_past_a_wide_character() {
    let row = &rows(
        &sources(&[], &["/srv/caf\u{e9}/\u{1F4C1}omarchy"], &[], &[]),
        "om",
    )[0];
    assert_eq!(&row.text[row.leaf_start..], "\u{1F4C1}omarchy");
    assert_eq!(&row.text[row.wash.clone()], "om");
}
