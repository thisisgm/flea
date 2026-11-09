//! S9G-8 proofs: Quick Look's columns widen to the widest held cell past the first screen (board ql-office-sheet), a frame at a time.
use super::*;

// S9G-8's long.csv: rows past the first screen hold wider values than it, and the last screen draws each one whole.
#[gpui::test]
fn s9g8_quick_look_columns_widen_to_cells_past_the_first_screen(cx: &mut TestAppContext) {
    let text: String = std::iter::once("a,b,c\n".to_string())
        .chain((0..600).map(|i| format!("{i},{},{}\n", i * 2, i * 3)))
        .collect();
    let (view, sent) = table(cx, true, 1100., 700.);
    view.update(cx, |v, _, cx| v.set_path("/d/long.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    view.update(cx, |v, _, cx| v.scroll_to_row(600, cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    widen(cx, view);
    draw(cx, view);
    let last: Vec<(String, bool)> = read(cx, view, |v| {
        v.rows
            .iter()
            .find(|row| row.record == 601)
            .expect("the last record is drawn")
            .cells
            .iter()
            .map(|c| (c.text.to_string(), c.cut))
            .collect()
    });
    println!("S9G-8 long.csv last row draws {last:?}");
    let whole = |t: &str| (t.to_string(), false);
    assert_eq!(
        last,
        vec![whole("599"), whole("1198"), whole("1797")],
        "every value past the first screen draws whole"
    );
}

// The view asks for its own next frame while held cells are left to widen by, and for none once they are done.
#[gpui::test]
fn s9g8_widening_asks_for_its_own_next_frame(cx: &mut TestAppContext) {
    // Reduced motion keeps the scroll bars' fade from asking for frames, so every frame asked here is the table's.
    cx.update(|cx| cx.set_reduce_motion(true));
    let text: String = std::iter::once("a,b,c,d\n".to_string())
        .chain((0..5000).map(|i| format!("{i},{i},{i},{i}\n")))
        .collect();
    let (view, sent) = table(cx, true, 1100., 700.);
    view.update(cx, |v, _, cx| v.set_path("/d/wide.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    view.update(cx, |v, _, cx| {
        v.feed.margin = 200;
        v.scroll_to_row(2000, cx);
    })
    .unwrap();
    serve(cx, view, &sent, &text);
    let frames = |cx: &mut TestAppContext| {
        view.update(cx, |_, window, cx| window.simulate_next_frame(cx))
            .unwrap()
    };
    frames(cx);
    draw(cx, view);
    assert!(
        read(cx, view, |v| v.feed.widening),
        "held cells are left after one frame"
    );
    assert!(frames(cx) >= 1, "the table asked for its next frame");
    widen(cx, view);
    frames(cx);
    draw(cx, view);
    assert_eq!(
        frames(cx),
        0,
        "a table with nothing left to widen asks for no frame"
    );
}
