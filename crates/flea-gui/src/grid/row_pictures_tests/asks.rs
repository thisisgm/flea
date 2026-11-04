//! Which rows the pictures ask for and cancel: the drawn files only, the thumbnail mode, a view switch, and a filter that hides every row.
use super::*;

fn offscreen_rows_make_no_request(cx: &mut TestAppContext, view: View) {
    let (handle, mut lines) = open(cx, view, "{}");
    let first = first_settle(cx, handle, &mut lines);
    let shown: Vec<usize> = read(cx, handle, |shell| drawn(shell, view))
        .into_iter()
        .map(|(row, _)| row)
        .collect();
    let total = read(cx, handle, |shell| {
        shell.panes[0].as_ref().unwrap().pane.held.total
    });
    assert!(
        shown.len() > 6 && shown.len() < total,
        "{view:?}: the viewport holds a screenful: {shown:?}"
    );
    let files: Vec<usize> = shown.iter().copied().filter(|&row| row != 3).collect();
    assert_eq!(
        asked(&first),
        files,
        "{view:?}: exactly the drawn files are asked"
    );
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        counters.asked,
        files.len(),
        "{view:?}: no offscreen row is asked"
    );
    settle(cx, handle, 200);
    assert!(
        asked(&drained(&mut lines)).is_empty(),
        "{view:?}: a still frame asks nothing again"
    );
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().pane.scroll_y = 3_100.
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let moved = drained(&mut lines);
    let now: Vec<usize> = read(cx, handle, |shell| drawn(shell, view))
        .into_iter()
        .map(|(row, _)| row)
        .collect();
    assert!(
        now[0] >= 90,
        "{view:?}: the scroll moved the viewport: {now:?}"
    );
    assert_eq!(
        asked(&moved),
        now,
        "{view:?}: only the rows now in view are asked"
    );
    let cancel = moved
        .iter()
        .find(|line| line.starts_with(r#"{"c":"thumbcancel","#))
        .expect("the rows left behind are cancelled");
    assert!(cancel.contains(&format!("[{}", files[0])), "{cancel}");
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        counters.asked,
        files.len() + now.len(),
        "{view:?}: the counter gate holds after a scroll"
    );
    assert_eq!(counters.cancelled, files.len(), "{view:?}");
}

#[gpui::test]
fn offscreen_list_rows_make_no_request(cx: &mut TestAppContext) {
    offscreen_rows_make_no_request(cx, View::List);
}

#[gpui::test]
fn offscreen_columns_rows_make_no_request(cx: &mut TestAppContext) {
    offscreen_rows_make_no_request(cx, View::Columns);
}

#[gpui::test]
fn show_thumbnails_off_and_images_follow_the_mode_in_list_and_columns(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns] {
        let (handle, mut lines) = open(cx, view, r#"{"preview":{"thumbnails":"off"}}"#);
        assert!(
            asked(&first_settle(cx, handle, &mut lines)).is_empty(),
            "{view:?}: Off asks nothing"
        );
        let (handle, mut lines) = open(cx, view, r#"{"preview":{"thumbnails":"images"}}"#);
        assert_eq!(
            asked(&first_settle(cx, handle, &mut lines)),
            vec![1],
            "{view:?}: Images asks the image only"
        );
    }
}

#[gpui::test]
fn a_view_switch_keeps_the_one_lender_and_asks_nothing_again(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    first_settle(cx, handle, &mut lines);
    deliver(cx, handle, &thumbed(1, PICTURE));
    cx.run_until_parked();
    step(cx, handle);
    assert!(read(cx, handle, |shell| picture_at(shell, View::List, 1)).is_some());
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let again = asked(&drained(&mut lines));
    assert!(
        !again.contains(&1),
        "the Grid borrows the List's decoded picture: {again:?}"
    );
    let lent = read(cx, handle, |shell| {
        let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
        let shared = grid
            .decor
            .pictures()
            .expect("the Grid lends the pane's pictures") as *const Pictures;
        (
            shared == Rc::as_ptr(&pictures(shell)),
            grid.state
                .borrow()
                .drawn_row(1)
                .and_then(|tile| tile.decoded),
        )
    });
    assert_eq!(
        lent,
        (true, Some((1280, 64))),
        "one lender per pane, drawn by the Grid tile"
    );
}

#[gpui::test]
fn a_filter_that_hides_every_row_cancels_the_asks_still_waiting(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    let first = asked(&first_settle(cx, handle, &mut lines));
    assert!(!first.is_empty());
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().filter.shown = Some(Rc::from(Vec::<usize>::new()))
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    let after = drained(&mut lines);
    let cancel = after
        .iter()
        .find(|line| line.starts_with(r#"{"c":"thumbcancel","#))
        .expect("an empty filter cancels every waiting ask");
    assert!(cancel.contains(&format!("[{}", first[0])), "{cancel}");
    assert!(asked(&after).is_empty(), "nothing hidden is asked");
    assert_eq!(
        read(cx, handle, |shell| pictures(shell).waiting()),
        Vec::<usize>::new()
    );
}

#[gpui::test]
fn a_stopped_pane_lets_its_asks_go_without_counting_a_cancel(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx, View::List, "{}");
    let first = asked(&first_settle(cx, handle, &mut lines));
    assert!(!first.is_empty(), "the drawn files are asked");
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().stopped = true
        })
        .unwrap();
    step(cx, handle);
    let counters = read(cx, handle, |shell| pictures(shell).counters());
    assert_eq!(
        counters.cancelled, 0,
        "a stopped backend is never counted a cancel it cannot send"
    );
    assert!(
        drained(&mut lines).is_empty(),
        "a stopped pane writes nothing"
    );
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_mut().unwrap().stopped = false
        })
        .unwrap();
    step(cx, handle);
    settle(cx, handle, 120);
    assert_eq!(
        asked(&drained(&mut lines)),
        first,
        "the released rows are asked again once the backend is back"
    );
}
