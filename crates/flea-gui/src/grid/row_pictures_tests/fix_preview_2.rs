//! SW-14: a filtered Columns track asks pictures for the rows the filter draws, as the List does (Filter.cut).
use super::*;

#[gpui::test]
fn fix_preview_2_a_filtered_columns_track_asks_the_rows_it_draws(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::Columns, "{}");
    let first = asked(&first_settle(cx, handle, &mut lines));
    assert!(
        !first.contains(&300),
        "row 300 starts off screen: {first:?}"
    );
    // The filter draws two rows deep in the listing at the top of the track, as a typed query does.
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().filter.shown = Some(Rc::from(vec![300, 301]))
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let drawn_rows: Vec<usize> = read(cx, handle, |shell| drawn(shell, View::Columns))
        .into_iter()
        .map(|(row, _)| row)
        .collect();
    assert_eq!(
        drawn_rows,
        vec![300, 301],
        "the track draws the filter's rows"
    );
    assert_eq!(
        asked(&drained(&mut lines)),
        vec![300, 301],
        "the rows the filter draws are asked"
    );
}
