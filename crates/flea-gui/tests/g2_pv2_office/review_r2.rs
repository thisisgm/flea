//! Review round 2 tests: a wide first screen measures only the columns the view reaches, Quick Look reaches a long
//! cell's end and the last column, and a converter error draws no sentence until the header says whether it is a lock.
use super::*;
use gpui::{point, Bounds};
use std::time::{Duration, Instant};

fn wide(cols: usize, rows: usize) -> String {
    let mut text = (1..=cols)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    let row = vec!["12"; cols].join(",");
    for _ in 0..rows {
        text.push_str(&row);
        text.push('\n');
    }
    text
}

// Every body cell a distinct number, so no measure is a line-cache hit.
fn distinct(cols: usize, rows: usize) -> String {
    let mut text = (1..=cols)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    for r in 0..rows {
        let row = (0..cols)
            .map(|c| (r * 1000 + c).to_string())
            .collect::<Vec<_>>()
            .join(",");
        text.push_str(&row);
        text.push('\n');
    }
    text
}

// serve, timing each layout that lands a parse: that layout measures the columns the frame reaches.
fn serve_timed(
    cx: &mut TestAppContext,
    view: WindowHandle<CsvView>,
    sent: &Sent,
    text: &str,
) -> Duration {
    let mut answered = 0;
    let mut worst = Duration::ZERO;
    for _ in 0..200 {
        draw(cx, view);
        let pending: Vec<String> = sent.borrow()[answered..].to_vec();
        if pending.is_empty() {
            break;
        }
        for ask in pending {
            answered += 1;
            let reply = answer(&ask, text);
            view.update(cx, |v, _, cx| v.answered(&reply, cx)).unwrap();
            cx.run_until_parked();
            let took = view
                .update(cx, |v, window, _| {
                    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(1100.), px(700.)));
                    let t = Instant::now();
                    v.layout(bounds, window);
                    t.elapsed()
                })
                .unwrap();
            worst = worst.max(took);
        }
    }
    worst
}

// A 1,000-column file opened in Quick Look: the frame measures the columns it reaches, not all of them.
#[gpui::test]
fn pv2_review_r2_wide_first_screen_measures_only_what_it_reaches(cx: &mut TestAppContext) {
    let text = distinct(1000, 60);
    let (view, sent) = table(cx, true, 1100., 700.);
    view.update(cx, |v, _, cx| v.set_path("/d/wide.csv", cx))
        .unwrap();
    let worst = serve_timed(cx, view, &sent, &text);
    let (head, measured) = read(cx, view, |v| (v.head.len(), v.feed.counters.measured));
    println!("REV-R2 first screen: 1000 columns, {} bytes; the slowest layout took {worst:?}, measured {measured} cells, drew {head} header cells", text.len());
    assert!(head <= 20, "{head} header cells drawn for a 1100 px frame");
    assert!(
        measured < 2_000,
        "{measured} cells measured for a frame that reaches {head} columns"
    );
    assert!(
        worst < Duration::from_micros(8_330),
        "a 1,000-column first screen held the UI thread for {worst:?}"
    );
}

// Quick Look reaches a long cell's end and the file's last column when scrolled to the right.
#[gpui::test]
fn pv2_review_r2_quick_look_reaches_a_long_cells_end_and_the_last_column(cx: &mut TestAppContext) {
    let long = format!("{}END", "abcdefghij".repeat(30));
    let text = format!("id,note\n1,{long}\n2,short\n");
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/notes.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    view.update(cx, |v, _, cx| {
        v.scroll.0 = 1e9;
        cx.notify()
    })
    .unwrap();
    draw(cx, view);
    let cells: Vec<String> = read(cx, view, |v| v.drawn_texts().map(str::to_string).collect());
    // The drawn cell may be the scrolled window rather than the whole cell, so the end is what names it.
    let note = cells
        .iter()
        .find(|t| t.ends_with("END"))
        .cloned()
        .unwrap_or_default();

    // The last header is named so its four-character cut is unique: every c10.. header cuts to the same "c10…".
    let mut text = wide(1001, 3);
    if let Some(pos) = text.find('\n') {
        let (head, rest) = text.split_at(pos);
        let mut fields: Vec<&str> = head.split(',').collect();
        *fields.last_mut().unwrap() = "z1001";
        text = format!("{}\n{}", fields.join(","), &rest[1..]);
    }
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/wide.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    view.update(cx, |v, _, cx| {
        v.scroll.0 = 1e9;
        cx.notify()
    })
    .unwrap();
    draw(cx, view);
    let (measured, last) = read(cx, view, |v| {
        (
            v.feed.widths.len(),
            v.head.last().map(|c| c.text.to_string()),
        )
    });
    println!("REV-R2 Quick Look at the right end: the note cell draws {} chars; {measured} columns measured; the last header drawn is {last:?}", note.chars().count());
    assert!(note.ends_with("END"), "the long cell's end is reachable");
    assert_eq!(measured, 1001, "the last column is measured and laid out");
    // The test platform cuts a five-character header to four, so the drawn text is z10 with an ellipsis.
    assert!(
        last.as_deref().is_some_and(|t| t.starts_with("z10")),
        "the last column is drawn"
    );
}

// A converter error on a locked file draws no sentence until the header sniff has said whether it is locked.
#[gpui::test]
fn pv2_review_r2_locked_file_draws_no_sentence_before_the_sniff(cx: &mut TestAppContext) {
    let (view, _, _) = office(cx, true);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/office-locked/locked.odt")
        .canonicalize()
        .unwrap();
    open(cx, view, path.to_str().unwrap());
    let i18n = crate::test_catalogs::english();
    let first = view
        .update(cx, |v, window, cx| {
            v.reply(
                r#"{"t":"officepdf","id":1,"state":"failed","err":"the converter wrote no PDF"}"#,
                window,
                cx,
            );
            v.failure(&i18n).map(|f| f.sentence)
        })
        .unwrap();
    let locked = Some(i18n.tr("document.office_locked"));
    let mut last = None;
    for _ in 0..200 {
        draw(cx, view);
        last = view
            .update(cx, |v, _, _| v.failure(&i18n).map(|f| f.sentence))
            .unwrap();
        if last == locked {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("REV-R2 sniff: before the sniff {first:?}; after the sniff {last:?}");
    assert_eq!(
        last, locked,
        "the sniff reads the real locked.odt as locked"
    );
    assert!(
        first.is_none(),
        "the swap showed {first:?} before the sniff said whether the file is locked"
    );
}
