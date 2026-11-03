//! EY-313, EY-314, EY-272 and EY-303 (no name starts with g, so type-to-find cannot stand in for g or G): the chooser's empty hero and its g g, G, s, S, period and page keys in List view, as v0.3.12 draws and answers them.
use super::{
    process::Line,
    tests::fixture::{fixture, Fixture},
};
use gpui::TestAppContext;

const THREE: &str = r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"alpha.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"},{"n":"beta.md","d":false,"s":9,"m":5,"p":33188,"i":"text-plain"},{"n":"delta.txt","d":false,"s":1,"m":2,"p":33188,"i":"text-plain"}]}"#;

pub(super) fn draw(f: &mut Fixture) {
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    f.cx.run_until_parked();
}

// Open File on /d holding three files, drawn in List view.
pub(super) fn three(cx: &mut TestAppContext) -> Fixture<'_> {
    three_for(cx, r#"{"folder":"/d"}"#)
}

fn three_for<'a>(cx: &'a mut TestAppContext, request: &str) -> Fixture<'a> {
    let mut f = fixture(cx, request, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":3,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(THREE.into()));
    f.cx.run_until_parked();
    f.update(|picker, cx| picker.user_set_view("list", cx));
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    draw(&mut f);
    assert_eq!(f.read(|picker| picker.listing.total), 3);
    f
}

fn order(f: &mut Fixture) -> (&'static str, bool) {
    let order = f.update(|picker, cx| picker.browse.listing.read(cx).listing().order());
    (order.key, order.desc)
}

// The last list request the chooser sent after `from`.
fn list_line(f: &Fixture, from: usize) -> String {
    f.listing.sent()[from..]
        .iter()
        .rev()
        .find(|line| line.contains(r#""c":"list""#))
        .cloned()
        .unwrap_or_default()
}

#[gpui::test]
fn ey455_j_and_k_walk_the_focused_list(cx: &mut TestAppContext) {
    let mut f = three(cx);
    f.cx.simulate_keystrokes("j");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "j moves onto beta.md"
    );
    f.cx.simulate_keystrokes("k");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        0,
        "k returns to alpha.txt"
    );
}

#[gpui::test]
fn ey314_g_goes_to_the_first_row_and_shift_g_to_the_last(cx: &mut TestAppContext) {
    let mut f = three(cx);
    f.cx.simulate_keystrokes("end");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        2,
        "End lands on delta.txt"
    );
    f.cx.simulate_keystrokes("g");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        0,
        "keys.toml's default g is First item"
    );
    f.cx.simulate_keystrokes("shift-g");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        2,
        "G goes to the last row"
    );
}

#[gpui::test]
fn ey314_vim_g_g_arms_then_goes_to_the_first_row(cx: &mut TestAppContext) {
    let mut f = three(cx);
    f.update(|picker, _| picker.chrome.preset = "vim".into());
    f.cx.simulate_keystrokes("end");
    f.cx.simulate_keystrokes("g");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        2,
        "the vim preset's first g only arms the chord"
    );
    f.cx.simulate_keystrokes("g");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        0,
        "g g goes to the first row"
    );
}

#[gpui::test]
fn ey272_s_and_shift_s_cycle_the_list_sort(cx: &mut TestAppContext) {
    let mut f = three(cx);
    assert_eq!(order(&mut f), ("name", false));
    let mut seen = Vec::new();
    for _ in 0..3 {
        let from = f.listing.sent().len();
        f.cx.simulate_keystrokes("s");
        seen.push((order(&mut f), list_line(&f, from).is_empty()));
    }
    assert_eq!(
        seen,
        [
            (("size", false), false),
            (("mtime", false), false),
            (("name", false), false)
        ],
        "s steps Size, Modified, Name and re-lists each time, PickerList.qml sortNext"
    );
    f.cx.simulate_keystrokes("shift-s");
    assert_eq!(
        order(&mut f),
        ("name", true),
        "S reverses the order in place"
    );
}

#[gpui::test]
fn ey272_period_shows_hidden_files_in_the_list(cx: &mut TestAppContext) {
    let mut f = three(cx);
    let from = f.listing.sent().len();
    f.cx.simulate_keystrokes(".");
    let line = list_line(&f, from);
    assert!(
        line.contains(r#""path":"/d""#) && line.contains(r#""hidden":true"#),
        "the period re-lists the folder with hidden files: {line:?}"
    );
    let from = f.listing.sent().len();
    f.cx.simulate_keystrokes(".");
    assert!(
        list_line(&f, from).contains(r#""hidden":false"#),
        "a second period hides them again"
    );
    assert!(
        f.read(|picker| picker.chrome.history.is_empty()),
        "re-listing the same folder adds no Back step, as openWithoutHistory does"
    );
}

fn filter(f: &mut Fixture, rows: &[usize]) {
    let rows = std::rc::Rc::from(rows);
    f.update(|picker, cx| {
        picker
            .browse
            .listing
            .update(cx, |view, _| view.set_filtered(Some(rows), "t", None))
    });
}

#[gpui::test]
fn ey314_g_and_shift_g_land_on_rows_a_filter_shows(cx: &mut TestAppContext) {
    let mut f = three(cx);
    filter(&mut f, &[1, 2]);
    f.cx.simulate_keystrokes("end");
    f.cx.simulate_keystrokes("g");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "g stops on beta.md, the first shown row, never on the hidden alpha.txt"
    );
    filter(&mut f, &[0, 1]);
    f.cx.simulate_keystrokes("home");
    assert_eq!(f.read(|picker| picker.listing.cursor), 0);
    f.cx.simulate_keystrokes("shift-g");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "G stops on beta.md, the last shown row, never on the hidden delta.txt"
    );
}

#[gpui::test]
fn ey314_g_g_and_the_page_keys_end_a_shift_range(cx: &mut TestAppContext) {
    let mut f = three_for(cx, r#"{"folder":"/d","multiple":true}"#);
    for key in ["g", "shift-g", "pagedown", "pageup"] {
        f.cx.simulate_keystrokes("home");
        f.cx.simulate_keystrokes("shift-down");
        assert!(
            f.read(|picker| picker.range_anchor()).is_some(),
            "Shift+Down opens a range"
        );
        f.cx.simulate_keystrokes(key);
        assert_eq!(
            f.read(|picker| picker.range_anchor()),
            None,
            "{key} moves without extending, so PickerList.qml moveCursor ends the range"
        );
    }
}

#[gpui::test]
fn ey313_the_hero_stands_over_an_empty_folder_and_leaves_with_it(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":0,"v":1,"w":true}"#.into(),
    ));
    f.cx.run_until_parked();
    draw(&mut f);
    assert_eq!(f.read(|picker| picker.listing.state), "empty");
    assert!(
        f.cx.debug_bounds("picker-empty-hero").is_some(),
        "PickerWindow.qml's EmptyState stands over an empty listing"
    );
    let viewport = f.cx.update(|window, _| window.viewport_size());
    let caption = f.cx.debug_bounds("fb1-hero-caption").unwrap();
    assert!(
        caption.bottom() <= viewport.height && caption.right() <= viewport.width,
        "the hero's caption {caption:?} draws inside the {viewport:?} window, over the listing and not below it"
    );
    let mark = f.cx.debug_bounds("fb1-hero-mark").unwrap();
    assert!(
        f.cx.update(|window, _| window.painted_quads().iter().any(|quad| {
            quad.bounds
                .is_contained_within(&mark.scale(window.scale_factor()))
        })),
        "the empty hero paints its mark, beyond reserving its slot"
    );
    f.update(|picker, cx| picker.open_folder("/e", cx));
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/e","n":3,"v":2,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(
        THREE.replace(r#""listing":1"#, r#""listing":2"#),
    ));
    f.cx.run_until_parked();
    draw(&mut f);
    assert_eq!(f.read(|picker| picker.listing.total), 3);
    assert!(
        f.cx.debug_bounds("picker-empty-hero").is_none(),
        "the same chooser drops the hero once the folder it shows has rows"
    );
}

#[gpui::test]
fn ey303_page_down_after_home_goes_to_the_last_row_and_enables_open(cx: &mut TestAppContext) {
    let mut f = three(cx);
    f.cx.simulate_keystrokes("home");
    assert_eq!(f.read(|picker| picker.listing.cursor), 0);
    f.cx.simulate_keystrokes("pagedown");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        2,
        "PickerList.qml moveCursor(visibleRows) clamps to the last row"
    );
    assert!(
        f.read(|picker| picker.can_accept()),
        "delta.txt under the cursor makes Open available"
    );
    f.cx.simulate_keystrokes("pageup");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        0,
        "Page_Up clamps to the first row"
    );
}

// More rows than two pages, so Page_Down twice never reaches the end.
const FOLDER_ROWS: usize = 200;
// A page of one row could not tell a page step from a row step.
const MIN_PAGE: usize = 2;

// Open File on /d holding `n` files named r000.txt up, drawn in List view.
fn many(cx: &mut TestAppContext, n: usize) -> Fixture<'_> {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","path":"/d","n":{n},"v":1,"w":true}}"#
    )));
    let rows: Vec<String> = (0..n)
        .map(|at| {
            format!(r#"{{"n":"r{at:03}.txt","d":false,"s":1,"m":0,"p":33188,"i":"text-plain"}}"#)
        })
        .collect();
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"listing":1,"rows":[{}]}}"#,
        rows.join(",")
    )));
    f.cx.run_until_parked();
    f.update(|picker, cx| picker.user_set_view("list", cx));
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    draw(&mut f);
    draw(&mut f);
    assert_eq!(f.read(|picker| picker.listing.total), n);
    f
}

#[gpui::test]
fn ey303_a_page_steps_by_the_rows_the_list_shows(cx: &mut TestAppContext) {
    let mut f = many(cx, FOLDER_ROWS);
    let listing = f.read(|picker| picker.browse.listing.clone());
    // The rows the list painted at the top, a part row included, are PickerList.qml's visibleRows.
    let page =
        f.cx.update(|_, cx| listing.read(cx).list_state().borrow().drawn_names().len());
    assert!(
        (MIN_PAGE..FOLDER_ROWS / 2).contains(&page),
        "the drawn list shows a page of rows: {page}"
    );
    f.cx.simulate_keystrokes("pagedown");
    assert_eq!(f.read(|picker| picker.listing.cursor), page);
    f.cx.simulate_keystrokes("pagedown");
    assert_eq!(f.read(|picker| picker.listing.cursor), 2 * page);
    f.cx.simulate_keystrokes("pageup");
    assert_eq!(f.read(|picker| picker.listing.cursor), page);
}

#[gpui::test]
fn ey303_a_page_walks_the_rows_a_filter_shows(cx: &mut TestAppContext) {
    let mut f = three(cx);
    filter(&mut f, &[0, 1]);
    f.cx.simulate_keystrokes("pagedown");
    assert_eq!(
        f.read(|picker| picker.listing.cursor),
        1,
        "Page_Down stops on beta.md, the last shown row, never on the hidden delta.txt"
    );
}
