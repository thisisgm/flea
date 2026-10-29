//! Review round 3 proofs: a multiline first screen draws whole, a late column costs its first screen, a sideways
//! frame draws only what it shows, and ragged rows draw their missing cells empty.
use super::*;

// Three-line records, 25 bodies: the first reply's line-counted ask holds only part of the first screen, and body 20
// carries the wide name and the text code the partial screen never sees.
#[gpui::test]
fn pv2_review_r3_first_screen_of_multiline_records_draws_whole_cells(cx: &mut TestAppContext) {
    let name = "abcdefghijklmnopqrstuvwxy1234";
    let mut text = String::from("name,code,city\n");
    for i in 1..=25 {
        if i == 20 {
            text.push_str(&format!("{name},n/a,\"gg\nhh\nii\"\n"));
        } else {
            text.push_str(&format!("{i},100,\"aa\nbb\ncc\"\n"));
        }
    }
    let (view, sent) = table(cx, true, 1100., 700.);
    view.update(cx, |v, _, cx| v.set_path("/d/notes.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let wide: Vec<(String, bool, bool)> = read(cx, view, |v| {
        v.rows
            .iter()
            .find(|row| row.record == 21)
            .expect("record 21 is drawn")
            .cells
            .iter()
            .map(|c| (c.text.to_string(), c.right, c.cut))
            .collect()
    });
    println!("REV-R3 multiline first screen: record 21 draws {wide:?}");
    assert_eq!(
        wide[0].0, name,
        "the 29-character name draws whole, not cut to the partial screen's box"
    );
    assert_eq!(
        wide[1],
        ("n/a".to_string(), false, false),
        "the text code stays left-aligned"
    );
}

// Forty columns, 2,500 rows: the last column is short and numeric on the first screen and long text after it. After
// scrolling down, one frame to the right measures the late columns over the first screen's 48 rows, never the held.
#[gpui::test]
fn pv2_review_r3_sideways_after_scrolling_down_measures_the_first_screen(cx: &mut TestAppContext) {
    const COLS: usize = 40;
    let mut text = (0..COLS)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    for r in 0..2500 {
        let row = (0..COLS)
            .map(|c| {
                if c == COLS - 1 && r >= 48 {
                    "y".repeat(40)
                } else {
                    ((r + c) % 10).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        text.push_str(&row);
        text.push('\n');
    }
    let (view, sent) = table(cx, true, 1100., 700.);
    view.update(cx, |v, _, cx| v.set_path("/d/far.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    view.update(cx, |v, _, cx| {
        v.feed.margin = 200;
        v.scroll_to_row(2000, cx);
    })
    .unwrap();
    serve(cx, view, &sent, &text);
    let (reached, measured) = read(cx, view, |v| {
        (v.feed.widths.len(), v.feed.counters.measured)
    });
    assert!(
        (1..COLS).contains(&reached),
        "the first screen reaches some columns but not all"
    );
    view.update(cx, |v, _, cx| {
        v.scroll.0 = 1e9;
        cx.notify()
    })
    .unwrap();
    draw(cx, view);
    let (delta, width, right) = read(cx, view, |v| {
        (
            v.feed.counters.measured - measured,
            v.feed.widths[COLS - 1],
            v.feed.rights[COLS - 1],
        )
    });
    let late = COLS - reached;
    println!("REV-R3 sideways after scroll down: {late} late columns measured as {delta} cells; far width {width}, right {right}");
    assert_eq!(
        delta,
        late * 49,
        "one frame to the right measures its columns over the first screen's 48 rows"
    );
    assert_eq!(
        (width, right),
        (22.0, true),
        "the far column keeps the first screen's width and alignment"
    );
}

// Twenty rows of 2,000-character notes: every 40 px sideways step draws at most the frame's characters, and the
// notes' ends stay reachable. Drawn characters are the deterministic pin for glyph lookups per frame: paint looks each
// drawn glyph up once, so the frame's lookups follow its characters, never the cells' whole length. The bound reads
// the same advance('0') the view shapes with, since the test platform's advance is not the board font's.
fn advance_px(cx: &mut TestAppContext, view: WindowHandle<CsvView>) -> f32 {
    view.update(cx, |v, window, _| {
        let text = window.text_system();
        let id = text.resolve_font(&v.face.font);
        text.advance(id, v.face.caption, '0')
            .map_or(f32::from(v.face.caption) * 0.6, |a| f32::from(a.width))
            .max(1.)
    })
    .unwrap()
}

#[gpui::test]
fn pv2_review_r3_sideways_frame_over_long_cells_draws_only_what_it_shows(cx: &mut TestAppContext) {
    let mut seed = 0x12345678u64;
    let note: String = (0..2000)
        .map(|_| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + (seed >> 33) as u8 % 26) as char
        })
        .collect();
    let mut text = String::from("id,note\n");
    for i in 1..=20 {
        text.push_str(&format!("{i},{note}END\n"));
    }
    let (view, sent) = table(cx, true, 1100., 700.);
    view.update(cx, |v, _, cx| v.set_path("/d/long.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let adv = advance_px(cx, view);
    let budgeted = (1100. / adv).ceil() as usize + 2;
    let mut windows = Vec::new();
    for step in [0., 40., 80., 120., 160.] {
        view.update(cx, |v, _, cx| {
            v.scroll.0 = step;
            cx.notify()
        })
        .unwrap();
        draw(cx, view);
        let (total, longest) = read(cx, view, |v| {
            let mut total = 0;
            let mut longest = String::new();
            for row in &v.rows {
                for c in &row.cells {
                    total += c.text.chars().count();
                    if c.text.chars().count() > longest.chars().count() {
                        longest = c.text.to_string();
                    }
                }
            }
            (total, longest)
        });
        println!(
            "REV-R3 long cells at {step}px, advance {adv}: {total} characters drawn, longest {}",
            longest.chars().count()
        );
        assert!(
            longest.chars().count() <= budgeted,
            "a drawn cell holds {} characters though the frame shows {budgeted}",
            longest.chars().count()
        );
        assert!(
            total <= 20 * budgeted + 200,
            "a 40 px step draws {total} characters though the frame shows about {}",
            20 * budgeted
        );
        windows.push(longest);
    }
    // Consecutive windows track the scroll: they differ, and each new head was visible in the old window.
    assert_ne!(
        windows[0], windows[4],
        "a 160 px step moves the drawn window"
    );
    assert!(
        windows[0].contains(&windows[4][..10]),
        "the new window's head was visible before the step"
    );
    let tail: String = windows[0]
        .chars()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    assert!(
        windows[4].contains(&tail),
        "the old window's tail stays visible after the step"
    );
    view.update(cx, |v, _, cx| {
        v.scroll.0 = 1e9;
        cx.notify()
    })
    .unwrap();
    draw(cx, view);
    let tail = read(cx, view, |v| {
        v.drawn_texts()
            .map(str::to_string)
            .find(|t| t.ends_with("END"))
    });
    assert!(
        tail.is_some(),
        "the long cells' ends stay reachable at the right end"
    );
}

// A short row draws its missing cells empty and a long row draws its extra cell: the contract padding gave the view.
#[gpui::test]
fn pv2_review_r3_ragged_rows_draw_missing_cells_empty(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/ragged.csv", cx))
        .unwrap();
    serve(cx, view, &sent, "a,b,c,d\nshort\n1,2,3,extra\n");
    let rows: Vec<Vec<String>> = read(cx, view, |v| {
        v.rows
            .iter()
            .map(|row| row.cells.iter().map(|c| c.text.to_string()).collect())
            .collect()
    });
    println!("REV-R3 ragged rows draw {rows:?}");
    assert_eq!(
        rows,
        vec![vec!["short", "", "", ""], vec!["1", "2", "3", "extra"]],
        "a short row draws its missing cells empty, and each column holds its widest cell whole (EY-47)"
    );
}

// EY-47: Quick Look sizes each column to its widest cell, so a body cell longer than its header draws whole.
#[gpui::test]
fn pv2_ey47_quick_look_columns_fit_their_widest_cell(cx: &mut TestAppContext) {
    let text = "name,kind,size\nimage,png,320x240\nvideo,mp4,3s\n";
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/text-csv.csv", cx))
        .unwrap();
    serve(cx, view, &sent, text);
    let cells: Vec<(String, bool)> = read(cx, view, |v| {
        v.rows
            .iter()
            .flat_map(|r| r.cells.iter().map(|c| (c.text.to_string(), c.cut)))
            .collect()
    });
    for whole in ["image", "video", "320x240"] {
        assert!(
            cells.iter().any(|(t, cut)| t == whole && !cut),
            "{whole} draws whole: {cells:?}"
        );
    }
}
