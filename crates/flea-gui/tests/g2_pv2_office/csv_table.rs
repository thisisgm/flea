//! Quick Look CSV table proofs: the grid draws where the board puts it, a partial or refused table draws nothing, and a scroll asks only near the view.
use super::*;

// ql-office-sheet: the grid sits where the board draws it, row numbers muted, the header bold over its rule.
#[gpui::test]
fn pv2_csv_quick_look_draws_the_boards_grid(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 902., 625.);
    view.update(cx, |v, _, cx| v.set_path("/d/budget.csv", cx))
        .unwrap();
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| v.head.len()),
        0,
        "nothing draws before the first answer"
    );
    serve(cx, view, &sent, &budget());
    assert!(
        sent.borrow()
            .iter()
            .all(|a| a.contains(r#""raw":true"#) && a.contains(r#""csv":",""#)),
        "every ask is raw"
    );
    assert_eq!(sent.borrow().len(), 1, "one ask fills the first screen");
    let (head, rows) = read(cx, view, |v| {
        (
            v.head
                .iter()
                .map(|c| (c.x, c.text.clone(), c.right))
                .collect::<Vec<_>>(),
            v.rows
                .iter()
                .map(|r| {
                    (
                        r.top,
                        r.number.as_ref().map(|n| (n.x, n.text.clone(), n.right)),
                    )
                })
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(
        head[0],
        (56., "Item".into(), false),
        "Item starts at 56 as the board's first column"
    );
    assert!(
        head[1..].iter().all(|h| h.2),
        "Planned, Actual and Difference are right-aligned numbers"
    );
    assert_eq!(rows.len(), 13, "13 rows on the board's card");
    assert_eq!(
        rows[0].0, 33.,
        "the first row starts under the 33 px header"
    );
    assert_eq!(rows[1].0 - rows[0].0, 28., "rows are 28 apart");
    assert_eq!(
        rows[0].1,
        Some((0., "1".into(), true)),
        "row 1's number is right-aligned in the gutter"
    );
    let i18n = crate::test_catalogs::english();
    assert_eq!(
        read(cx, view, |v| v.count_text(&i18n)),
        Some("13 rows".into()),
        "the bar counts 13 rows"
    );
    let (shaped, painted) = read(cx, view, |v| (v.cells.shaped, v.painted.get()));
    assert_eq!(
        shaped,
        4 + 13 * 5,
        "each drawn cell, number and header shaped once"
    );
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| v.cells.shaped),
        shaped,
        "a second frame shapes nothing"
    );
    assert!(
        read(cx, view, |v| v.painted.get()) > painted,
        "and paints again"
    );
}

#[gpui::test]
fn pv2_csv_column_fits_its_frame_and_elides(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, false, 400., 600.);
    let mut text = String::from("name,size\n");
    text.push_str(&format!("{},1\n", "x".repeat(200)));
    view.update(cx, |v, _, cx| v.set_path("/d/long.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let cells = read(cx, view, |v| {
        v.rows[0]
            .cells
            .iter()
            .map(|c| (c.x, c.w, c.cut))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        read(cx, view, |v| v.head[0].x),
        12.,
        "the header has only the board's 12 px inset"
    );
    assert!(cells[0].2, "the long first cell is elided");
    assert!(
        cells.iter().all(|c| c.0 + c.1 <= 400. - 12.),
        "the drawn columns end inside the frame"
    );
    assert!(
        read(cx, view, |v| v.rows[0].number.is_none()),
        "the column draws no row numbers"
    );
}

#[gpui::test]
fn pv2_csv_wide_grid_scrolls_sideways_under_a_fixed_header(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 460., 600.);
    let mut text = (1..=12)
        .map(|c| format!("c{c}"))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    text.push_str(
        &(1..=12)
            .map(|c| format!("value-{c}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    text.push('\n');
    view.update(cx, |v, _, cx| v.set_path("/d/wide.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let at = |v: &CsvView, name: &str| v.head.iter().find(|c| &*c.text == name).map(|c| c.x);
    let before = read(cx, view, |v| at(v, "c3")).unwrap();
    assert_eq!(
        read(cx, view, |v| at(v, "c12")),
        None,
        "a column out of view is not laid out"
    );
    view.update(cx, |v, _, _| v.scroll.0 = 200.).unwrap();
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| at(v, "c3")),
        Some(before - 200.),
        "the header moves with the grid"
    );
    assert_eq!(
        read(cx, view, |v| v.rows[0].top),
        33.,
        "and stays at the top"
    );
}

// A frame between the answer and its parse, or for a binary file, draws no table at all.
#[gpui::test]
fn pv2_csv_never_draws_a_partial_or_refused_table(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 460., 600.);
    view.update(cx, |v, _, cx| v.set_path("/d/nul.csv", cx))
        .unwrap();
    draw(cx, view);
    let ask = sent.borrow()[0].clone();
    let token = field_usize(&ask, "token").unwrap();
    let binary = format!(
        r#"{{"t":"code","token":{token},"slot":"look","state":"binary","first":1,"total":0,"lines":[]}}"#
    );
    view.update(cx, |v, _, cx| v.answered(&binary, cx)).unwrap();
    draw(cx, view);
    assert!(
        read(cx, view, |v| v.head.is_empty() && !v.first_screen()),
        "the binary file draws no table"
    );
    draw(cx, view);
    assert_eq!(sent.borrow().len(), 1, "and asks nothing more");
    view.update(cx, |v, _, cx| v.set_text("plain.csv", "a,b\n1,2\n", cx))
        .unwrap();
    assert!(
        read(cx, view, |v| v.head.is_empty()),
        "a parse not yet landed draws nothing"
    );
    draw(cx, view);
    assert_eq!(
        read(cx, view, |v| (v.feed.body_rows(), v.rows.len())),
        (1, 1),
        "the caller's text draws one row"
    );
}

// A scroll past the first screen asks only what it lands on, and its wider cells widen their columns (S9G-8).
#[gpui::test]
fn pv2_csv_scroll_widens_to_held_cells_and_asks_near_the_view(cx: &mut TestAppContext) {
    let (view, sent) = table(cx, true, 460., 600.);
    let mut text = String::from("id,name\n");
    for i in 1..5000 {
        text.push_str(&format!(
            "{i},{}\n",
            if i == 60 {
                "n".repeat(120)
            } else {
                format!("f{i:05}")
            }
        ));
    }
    view.update(cx, |v, _, cx| v.set_path("/d/big.csv", cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    let widths = read(cx, view, |v| v.feed.widths.clone());
    let asked = sent.borrow().len();
    view.update(cx, |v, _, cx| v.scroll_to_row(4990, cx))
        .unwrap();
    serve(cx, view, &sent, &text);
    assert_eq!(
        sent.borrow().len() - asked,
        1,
        "one request draws the last screen"
    );
    widen(cx, view);
    let jumped = read(cx, view, |v| v.feed.widths.clone());
    assert!(
        jumped[0] > widths[0] && jumped[1] == widths[1],
        "a far jump widens the id column to its four-digit ids and never narrows the names: {widths:?} to {jumped:?}"
    );
    assert!(
        read(cx, view, |v| v.drawn_texts().any(|t| t == "f04999")),
        "the last row draws"
    );
    let shaped = read(cx, view, |v| v.cells.len());
    assert!(
        shaped <= 3 * 22 * 3,
        "the shaped cells stay bounded: {shaped}"
    );
}
