//! An own delete lands the cursor on the first deleted row's slot, wherever the cursor stood, deep in a long folder.
use super::super::{deep, folder, name, place, receive, serve, without, Place, DONE, TOTAL, WATCH};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
    window::dispatch::test_shell::{open_in, sent},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

const CHANGED: &str = r#"{"t":"changed","path":"/files"}"#;

// What the backend says after the delete request, in the order a case names.
#[derive(Clone, Copy)]
enum Answer {
    Trashed,
    // The transfer a trash also is, ended: the pane owes its anchored re-read.
    Done,
    // A change notice whose re-read is out and unanswered when the next answer comes.
    Rereading,
}

fn index(place: &Place) -> usize {
    place.cursor[1..].parse().unwrap()
}

// How far below the viewport's top row `index` is drawn, read as Place reads the cursor's.
fn drawn(shell: &Entity<Shell>, cx: &mut VisualTestContext, index: usize) -> f32 {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match host.grid.as_ref().filter(|grid| grid.view.visible) {
            Some(grid) => grid
                .state
                .borrow()
                .drawn_row(index)
                .map_or(f32::NAN, |tile| tile.cell.y),
            None => index as f32 * host.pane.row_height() - host.pane.scroll_y,
        }
    })
}

// The rows at `marks` (steps from the cursor row) marked, with the cursor left where `deep` put it.
fn marked(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    before: &Place,
    marks: &[isize],
) -> Vec<usize> {
    let cursor = index(before);
    let rows: Vec<usize> = marks
        .iter()
        .map(|by| cursor.checked_add_signed(*by).unwrap())
        .collect();
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.selected = rows.clone();
            host.input.selection.sync(&host.selected);
        })
    });
    rows
}

// The answers arrive, the folder is read without `rows`, and the cursor stands on the row now in the first deleted slot, drawn where that slot was, as the lone mark.
fn lands_on_the_slot(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    rows: &[usize],
    slot_drawn: f32,
    answers: &[Answer],
) {
    let gone: Vec<String> = rows.iter().map(|&row| name(row)).collect();
    let after = without(&gone);
    for answer in answers {
        match answer {
            Answer::Trashed => {
                let line = format!(r#"{{"t":"trashed","ok":{},"failed":0}}"#, rows.len());
                receive(shell, cx, 0, Backend::test_reply_line(&line));
            }
            Answer::Done => receive(shell, cx, 0, BackendEvent::Operations(DONE.into())),
            Answer::Rereading => {
                receive(shell, cx, 0, Backend::test_reply_line(CHANGED));
                cx.executor()
                    .advance_clock(WATCH + Duration::from_millis(1));
                cx.run_until_parked();
                let busy =
                    shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy());
                assert!(busy, "the watched re-read is out");
            }
        }
    }
    serve(shell, cx, 0, "/files", &after);
    cx.executor().advance_clock(WATCH * 3);
    cx.run_until_parked();
    serve(shell, cx, 0, "/files", &after);
    let replacement = after[rows[0]].name.clone();
    assert_eq!(
        place(shell, cx, 0),
        Place {
            cursor: replacement.clone(),
            offset: slot_drawn,
            marked: vec![replacement],
        },
        "the cursor took the first deleted slot, drawn where that slot was"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Delete on the marks, then the answers.
fn delete(cx: &mut TestAppContext, view: &str, marks: &[isize], answers: &[Answer]) {
    let (shell, cx, before) = deep(cx, view);
    let rows = marked(&shell, cx, &before, marks);
    let slot_drawn = drawn(&shell, cx, rows[0]);
    sent();
    cx.simulate_keystrokes("delete");
    let wanted = format!(
        "\"rows\":[{}]",
        rows.iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let requests = sent();
    assert!(
        requests
            .iter()
            .any(|line| line.contains(r#""c":"trash""#) && line.contains(&wanted)),
        "Delete trashes the marked rows: {requests:?}"
    );
    lands_on_the_slot(&shell, cx, &rows, slot_drawn, answers);
}

const TRASH: [Answer; 2] = [Answer::Trashed, Answer::Done];

#[gpui::test]
fn gh476_list_delete_of_the_cursor_row_keeps_its_slot(cx: &mut TestAppContext) {
    delete(cx, "list", &[0], &TRASH);
}

#[gpui::test]
fn gh476_list_delete_of_a_mark_above_the_cursor_lands_on_its_slot(cx: &mut TestAppContext) {
    delete(cx, "list", &[-3], &TRASH);
}

#[gpui::test]
fn gh476_list_delete_of_a_mark_below_the_cursor_lands_on_its_slot(cx: &mut TestAppContext) {
    delete(cx, "list", &[2], &TRASH);
}

#[gpui::test]
fn gh476_list_delete_of_marks_around_the_cursor_lands_on_the_first_slot(cx: &mut TestAppContext) {
    delete(cx, "list", &[-3, 3], &TRASH);
}

#[gpui::test]
fn gh476_list_delete_with_the_cursor_on_a_later_mark_lands_on_the_first_slot(
    cx: &mut TestAppContext,
) {
    delete(cx, "list", &[-3, 0, 3], &TRASH);
}

#[gpui::test]
fn gh476_grid_delete_of_a_mark_before_the_cursor_lands_on_its_slot(cx: &mut TestAppContext) {
    delete(cx, "grid", &[-3], &TRASH);
}

#[gpui::test]
fn gh476_columns_delete_of_a_mark_above_the_cursor_lands_on_its_slot(cx: &mut TestAppContext) {
    delete(cx, "columns", &[-3], &TRASH);
}

// The transfer's own re-read and the delete landing: whichever is asked first, the cursor ends on the slot once.
#[gpui::test]
fn gh476_delete_lands_on_its_slot_when_the_transfer_ends_first(cx: &mut TestAppContext) {
    delete(cx, "list", &[-3], &[Answer::Done, Answer::Trashed]);
}

#[gpui::test]
fn gh476_delete_lands_on_its_slot_when_a_watched_reread_is_out(cx: &mut TestAppContext) {
    delete(
        cx,
        "list",
        &[-3],
        &[Answer::Rereading, Answer::Trashed, Answer::Done],
    );
}

// A drag of the marked rows released on the rail's Trash.
#[gpui::test]
fn gh476_trash_drop_lands_on_the_first_dropped_slot(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "list");
    let rows = marked(&shell, cx, &before, &[-3, 3]);
    let slot_drawn = drawn(&shell, cx, rows[0]);
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            let line = format!(
                r#"{{"c":"trash","paths":["/files/{}","/files/{}"]}}"#,
                name(rows[0]),
                name(rows[1])
            );
            shell.trash_dropped(0, line, &rows, true, cx);
        })
    });
    lands_on_the_slot(&shell, cx, &rows, slot_drawn, &TRASH);
}

// Delete Permanently: the reviewed snapshot's delete answers, and the landing is the trash's.
#[gpui::test]
fn gh476_delete_permanently_lands_on_the_first_deleted_slot(cx: &mut TestAppContext) {
    let (shell, cx, before) = deep(cx, "list");
    let rows = marked(&shell, cx, &before, &[-3]);
    let slot_drawn = drawn(&shell, cx, rows[0]);
    sent();
    cx.simulate_keystrokes("shift-delete");
    let asked = sent();
    let snapshot = asked
        .iter()
        .find(|line| line.contains(r#""op":"snapshot""#))
        .unwrap_or_else(|| panic!("Shift+Delete asks for its snapshot: {asked:?}"));
    let id = flea::json::field_usize(snapshot, "id").unwrap();
    let reply = |cx: &mut VisualTestContext, line: String| {
        receive(&shell, cx, 0, Backend::test_reply_line(&line));
    };
    reply(
        cx,
        format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    reply(
        cx,
        format!(
            r#"{{"t":"menuaction","id":{id},"op":"prepareDelete","ok":true,"token":77,"count":1,"bytes":12}}"#
        ),
    );
    cx.simulate_keystrokes("tab enter");
    let confirmed = sent();
    assert!(
        confirmed
            .iter()
            .any(|line| line.contains(r#""op":"delete""#) && line.contains(r#""token":77"#)),
        "the card's Delete sends the reviewed token: {confirmed:?}"
    );
    reply(
        cx,
        format!(r#"{{"t":"menuaction","id":{id},"op":"delete","ok":true,"deleted":1}}"#),
    );
    lands_on_the_slot(&shell, cx, &rows, slot_drawn, &[]);
}

// Marks on the folder's last two rows with the cursor above them: the slot is past the new end, so the cursor takes the new last row.
#[gpui::test]
fn gh476_list_delete_of_the_last_rows_lands_on_the_new_last_row(cx: &mut TestAppContext) {
    let (shell, cx) = open_in(cx, "default", "list");
    serve(&shell, cx, 0, "/files", &folder());
    cx.simulate_keystrokes("end");
    serve(&shell, cx, 0, "/files", &folder());
    cx.simulate_keystrokes("up up up up");
    serve(&shell, cx, 0, "/files", &folder());
    let rows = vec![TOTAL - 2, TOTAL - 1];
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            assert_eq!(
                host.pane.cursor,
                TOTAL - 5,
                "the cursor stands above the marks"
            );
            host.selected = rows.clone();
            host.input.selection.sync(&host.selected);
        })
    });
    sent();
    cx.simulate_keystrokes("delete");
    let requests = sent();
    let wanted = format!("\"rows\":[{},{}]", rows[0], rows[1]);
    assert!(
        requests
            .iter()
            .any(|line| line.contains(r#""c":"trash""#) && line.contains(&wanted)),
        "Delete trashes the last two rows: {requests:?}"
    );
    let line = format!(r#"{{"t":"trashed","ok":{},"failed":0}}"#, rows.len());
    receive(&shell, cx, 0, Backend::test_reply_line(&line));
    receive(&shell, cx, 0, BackendEvent::Operations(DONE.into()));
    let after = without(&[name(TOTAL - 2), name(TOTAL - 1)]);
    serve(&shell, cx, 0, "/files", &after);
    cx.executor().advance_clock(WATCH * 3);
    cx.run_until_parked();
    serve(&shell, cx, 0, "/files", &after);
    let landed = place(&shell, cx, 0);
    let last = name(TOTAL - 3);
    assert_eq!(
        (landed.cursor, landed.marked),
        (last.clone(), vec![last]),
        "the cursor took the new last row as the lone mark"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
