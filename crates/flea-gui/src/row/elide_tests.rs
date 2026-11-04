//! EY-437: v0.3.12 MatchText keeps a late match run in view; the cut moves before the run, inside the name's cells.
use super::elide_around;
use crate::row::cache::{Extra, Shaping};
use crate::row::cell_count as cells;
use flea::protocol::RowFacts;

const LATE: &str = "a-long-file-name-with-early-words-and-late-punctuation-[native2-late].txt";
const RUN: &str = "[native2-late]";

fn run_of(text: &str, needle: &str) -> (usize, usize) {
    let start = text.find(needle).expect("the needle is in the text");
    (start, start + needle.len())
}

#[test]
fn ey437_a_late_run_stays_whole_after_a_cut_before_it() {
    // 30 cells is v0.3.12's own text, which there ran on over the size column.
    for (max, expected) in [
        (74, LATE),
        (30, "a-long-file-nam\u{2026}[native2-late]"),
        (24, "a-long-fi\u{2026}[native2-late]"),
        (20, "a-lon\u{2026}[native2-late]"),
        (16, "a\u{2026}[native2-late]"),
        (15, "\u{2026}[native2-late]"),
        (14, "\u{2026}[native2-lat\u{2026}"),
    ] {
        let (drawn, (start, end)) = elide_around(LATE, max, run_of(LATE, RUN));
        assert_eq!(drawn, expected, "{max} cells");
        assert!(cells(&drawn) <= max, "{max} cells hold {drawn}");
        let shown = if max > 14 { RUN } else { "[native2-lat" };
        assert_eq!(&drawn[start..end], shown, "{max} cells");
    }
}

#[test]
fn ey437_a_run_in_the_head_keeps_elide_right() {
    let name = "notes-from-the-meeting.txt";
    assert_eq!(
        elide_around(name, 10, (0, 5)),
        ("notes-fro\u{2026}".into(), (0, 5))
    );
    assert_eq!(
        elide_around(name, 10, (6, 9)),
        ("notes-fro\u{2026}".into(), (6, 9))
    );
    assert_eq!(elide_around(name, 40, (15, 22)), (name.into(), (15, 22)));
    // A run the head only half holds moves the cut before it rather than washing half a run.
    assert_eq!(
        elide_around(name, 18, run_of(name, "meeting")),
        ("notes-from\u{2026}meeting".into(), (13, 20))
    );
    // A whole head that fills the room gives a cell back, so the cut tail keeps its ellipsis.
    assert_eq!(
        elide_around("ab[x]cd", 5, (2, 5)),
        ("\u{2026}[x]\u{2026}".into(), (3, 6))
    );
    assert_eq!(
        elide_around("[native2-late]-tail", 8, (0, 14)),
        ("[native\u{2026}".into(), (0, 7))
    );
}

#[test]
fn ey437_every_budget_stays_inside_its_cells_on_char_bounds() {
    let names = [
        (LATE, RUN),
        (
            "\u{4f60}\u{597d}-\u{754c}\u{9762}-\u{4f60}\u{597d}-[late]-\u{754c}.txt",
            "[late]",
        ),
        (
            "e\u{301}e\u{301}e\u{301}e\u{301}-match-e\u{301}e\u{301}",
            "match",
        ),
        ("short-[x]", "[x]"),
    ];
    for (name, needle) in names {
        for max in 0..=cells(name) + 2 {
            let (drawn, (start, end)) = elide_around(name, max, run_of(name, needle));
            assert!(cells(&drawn) <= max.max(1), "{name} at {max}: {drawn}");
            let shown = drawn.get(start..end).expect("the run sits on char bounds");
            assert!(needle.starts_with(shown), "{name} at {max}: {shown}");
            if max > cells(needle) {
                assert_eq!(shown, needle, "{name} at {max} keeps the run whole");
            }
        }
    }
}

#[test]
fn ey437_list_draws_a_late_filter_run_whole_inside_the_name() {
    let facts = RowFacts {
        name: LATE.into(),
        ..RowFacts::default()
    };
    let shaping = Shaping {
        name_cells: 24,
        clip_cells: 20,
        ..Shaping::default()
    };
    for (clipped, expected) in [
        (false, "a-long-fi\u{2026}[native2-late]"),
        (true, "a-lon\u{2026}[native2-late]"),
    ] {
        let extra = Extra {
            clipped,
            ..Extra::default()
        };
        let (drawn, run) = crate::list::element::drawn_name(&facts, extra, &shaping, RUN, None);
        assert_eq!(drawn, expected);
        let (start, end) = run.expect("the filter run is marked");
        assert_eq!(&drawn[start..end], RUN);
    }
}

/// Per keystroke: the List name of 300 filtered rows, the path every visible row runs when the query changes.
#[test]
#[ignore = "timing probe, run with --ignored --nocapture"]
fn ey437_drawn_name_cost_per_keystroke() {
    let rows: Vec<RowFacts> = (0..300)
        .map(|at| RowFacts {
            name: format!(
                "a-long-file-name-{at:03}-with-early-words-and-late-punctuation-[native2-late].txt"
            ),
            ..RowFacts::default()
        })
        .collect();
    let shaping = Shaping {
        name_cells: 24,
        clip_cells: 20,
        advance: 8.4,
        ..Shaping::default()
    };
    // A monospace stand-in for the shaper, so the probe times the glyph fit's search and not the font.
    let width = |text: &str| text.chars().count() as f32 * 8.4;
    for query in ["[native2-late]", "a-long", "zzz"] {
        let mut costs: Vec<f64> = (0..400)
            .map(|_| {
                let started = std::time::Instant::now();
                for facts in &rows {
                    std::hint::black_box(crate::list::element::drawn_name(
                        facts,
                        Extra::default(),
                        &shaping,
                        query,
                        Some(&width),
                    ));
                }
                started.elapsed().as_secs_f64() * 1e6
            })
            .collect();
        costs.drain(..50);
        costs.sort_by(f64::total_cmp);
        let at = |q: f64| costs[((costs.len() - 1) as f64 * q) as usize];
        eprintln!(
            "DRAWN_NAME query={query} rows=300 n={} median_us={:.1} p90_us={:.1}",
            costs.len(),
            at(0.5),
            at(0.9)
        );
    }
}
