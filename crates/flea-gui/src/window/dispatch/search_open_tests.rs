//! Enter on a search result opens that result, though leaving the search runs before the step reads the cursor.
use super::test_shell::{asked, land, open, row};
use crate::backend::{Backend, Event as BackendEvent};
use flea::protocol::{Reply, RowFacts};
use gpui::{Entity, TestAppContext, VisualTestContext};

pub(super) fn line(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, wire: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(wire), window, cx)
        })
    });
}

// One search generation's answer: its listed line, its rows and the searched line that ends the walk.
pub(super) fn answer(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    generation: usize,
    rows: Vec<RowFacts>,
) {
    let total = rows.len();
    line(
        shell,
        cx,
        &format!(r#"{{"t":"listed","path":"/files","n":{total},"gen":{generation}}}"#),
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let reply = Reply::Rows {
                start: 0,
                listing: 10 + generation,
                rows,
            };
            shell.receive(0, BackendEvent::Session { generation, reply }, window, cx)
        })
    });
    line(
        shell,
        cx,
        &format!(
            r#"{{"t":"searched","n":{total},"scanned":9,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
        ),
    );
    cx.run_until_parked();
}

#[gpui::test]
fn enter_on_a_result_row_opens_that_result_and_not_the_folder_row_at_its_index(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![
            row("alpha", true),
            row("beta", true),
            row("notes.txt", false),
        ],
        1,
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("f s");
    answer(
        &shell,
        cx,
        1,
        vec![row("notes.txt", false), row("sub/deep", true)],
    );
    // Down leaves the query line for result 1, the folder's beta row by index; Enter then opens it.
    cx.simulate_keystrokes("down");
    let (cursor, held) = shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        (pane.cursor, pane.held.row(1).map(|row| row.name.clone()))
    });
    assert_eq!((cursor, held.as_deref()), (1, Some("sub/deep")));
    cx.simulate_keystrokes("enter");
    assert_eq!(
        asked(&shell, cx),
        "/files/sub/deep",
        "the result under the cursor opens"
    );
    shell.read_with(cx, |shell, _| {
        let search = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(
            search.session.leaving,
            "opening the result leaves the search"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
