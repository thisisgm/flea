//! Ctrl+Shift+N, Delete, dd, Shift+Delete and Duplicate run on the production Shell, and their replies land in the pane.
use super::test_shell::{keystroke, land, lists, open, row, set_cursor, status};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
};
use flea::{
    keys::{self, Frontend},
    protocol::Reply,
    ui_logic_op1::{lines, trash},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

// The pane's backend becomes a line recorder; the requests this family sends are read back in order.
pub(super) fn wire(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> impl FnMut() -> Vec<String> {
    let (backend, mut read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    move || {
        std::iter::from_fn(&mut read)
            .filter(|line| {
                let c = flea::json::field_str(line, "c").unwrap_or_default();
                matches!(c.as_str(), "mkdir" | "duplicate" | "trash" | "menuaction")
            })
            .collect()
    }
}

pub(super) fn reply(shell: &Entity<Shell>, cx: &mut VisualTestContext, line: &str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, Backend::test_reply_line(line), window, cx)
        })
    });
    cx.run_until_parked();
}

pub(crate) fn creation_request(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    line: &str,
) -> String {
    let request = shell.read_with(cx, |shell, _| shell.menu.sequence);
    assert!(request > 0);
    format!(
        "{},\"request\":{request}}}",
        line.strip_suffix('}').unwrap()
    )
}

// The keystroke a preset binds to an action, as the platform delivers it.
fn hit(cx: &mut VisualTestContext, preset: &str, action: &str) {
    let binding = keys::binding_rows(preset, Frontend::Gui)
        .find(|binding| binding.action == action && binding.context.contains("listing"))
        .expect("a bound key");
    cx.simulate_event(keystroke(binding));
    cx.run_until_parked();
}

// The footer's notice, the line a finished operation says.
fn notice(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.update(cx, |shell, cx| {
        let catalog = shell.locale.catalog.clone();
        shell.status_messages(cx, |messages, _| messages.notice_text(&catalog))
    })
}

fn catalog(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> std::sync::Arc<flea::i18n::I18n> {
    shell.read_with(cx, |shell, _| shell.locale.catalog.clone())
}

#[gpui::test]
fn new_folder_and_trash_keys_send_their_requests_and_the_replies_land(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    let catalog = catalog(&shell, cx);
    // New Folder supplies the catalog base; the backend resolves collisions.
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(
        sent(),
        [creation_request(
            &shell,
            cx,
            r#"{"c":"mkdir","path":"/files","defaultBase":"New Folder"}"#
        )]
    );
    let before = lists(&shell, cx);
    reply(
        &shell,
        cx,
        r#"{"t":"made","ok":true,"path":"/files/New Folder"}"#,
    );
    assert_eq!(
        notice(&shell, cx),
        lines::made(&catalog, "default", "/files/New Folder")
    );
    assert!(notice(&shell, cx).contains("New Folder"));
    assert_eq!(lists(&shell, cx), before + 1, "the folder is read again");
    let select = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.pending_select.clone()
    });
    assert_eq!(
        select.as_deref(),
        Some("/files/New Folder"),
        "the new folder lands under the cursor"
    );
    land(&shell, cx, "/files", home(), 2);
    // Ops.js trash: the marked rows, cleared once the backend answers.
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 2]
    });
    cx.simulate_keystrokes("delete");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[0,2],"menuId":0,"listing":2}"#]
    );
    reply(&shell, cx, r#"{"t":"trashed","ok":2,"failed":0}"#);
    assert_eq!(
        status(&shell, cx),
        lines::trashed(&catalog, "default", 2, 0, "")
    );
    assert!(shell.read_with(cx, |shell, _| shell.panes[0]
        .as_ref()
        .unwrap()
        .selected
        .is_empty()));
    land(
        &shell,
        cx,
        "/files",
        vec![row("beta", true), row("arrived.txt", false)],
        3,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .selected
            .clone()),
        [0]
    );
    cx.simulate_keystrokes("escape");
    // With nothing marked the cursor row goes; with no row at all it says why.
    set_cursor(&shell, cx, 1);
    cx.simulate_keystrokes("delete");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[1],"menuId":0,"listing":3}"#]
    );
    land(&shell, cx, "/files", Vec::new(), 4);
    cx.simulate_keystrokes("delete");
    assert_eq!(sent(), Vec::<String>::new());
    assert_eq!(status(&shell, cx), catalog.tr("filter.nothing_to_act_on"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn duplicate_sends_the_cursor_row_and_lands_on_the_copy(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "mac");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    set_cursor(&shell, cx, 2);
    hit(cx, "mac", "duplicate");
    assert_eq!(
        sent(),
        [creation_request(
            &shell,
            cx,
            r#"{"c":"duplicate","path":"/files/notes.txt","menuId":0}"#
        )]
    );
    reply(
        &shell,
        cx,
        r#"{"t":"duplicated","ok":true,"path":"/files/notes copy.txt"}"#,
    );
    let catalog = catalog(&shell, cx);
    let line = catalog.tr_args("jobs.done.duplicated", &["notes copy.txt"]);
    assert_eq!(notice(&shell, cx), lines::with_undo(&catalog, "mac", line));
    assert!(notice(&shell, cx).contains("notes copy.txt"));
    let select = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.pending_select.clone()
    });
    assert_eq!(select.as_deref(), Some("/files/notes copy.txt"));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn the_dd_pair_arms_then_trashes_and_a_move_or_the_clock_ends_the_arm(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    let catalog = catalog(&shell, cx);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("d");
    assert_eq!(sent(), Vec::<String>::new(), "the first d only arms");
    assert_eq!(
        status(&shell, cx),
        trash::trash_prompt(&catalog, "default", 1)
    );
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    // Any other listing action ends the pair, even one another family claims that keeps the seat: Forward with no forward history.
    cx.simulate_keystrokes("d");
    hit(cx, "default", "historyForward");
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        Vec::<String>::new(),
        "the d after Forward arms again"
    );
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    // A cursor move between the presses ends the pair: the d after it arms again on the new row.
    cx.simulate_keystrokes("d up d");
    assert_eq!(sent(), Vec::<String>::new());
    // So does the clock: a d past ARM_MS arms again, and only the d after it trashes.
    cx.executor()
        .advance_clock(Duration::from_millis(trash::ARM_MS + 1));
    cx.run_until_parked();
    cx.simulate_keystrokes("d");
    assert_eq!(sent(), Vec::<String>::new());
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[1],"menuId":0,"listing":1}"#]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_key_only_handler_or_an_unbound_key_between_the_presses_ends_the_dd_pair(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    set_cursor(&shell, cx, 2);
    let trashed = |lines: &[String]| lines.iter().any(|line| line.contains(r#""c":"trash""#));
    // Copy is taken by the clipboard handler ahead of the dispatch point and keeps the seat.
    cx.simulate_keystrokes("d");
    hit(cx, "default", "copy");
    cx.simulate_keystrokes("d");
    assert!(!trashed(&sent()), "the d after Copy arms again");
    // F12 names no action in the default preset, and Focus.js still ends the arm on it.
    cx.simulate_keystrokes("f12 d");
    assert!(!trashed(&sent()), "the d after an unbound key arms again");
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":1}"#]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_new_listing_between_the_presses_ends_the_dd_pair(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    set_cursor(&shell, cx, 2);
    cx.simulate_keystrokes("d");
    // The listing lands again inside ARM_MS with no key between, so only the seat can end the arm.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let listed = Reply::Listed {
                path: "/files".into(),
                total: 3,
            };
            let rows = Reply::Rows {
                start: 0,
                rows: home(),
                listing: 2,
            };
            for reply in [listed, rows] {
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
            }
        })
    });
    cx.run_until_parked();
    // The cursor goes back to the armed row without a key, since any key would end the arm itself; only the listing differs.
    set_cursor(&shell, cx, 2);
    let (listing, cursor) = shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        (pane.held.listing, pane.cursor)
    });
    assert_eq!(
        (listing, cursor),
        (2, 2),
        "the new listing is held with the cursor on the armed row"
    );
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        Vec::<String>::new(),
        "the d on the new listing arms again"
    );
    cx.simulate_keystrokes("d");
    assert_eq!(
        sent(),
        [r#"{"c":"trash","rows":[2],"menuId":0,"listing":2}"#]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn a_stopped_backend_ends_the_delete_in_flight(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    let catalog = catalog(&shell, cx);
    cx.simulate_keystrokes("shift-delete");
    assert_eq!(sent().len(), 1, "the snapshot is out");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.receive(0, BackendEvent::Drained, window, cx)
        })
    });
    cx.run_until_parked();
    let stopped = catalog.tr_args("window.backend_stopped", &[""]);
    let deleting = shell.read_with(cx, |shell, _| shell.operations.fileops.deleting[0]);
    assert_eq!(deleting, None, "the stop ended the delete in flight");
    assert_ne!(
        status(&shell, cx),
        stopped,
        "only the second Shift+Delete says the backend stopped"
    );
    cx.simulate_keystrokes("shift-delete");
    assert_eq!(status(&shell, cx), stopped);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "fileops_review_tests.rs"]
mod review;

#[gpui::test]
fn ey421_a_created_filename_ending_in_undo_words_stays_whole(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-n");
    assert_eq!(
        sent(),
        [r#"{"c":"mkdir","path":"/files","defaultBase":"New Folder","request":1}"#]
    );
    reply(
        &shell,
        cx,
        r#"{"t":"made","request":1,"ok":true,"path":"/files/notes · z undoes"}"#,
    );
    let catalog = catalog(&shell, cx);
    let receipt = lines::made(&catalog, "default", "/files/notes · z undoes");
    assert_eq!(notice(&shell, cx), receipt, "wire receipt stays whole");
    let line = shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.line(&catalog, ""))
    });
    assert_eq!(
        line.primary, "Created notes · z undoes",
        "the filename is not advice"
    );
    assert_eq!(
        line.undo, "z undoes",
        "only the generated advice becomes an undo target"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[path = "newfolder_tests.rs"]
mod newfolder_tests;
