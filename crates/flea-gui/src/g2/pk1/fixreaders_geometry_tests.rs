//! FIX-HARNESS-READERS: the picker snapshot readers, beside the geometry endpoint fixture.
use super::*;

// CTL2 add-on: PickerWindow.qml snapshot carries jump {open, rows, visibleRows, contentY, cursor, paths}; GPUI reads its path view's dropdown.
#[gpui::test]
fn fixreaders_picker_snapshot_carries_the_jump_dropdown(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let reply = endpoint(&mut f, "snapshot", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    let jump = state.get("jump").expect("the snapshot has a jump member");
    assert_eq!(jump.get("open").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(jump.get("rows").and_then(|v| v.as_f64()), Some(0.));
    assert_eq!(jump.get("cursor").and_then(|v| v.as_f64()), Some(-1.));
    assert_eq!(
        jump.get("paths").and_then(|v| v.as_array()).map(<[_]>::len),
        Some(0)
    );
    for key in ["visibleRows", "contentY"] {
        assert!(jump.get(key).is_some(), "{key}");
    }
}

// Sample sent line: {"c":"jump","id":3,"ranking":0,"favourites":[],"recent":[]}
fn answer_jumps(f: &mut Fixture, from: usize, places: &str) {
    let mut at = from;
    while let Some(line) = f.listing.sent().get(at).cloned() {
        at += 1;
        let Some(id) = line
            .strip_prefix(r#"{"c":"jump","id":"#)
            .and_then(|line| line.split(',').next())
        else {
            continue;
        };
        f.listing.say(Line::Reply(format!(
            r#"{{"t":"jumped","id":{id},{places},"frecency":{{}}}}"#
        )));
        f.cx.run_until_parked();
    }
}

/// Opens the path bar on /d/sub, answers every jump ask with places, then types name.
fn typed(f: &mut Fixture, places: &str, name: &str) -> flea::jsondoc::Json {
    crate::g2::pk1::pointer_tests::seed(f);
    let from = f.listing.sent().len();
    crate::g2::pk1::pointer_tests::path_click(f, true);
    answer_jumps(f, from, places);
    // A path line ranks nothing (rank.rs is_query), so /d/subway, which the line /d/sub would match as a name, stays undrawn.
    let closed = jump(f);
    assert_eq!(closed.get("open").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(number(&closed, "rows"), Some(0.));
    let from = f.listing.sent().len();
    f.cx.simulate_input(name);
    f.cx.run_until_parked();
    answer_jumps(f, from, places);
    jump(f)
}

fn jump(f: &mut Fixture) -> flea::jsondoc::Json {
    let reply = endpoint(f, "snapshot", &[]);
    flea::jsondoc::parse(&reply.stdout)
        .unwrap()
        .get("jump")
        .cloned()
        .unwrap()
}

fn number(state: &flea::jsondoc::Json, key: &str) -> Option<f64> {
    state.get(key).and_then(|v| v.as_f64())
}

// The open dropdown: a typed name drops the folders it does not match, keeps the answer's order for equal matches, and Down steps the cursor.
#[gpui::test]
fn fixreaders_picker_snapshot_reads_the_open_jump_dropdown(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    let places = r#""favourites":["/d/subway","/d/Beta","/d/Alpha","/d/Altos","/d/Alder"],"zoxide":[],"recent":[]"#;
    let open = typed(&mut f, places, "al");
    assert_eq!(
        open.get("open").and_then(|v| v.as_bool()),
        Some(true),
        "{open:?}"
    );
    assert_eq!(number(&open, "rows"), Some(3.), "{open:?}");
    assert_eq!(number(&open, "visibleRows"), Some(3.), "{open:?}");
    let paths: Vec<_> = open
        .get("paths")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    // Equal matches keep the answer's order, which is neither ascending nor descending.
    assert_eq!(paths, ["/d/Alpha", "/d/Altos", "/d/Alder"]);
    assert_eq!(number(&open, "cursor"), Some(0.), "{open:?}");
    f.cx.simulate_keystrokes("down");
    f.cx.run_until_parked();
    assert_eq!(number(&jump(&mut f), "cursor"), Some(1.));
}

// A dropdown taller than the window draws only the rows that fit, and walking to the last row scrolls it.
#[gpui::test]
fn fixreaders_picker_snapshot_reads_a_scrolled_jump_dropdown(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    let source = |tag: &str| {
        (0..rank_rows())
            .map(|at| format!(r#""/d/Al{tag}{at}""#))
            .collect::<Vec<_>>()
            .join(",")
    };
    let places = format!(
        r#""favourites":["/d/subway",{}],"zoxide":[{}],"recent":[{}]"#,
        source("f"),
        source("z"),
        source("r")
    );
    let open = typed(&mut f, &places, "al");
    let rows = number(&open, "rows").unwrap();
    assert_eq!(rows, (3 * rank_rows()) as f64, "{open:?}");
    assert_eq!(number(&open, "contentY"), Some(0.), "{open:?}");
    assert_eq!(number(&open, "visibleRows"), Some(rows), "{open:?}");
    let (width, height) = SHORT_WINDOW;
    f.cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
    f.cx.run_until_parked();
    for _ in 1..3 * rank_rows() {
        f.cx.simulate_keystrokes("down");
    }
    f.cx.run_until_parked();
    let walked = jump(&mut f);
    assert_eq!(number(&walked, "cursor"), Some(rows - 1.), "{walked:?}");
    let drawn = number(&walked, "visibleRows").unwrap();
    assert!(drawn > 0. && drawn < rows, "{walked:?}");
    assert!(
        number(&walked, "contentY").is_some_and(|y| y > 0.),
        "{walked:?}"
    );
}

// seed's width, and a height that clips the 15 rows yet still draws some.
const SHORT_WINDOW: (f32, f32) = (1100., 260.);

fn rank_rows() -> usize {
    crate::path::rank::SOURCE_ROWS
}
