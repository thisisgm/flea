//! A parked archive wait is quiet when focus or the cursor leaves (a shrink that moves the cursor included); a row that cannot load or was replaced is refused.
use super::opener_tests::{looking, quit, shell};
use super::test_shell::{asked_on, land_on, row, status};
use crate::backend::{Backend, Event};
use crate::window::startup::View;
use flea::protocol::Reply;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};

type Actions = Rc<RefCell<Vec<&'static str>>>;

fn answer(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Event::Producer(line.into()), window, cx)
        })
    });
    cx.run_until_parked();
}

// Parks the offscreen marked archive of opener_selection_tests until its row window is requested, and returns that window.
fn parked_archive(
    cx: &mut TestAppContext,
    dual: bool,
) -> (
    Entity<crate::state::Shell>,
    &mut VisualTestContext,
    Actions,
    Vec<gpui::Subscription>,
    usize,
    usize,
) {
    let (shell, cx, (actions, _), keep) = shell(cx);
    if dual {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx))
        });
        let right = asked_on(&shell, cx, 1);
        land_on(&shell, cx, 1, &right, vec![row("gamma", true)], 1, 1);
    }
    cx.simulate_keystrokes("f5");
    let count = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (0..count)
            .map(|i| row(&format!("filler-{i}.txt"), false))
            .collect(),
        1000,
        2,
    );
    let (backend, mut windows) = Backend::test_channel();
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.backend = Some(backend);
        host.selected = vec![900];
    });
    cx.simulate_keystrokes("enter");
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    answer(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[32768]}}"#),
    );
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"open","id":{id},"ok":true,"paths":["/files/backup.tar.zst"]}}"#
        ),
    );
    let (start, count) =
        windows().expect("the existing bounded window request loads the archive row");
    assert!(start <= 900 && start + count > 900 && count < 1000);
    (shell, cx, actions, keep, start, count)
}

fn deliver_rows(
    shell: &Entity<crate::state::Shell>,
    cx: &mut VisualTestContext,
    start: usize,
    count: usize,
    archive: &str,
) {
    let rows = (start..(start + count).min(1000))
        .map(|i| row(if i == 900 { archive } else { "filler.txt" }, false))
        .collect();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start,
                    rows,
                    listing: 2,
                }),
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn gh350_archive_wait_ends_quietly_when_focus_moves_to_the_other_pane(cx: &mut TestAppContext) {
    let (shell, cx, actions, _keep, start, count) = parked_archive(cx, true);
    cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
    cx.simulate_keystrokes("tab");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.focus_side),
        1,
        "Tab moves focus to the right pane"
    );
    actions.borrow_mut().clear();
    let before = status(&shell, cx);
    deliver_rows(&shell, cx, start, count, "backup.tar.zst");
    assert!(
        actions.borrow().is_empty(),
        "a bare preview acts on the focused pane's cursor, so the abandoned wait must not preview: {:?}",
        actions.borrow()
    );
    assert_eq!(looking(&shell, cx), "false", "no Quick Look opens");
    assert_eq!(
        status(&shell, cx),
        before,
        "an abandoned wait posts no refusal"
    );
    assert!(
        !shell.read_with(cx, |shell, _| shell.open_selection_parked(0)),
        "the abandoned wait releases its capture"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_archive_wait_ends_quietly_when_the_cursor_leaves_its_row(cx: &mut TestAppContext) {
    let (shell, cx, actions, _keep, start, count) = parked_archive(cx, false);
    cx.simulate_keystrokes("down");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
        901,
        "down moves the cursor off the archive row"
    );
    actions.borrow_mut().clear();
    let before = status(&shell, cx);
    deliver_rows(&shell, cx, start, count, "backup.tar.zst");
    assert_eq!(
        status(&shell, cx),
        before,
        "an abandoned wait posts no not_loaded refusal"
    );
    assert!(actions.borrow().is_empty(), "{:?}", actions.borrow());
    assert_eq!(looking(&shell, cx), "false", "no Quick Look opens");
    assert!(
        !shell.read_with(cx, |shell, _| shell.open_selection_parked(0)),
        "the abandoned wait releases its capture"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_archive_wait_when_its_row_vanishes(cx: &mut TestAppContext) {
    let (shell, cx, actions, _keep, start, count) = parked_archive(cx, false);
    actions.borrow_mut().clear();
    assert!(
        shell.read_with(cx, |shell, _| shell.open_selection_parked(0)),
        "the archive wait is still parked when the shrink arrives"
    );
    let refusal = shell.read_with(cx, |shell, _| {
        shell.locale.catalog.tr("feedback.refusal.not_loaded")
    });
    let held = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.held.rows.len()
    });
    land_on(
        &shell,
        cx,
        0,
        "/files",
        (0..held)
            .map(|i| row(&format!("filler-{i}.txt"), false))
            .collect(),
        800,
        3,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .total),
        800,
        "the same folder listing shrinks below the parked archive row 900"
    );
    assert!(
        !shell.read_with(cx, |shell, _| shell.open_selection_parked(0)),
        "the shrink itself ends the wait"
    );
    let notice = cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let footer = shell.footer(cx);
            let line = footer.read(cx).line.clone();
            line.read(cx).messages().shown().text.to_string()
        })
    });
    assert_ne!(
        notice, refusal,
        "the shrink that ends the wait posts no not_loaded refusal"
    );
    let before = status(&shell, cx);
    deliver_rows(&shell, cx, start, count, "backup.tar.zst");
    assert!(
        actions.borrow().is_empty(),
        "a vanished row previews nothing: {:?}",
        actions.borrow()
    );
    assert_eq!(looking(&shell, cx), "false", "no Quick Look opens");
    assert_eq!(
        status(&shell, cx),
        before,
        "an abandoned wait posts no not_loaded refusal"
    );
    assert!(
        !shell.read_with(cx, |shell, _| shell.open_selection_parked(0)),
        "the abandoned wait releases its capture"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_archive_wait_refuses_when_its_row_is_replaced(cx: &mut TestAppContext) {
    let (shell, cx, actions, _keep, start, count) = parked_archive(cx, false);
    actions.borrow_mut().clear();
    deliver_rows(&shell, cx, start, count, "other.tar.zst");
    assert!(
        actions.borrow().is_empty(),
        "a replaced row previews nothing: {:?}",
        actions.borrow()
    );
    assert_eq!(looking(&shell, cx), "false", "no Quick Look opens");
    assert_eq!(
        status(&shell, cx),
        "Some marked files haven’t loaded yet.",
        "a replaced target row is refused"
    );
    assert!(
        !shell.read_with(cx, |shell, _| shell.open_selection_parked(0)),
        "the refused wait releases its capture"
    );
    quit(&shell, cx);
}
