//! Clearing a deep filter refills only the missing viewport.
use super::*;

fn drawn_rows_are_held_or_asked(cx: &mut TestAppContext, handle: Handle) -> Vec<usize> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let held = &host.pane.held;
        let owed = host.pane.requests.outstanding;
        let rows: Vec<usize> = host
            .list
            .borrow()
            .visible()
            .iter()
            .map(|row| row.index)
            .collect();
        for row in &rows {
            let holds = *row >= held.start && *row < held.start + held.rows.len();
            let asked =
                owed.is_some_and(|owed| *row >= owed.start && *row < owed.start + owed.count);
            assert!(
                holds || asked,
                "row {row} is drawn with no data and no request (held {}..{}, asked {owed:?})",
                held.start,
                held.start + held.rows.len()
            );
        }
        rows
    })
}

fn answer_window(cx: &mut TestAppContext, handle: Handle) {
    let owed = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.requests.outstanding
    });
    let Some(owed) = owed else { return };
    let row = r#"{"n":"r","d":false,"s":0,"m":0,"p":33188,"i":"text"}"#;
    let rows = vec![row; owed.count.min(1000 - owed.start)].join(",");
    reply(
        cx,
        handle,
        &format!(
            r#"{{"t":"rows","start":{},"listing":2,"rows":[{rows}]}}"#,
            owed.start
        ),
    );
}

#[gpui::test]
fn clearing_a_deep_filter_refills_the_rows_with_one_escape(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 900;
        host.pane.reveal_cursor();
    });
    frame(cx, handle);
    answer_window(cx, handle);
    frame(cx, handle);
    while read().is_some() {}
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "n", Some("n"), false);
    assert_eq!(
        read().as_deref(),
        Some(r#"{"c":"filter","query":"n","id":1}"#)
    );
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":1,"query":"n","n":1000,"rows":[40,950],"ms":0.010,"listing":2}"#,
    );
    frame(cx, handle);
    answer_window(cx, handle);
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.error.is_none(), "fixture error: {:?}", host.error);
        assert!(host.filter.owner.typing(), "the line still has the caret");
    });
    // One Escape clears the line and the filter; the rows under the cursor come back, never a blank list (#318).
    press(cx, handle, "escape", None, false);
    if shown(cx, handle).is_some() {
        press(cx, handle, "escape", None, false);
        panic!("the first Escape left the filter standing");
    }
    frame(cx, handle);
    let rows = drawn_rows_are_held_or_asked(cx, handle);
    assert!(!rows.is_empty(), "the cleared list draws rows");
    let cursor = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.cursor
    });
    assert!(
        rows.contains(&cursor),
        "the cursor row {cursor} is drawn: {rows:?}"
    );
    answer_window(cx, handle);
    frame(cx, handle);
    drawn_rows_are_held_or_asked(cx, handle);
}
