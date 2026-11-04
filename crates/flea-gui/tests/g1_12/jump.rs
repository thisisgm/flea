//! The folder jump behind the bar, through a test ranker, and the popup's board geometry, clip and pointer rule.
use flea_gui::{
    list::Typeface,
    path::{
        menu::{cuts, layers},
        scale::Scale,
        suggestions::{step, Ask, Enter, Jump, Pointer, Popup, Ranker, Suggestion},
        Jumped,
    },
};

/// The geometry at the board's text size, body 14, where every number is the board's pixel.
pub fn board() -> Scale {
    Scale::at(&Typeface::omarchy_default())
}

/// A stand-in scorer for the seam: every answered folder whose leaf holds the typed name, in answer order.
#[derive(Default)]
pub struct Folders {
    candidates: Vec<String>,
    pub prepared: usize,
}

impl Ranker for Folders {
    fn prepare(&mut self, answer: &Jumped, _: &str) {
        self.prepared += 1;
        self.candidates.clear();
        for path in answer
            .favourites
            .iter()
            .chain(&answer.zoxide)
            .chain(&answer.recent)
        {
            if !self.candidates.contains(path) {
                self.candidates.push(path.clone());
            }
        }
    }

    fn rows(&self, line: &str, out: &mut Vec<Suggestion>) {
        if !self.is_query(line) {
            return;
        }
        let needle = line.to_lowercase();
        for path in &self.candidates {
            let leaf_start = path.rfind('/').map_or(0, |at| at + 1);
            if let Some(at) = path[leaf_start..].to_lowercase().find(&needle) {
                let start = leaf_start + at;
                out.push(Suggestion {
                    path: path.clone(),
                    text: path.clone(),
                    leaf_start,
                    wash: start..start + line.len(),
                });
            }
        }
    }

    fn is_query(&self, line: &str) -> bool {
        !line.is_empty() && !line.contains('/') && !line.starts_with('~')
    }
}

pub fn answer(id: u64, zoxide: &[&str]) -> Jumped {
    Jumped {
        id,
        favourites: vec!["/fx/v/work/Documents".into()],
        zoxide: zoxide.iter().map(|path| path.to_string()).collect(),
        ..Jumped::default()
    }
}

fn ids(asks: &[Ask]) -> Vec<(u64, u64)> {
    asks.iter()
        .filter_map(|ask| match ask {
            Ask::Jump { id, ranking } => Some((*id, *ranking)),
            Ask::ReadHistory => None,
        })
        .collect()
}

#[test]
fn a_stale_history_asks_twice_and_a_fresh_one_asks_once() {
    let (mut jump, mut ranker, mut out) = (Jump::default(), Folders::default(), Vec::new());
    jump.open(&mut out);
    assert_eq!(out, [Ask::Jump { id: 1, ranking: 0 }, Ask::ReadHistory]);
    out.clear();
    assert_eq!(
        jump.take(
            &answer(1, &["/fx/v/work/Media"]),
            &mut ranker,
            "/h",
            "o",
            &mut out
        ),
        None
    );
    assert!(out.is_empty(), "the whole ask waits for the history read");
    assert_eq!(
        jump.entries.len(),
        1,
        "the provisional answer ranks at once"
    );
    jump.history_read(&mut out);
    assert_eq!(ids(&out), [(2, 1)]);
    out.clear();
    jump.take(
        &answer(2, &["/fx/v/work/Media", "/srv/omarchy"]),
        &mut ranker,
        "/h",
        "o",
        &mut out,
    );
    assert_eq!(
        jump.entries
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        ["/fx/v/work/Documents", "/srv/omarchy"]
    );
    assert_eq!(
        jump.take(&answer(1, &["/old/o"]), &mut ranker, "/h", "o", &mut out),
        None
    );
    assert_eq!(
        ranker.prepared, 2,
        "a provisional answer landing after the whole one is dropped like a stale one"
    );
    jump.close();
    assert!(!jump.shown());
    jump.open(&mut out);
    assert_eq!(out.len(), 1, "a kept history asks once");
    assert!(matches!(out[0], Ask::Jump { ranking: 0, .. }));
    jump.history_changes += 1;
    out.clear();
    jump.open(&mut out);
    assert_eq!(out.len(), 2, "a changed history is read again");
}

#[test]
fn an_answer_for_another_open_or_a_closed_bar_is_dropped() {
    let (mut jump, mut ranker, mut out) = (Jump::default(), Folders::default(), Vec::new());
    jump.open(&mut out);
    jump.close();
    assert_eq!(
        jump.take(&answer(1, &["/a/o"]), &mut ranker, "/h", "o", &mut out),
        None
    );
    assert!(!jump.shown() && ranker.prepared == 0);
    jump.open(&mut out);
    assert_eq!(
        jump.take(&answer(1, &["/a/o"]), &mut ranker, "/h", "o", &mut out),
        None
    );
    assert!(
        !jump.shown() && ranker.prepared == 0,
        "the first open's answer is not the second's"
    );
}

#[test]
fn enter_on_a_name_holds_for_the_whole_answer_and_never_spends_on_a_provisional_one() {
    let (mut jump, mut ranker, mut out) = (Jump::default(), Folders::default(), Vec::new());
    jump.open(&mut out);
    assert_eq!(jump.press_enter(&ranker, "Med"), Some(Enter::Held));
    assert!(jump.enter_waiting);
    out.clear();
    jump.history_read(&mut out);
    assert_eq!(
        jump.take(
            &answer(1, &["/fx/v/work/Media"]),
            &mut ranker,
            "/h",
            "Med",
            &mut out
        ),
        None
    );
    assert!(
        jump.enter_waiting,
        "the provisional answer never spends a held Enter"
    );
    let (id, _) = ids(&out)[0];
    let chosen = jump.take(
        &answer(id, &["/fx/v/work/Media"]),
        &mut ranker,
        "/h",
        "Med",
        &mut out,
    );
    assert_eq!(chosen, Some(Enter::Chosen("/fx/v/work/Media".into())));
    assert!(!jump.enter_waiting);
    jump.open(&mut out);
    assert_eq!(jump.press_enter(&ranker, "Zz"), Some(Enter::Held));
    assert!(
        jump.expire(),
        "the answer limit turns the held Enter into the line"
    );
    assert!(!jump.expire());
    assert_eq!(
        jump.press_enter(&ranker, "/etc"),
        None,
        "a path is the line's own Enter"
    );
}

#[test]
fn keyboard_steps_clamp_and_tab_turns_the_line_into_a_path() {
    let (mut jump, mut ranker, mut out) = (Jump::default(), Folders::default(), Vec::new());
    jump.open(&mut out);
    jump.history_read(&mut out);
    jump.take(
        &answer(1, &["/a/one", "/b/done", "/c/ozone"]),
        &mut ranker,
        "/h",
        "o",
        &mut out,
    );
    assert_eq!((jump.entries.len(), jump.cursor), (4, Some(0)));
    assert!(jump.step(1));
    assert_eq!(jump.enter(), Enter::Chosen("/a/one".into()));
    assert!(jump.step(9));
    assert_eq!(jump.cursor, Some(3));
    assert!(!jump.step(1));
    assert!(jump.step(-9));
    assert_eq!(
        jump.press_enter(&ranker, "o"),
        Some(Enter::Chosen("/fx/v/work/Documents".into()))
    );
    jump.rank(&ranker, "zz");
    assert_eq!(
        (jump.shown(), jump.cursor, jump.high_water),
        (false, None, 4)
    );
    assert_eq!(jump.enter(), Enter::Declined);
    jump.rank(&ranker, "o");
    jump.path_typed();
    assert!(!jump.shown());
    jump.rank(&ranker, "on");
    assert!(!jump.shown(), "a line Tab touched stays a path");
    assert_eq!(jump.press_enter(&ranker, "on"), None);
    assert_eq!(
        (step(0, None, 1), step(3, None, 1), step(3, Some(1), -5)),
        (None, Some(0), Some(0))
    );
}

#[test]
fn the_popup_sits_three_pixels_under_the_rule_and_nineteen_over_the_status_bar() {
    let s = board();
    let tall = Popup::measure(s, 700., 80., 788., 7);
    assert_eq!(
        (tall.x, tall.y, tall.width, tall.height),
        (80., 30., 788., 271.)
    );
    assert_eq!(tall.y - s.chrome, 3., "3 px below the toolbar rule");
    assert_eq!(tall.max_scroll(), 0.);
    assert_eq!(tall.fades(0.), (false, false));
    let short = Popup::measure(s, 360., 80., 788., 9);
    assert_eq!(
        (short.x, short.y, short.width, short.height),
        (80., 30., 788., 284.)
    );
    assert_eq!(short.y + short.height, 314.);
    assert_eq!(360. - s.chrome, 333.);
    assert_eq!(360. - s.chrome - (short.y + short.height), 19.);
    assert_eq!(short.max_scroll(), 61.);
}

#[test]
fn the_clipped_row_shows_exactly_half_and_all_of_it_under_the_fade() {
    let s = board();
    let (row, pad) = (s.row, s.pad);
    let popup = Popup::measure(s, 360., 80., 788., 9);
    let half = (row / 2.).floor();
    assert_eq!(popup.visible(0., 9), (0, 8));
    let bottom_row_top = pad + 7. * row;
    assert_eq!(popup.inner - bottom_row_top, half, "half bottom row");
    assert!(
        popup.inner - s.fade <= bottom_row_top,
        "the clipped half sits wholly under the fade"
    );
    assert_eq!(popup.fades(0.), (false, true));
    let end = popup.reveal(0., 8, 9);
    assert_eq!(
        end,
        popup.max_scroll(),
        "End shows the last row and its padding"
    );
    assert_eq!(popup.visible(end, 9), (1, 9));
    let top_row_bottom = pad + 2. * row;
    assert_eq!(top_row_bottom - end, half, "half top row");
    assert!(top_row_bottom - end <= s.fade);
    assert_eq!(popup.fades(end), (true, false));
    assert_eq!(
        popup.reveal(end, 0, 9),
        0.,
        "the first row brings its padding"
    );
    assert_eq!(popup.reveal(0., 3, 9), 0.);
    assert_eq!(popup.reveal(0., 7, 9), pad + 8. * row - popup.inner);
    let (first, last) = popup.visible(30., 9);
    assert!(
        last - first <= (popup.inner / row).ceil() as usize + 1,
        "only the visible rows are shaped"
    );
}

#[test]
fn a_press_reads_the_row_under_it_and_padding_reads_none() {
    let s = board();
    let popup = Popup::measure(s, 360., 80., 788., 9);
    let y = |content: f32| 30. + 1. + content;
    assert_eq!(
        popup.row_at(100., y(s.pad + 2. * s.row + 10.), 0., 9),
        Some(2)
    );
    assert_eq!(
        popup.row_at(100., y(s.pad + 2. * s.row + 10.), 61., 9),
        Some(3),
        "61 px of scroll moves the press down one row"
    );
    assert_eq!(popup.row_at(100., y(2.), 0., 9), None, "the top padding");
    assert_eq!(popup.row_at(79., y(50.), 0., 9), None);
    assert_eq!(popup.row_at(100., 30.5, 0., 9), None, "the frame");
    assert!(popup.contains(80., 30.) && !popup.contains(868., 30.));
}

#[test]
fn rows_appearing_under_a_resting_pointer_never_take_the_cursor() {
    let mut pointer = Pointer::default();
    pointer.appeared();
    assert!(
        !pointer.moved((100., 120.)),
        "the first sighting is where it already was"
    );
    assert!(
        !pointer.moved((100., 121.)),
        "still settling under the new rows"
    );
    pointer.settling = false;
    assert!(!pointer.moved((100., 121.)));
    assert!(pointer.moved((100., 160.)));
}

#[test]
fn the_cursor_row_bolds_its_match_run_with_no_wash_inside_the_band() {
    let row = Suggestion {
        path: "/fx/v/work/Media".into(),
        text: "/fx/v/work/Media".into(),
        leaf_start: 11,
        wash: 11..14,
    };
    let cursor = layers(&row, true);
    assert!(cursor.band && cursor.bold_run && !cursor.run_wash);
    let other = layers(&row, false);
    assert!(!other.band && !other.bold_run && other.run_wash);
    let plain = Suggestion {
        wash: 0..0,
        ..row.clone()
    };
    assert_eq!(
        (layers(&plain, true).run_wash, layers(&plain, true).bold_run),
        (false, false)
    );
    assert_eq!(cuts(&row), [0, 11, 11, 14, 16]);
    assert_eq!(cuts(&plain), [0, 11, 16, 16, 16]);
}
