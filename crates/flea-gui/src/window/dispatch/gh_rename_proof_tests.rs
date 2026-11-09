//! #305 and #265 consumer proof on the production Shell: Edit in Terminal hands the editor the names the card shows, and navigation answers around a rename.
use super::{
    gh_rename_proof_lane::{rooted, Lane},
    rename_tests::{close, draw, editing, ready, receive, sent, Lines},
    test_shell::{land_on, lists, open_root as open, row},
};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
    window::u07_tests::press,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn names(n: usize) -> Vec<String> {
    (0..n).map(|at| format!("note{at:03}.txt")).collect()
}

// A listing taller than the list, marked with v on its first row, its last row and the row above it, the first one scrolled out.
fn marked(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Lines, Vec<String>) {
    let (shell, cx) = open(cx, "default");
    marks(shell, cx)
}

fn marks(
    shell: Entity<Shell>,
    cx: &mut VisualTestContext,
) -> (Entity<Shell>, &mut VisualTestContext, Lines, Vec<String>) {
    let first = shell.read_with(cx, |shell, _| {
        let pending = shell.panes[0].as_ref().unwrap().pane.swap.pending.as_ref();
        pending.unwrap().first
    });
    let all = names(first);
    let rows = all.iter().map(|name| row(name, false)).collect();
    land_on(&shell, cx, 0, "/files", rows, first, 1);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    for keys in ["v", "G", "v", "k", "v"] {
        cx.simulate_keystrokes(keys);
        draw(cx);
    }
    let (selected, scroll, pitch) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let mut selected = host.selected.clone();
        selected.sort_unstable();
        let list = host.list.borrow();
        (selected, list.drawn.scroll_y, list.drawn.metrics.row)
    });
    assert_eq!(selected.len(), 3, "three rows are marked: {selected:?}");
    assert!(
        selected.contains(&0) && selected.contains(&(first - 1)),
        "{selected:?}"
    );
    assert!(
        scroll > pitch,
        "the first marked row is scrolled out of the list"
    );
    let picked = selected
        .iter()
        .map(|&at| format!("/files/{}", all[at]))
        .collect();
    (shell, cx, Box::new(lines), picked)
}

fn card_names(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    shell.read_with(cx, |shell, _| {
        shell
            .rename
            .bulk
            .as_ref()
            .map(|open| open.card.names.clone())
    })
}

fn card_open(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx))
}

// F2 on the marks asks their paths, and the backend's answer opens the card.
fn f2_card(shell: &Entity<Shell>, cx: &mut VisualTestContext, lines: &mut Lines, paths: &[String]) {
    cx.simulate_keystrokes("f2");
    // The paths request is a rows control, which the line probe does not carry; the Shell records the side it asked for.
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.rename.paths),
        Some(0),
        "F2 asks the marked rows' paths"
    );
    assert_eq!(
        sent(lines),
        Vec::<String>::new(),
        "no line request before the paths answer"
    );
    receive(shell, cx, BackendEvent::Paths(paths.to_vec()));
    assert!(card_open(shell, cx), "the bulk card opens");
    draw(cx);
}

fn leaves(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.rsplit('/').next().unwrap().to_string())
        .collect()
}

#[gpui::test]
fn gh305_enter_on_edit_in_terminal_hands_the_editor_the_shown_names(cx: &mut TestAppContext) {
    let lane = Lane::new();
    let (shell, cx, mut lines, paths) = marked(cx);
    f2_card(&shell, cx, &mut lines, &paths);
    // Find "note", replace with "memo": the card shows the replaced names before Edit in Terminal.
    cx.simulate_input("note");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("memo");
    let shown = card_names(&shell, cx).unwrap();
    assert!(
        shown.iter().all(|name| name.starts_with("memo")),
        "{shown:?}"
    );
    lane.reply(0, &["one.txt", "two.txt", "three.txt"]);
    // Replace, then the three name lines, then Edit in Terminal.
    for _ in 0..4 {
        cx.simulate_keystrokes("tab");
    }
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .bulk
            .as_ref()
            .unwrap()
            .card
            .focus),
        flea::ui_logic_open_with_properties::bulkrename::Stop::Edit
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(lane.launches(), 1, "Enter launched the editor once");
    assert_eq!(
        lane.captured(0).unwrap(),
        shown,
        "the editor got the names the card showed"
    );
    assert_eq!(
        card_names(&shell, cx).unwrap(),
        ["one.txt", "two.txt", "three.txt"],
        "the editor's lines became the new names"
    );
    assert!(lane.scratch_left().is_empty(), "{:?}", lane.scratch_left());
    // Edit, Cancel, Rename: Right twice reaches Rename, and Enter sends the editor's names for the marked paths.
    cx.simulate_keystrokes("right right enter");
    let out = sent(&mut lines);
    let request = flea::ui_logic_open_with_properties::bulkrename::request(
        &paths,
        &["one.txt".into(), "two.txt".into(), "three.txt".into()],
    )
    .unwrap();
    assert_eq!(out, [request], "the rename carries the editor's names");
    close(&shell, cx);
}

#[gpui::test]
fn gh305_cancel_and_reopen_hands_the_editor_fresh_names(cx: &mut TestAppContext) {
    let lane = Lane::new();
    let (shell, cx, mut lines, paths) = marked(cx);
    f2_card(&shell, cx, &mut lines, &paths);
    cx.simulate_input("note");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("draft");
    cx.simulate_keystrokes("escape");
    assert!(!card_open(&shell, cx), "Escape cancels the card");
    assert_eq!(
        sent(&mut lines),
        Vec::<String>::new(),
        "cancel sends nothing"
    );
    assert_eq!(lane.launches(), 0);
    f2_card(&shell, cx, &mut lines, &paths);
    assert_eq!(
        card_names(&shell, cx).unwrap(),
        leaves(&paths),
        "the reopened card starts from the old names"
    );
    for _ in 0..5 {
        cx.simulate_keystrokes("tab");
    }
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(lane.launches(), 1);
    assert_eq!(
        lane.captured(0).unwrap(),
        leaves(&paths),
        "the editor gets the reopened card's names, not the cancelled draft"
    );
    // No reply file: the editor left the names as they were.
    assert_eq!(card_names(&shell, cx).unwrap(), leaves(&paths));
    close(&shell, cx);
}

#[gpui::test]
fn gh305_a_press_on_edit_in_terminal_hands_the_editor_the_shown_names(cx: &mut TestAppContext) {
    let lane = Lane::new();
    let (shell, cx) = rooted(cx);
    let (shell, cx, mut lines, paths) = marks(shell, cx);
    f2_card(&shell, cx, &mut lines, &paths);
    cx.simulate_input("note");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("memo");
    let shown = card_names(&shell, cx).unwrap();
    // A presented frame, which is the one debug_bounds reads.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    eprintln!(
        "GH305 POINTER card={:?} scrim={:?}",
        cx.debug_bounds("op3-card"),
        cx.debug_bounds("op3-bulkrename-scrim")
    );
    let bounds = cx
        .debug_bounds("op3-button-0")
        .expect("the card drew its Edit in Terminal button");
    let centre = bounds.center();
    press(
        cx,
        &format!("{} {}", f32::from(centre.x), f32::from(centre.y)),
        1,
    );
    cx.run_until_parked();
    assert_eq!(
        lane.launches(),
        1,
        "a press on Edit in Terminal launched the editor"
    );
    assert_eq!(lane.captured(0).unwrap(), shown);
    assert!(card_open(&shell, cx), "the card stays up over the editor");
    close(&shell, cx);
}

// Rename is drawn disabled while no name has changed, so a click on it must leave the focus where it was.
#[gpui::test]
fn gh305_a_click_on_a_disabled_rename_leaves_the_focus_put(cx: &mut TestAppContext) {
    let lane = Lane::new();
    let (shell, cx) = rooted(cx);
    let (shell, cx, mut lines, paths) = marks(shell, cx);
    f2_card(&shell, cx, &mut lines, &paths);
    let focus = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell.rename.bulk.as_ref().unwrap().card.focus
        })
    };
    assert_eq!(
        focus(&shell, cx),
        flea::ui_logic_open_with_properties::bulkrename::Stop::Find
    );
    // A presented frame, which is the one debug_bounds reads.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("op3-button-2")
        .expect("the card drew its Rename button");
    let centre = bounds.center();
    press(
        cx,
        &format!("{} {}", f32::from(centre.x), f32::from(centre.y)),
        1,
    );
    cx.run_until_parked();
    assert_eq!(
        focus(&shell, cx),
        flea::ui_logic_open_with_properties::bulkrename::Stop::Find,
        "a click on the disabled Rename moved the focus"
    );
    assert_eq!(
        sent(&mut lines),
        Vec::<String>::new(),
        "no rename request went out"
    );
    assert_eq!(lane.launches(), 0);
    assert!(card_open(&shell, cx), "the card stays up");
    close(&shell, cx);
}

#[gpui::test]
fn gh265_navigation_answers_around_a_delayed_rename(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let cursor = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor)
    };
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1, "the rename is out");
    // The reply is held: Down while the request is out, recorded as observed.
    cx.simulate_keystrokes("up");
    draw(cx);
    let held = cursor(&shell, cx);
    eprintln!(
        "GH265 OBSERVED cursor_after_up_while_pending={held} editing={:?}",
        editing(&shell, cx).is_some()
    );
    // S2-4, v0.3.12 RenameField.qml: Escape cannot abandon the write that is out, so the editor stays and Up moves nothing.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        editing(&shell, cx).is_some(),
        "Escape keeps the pending editor"
    );
    cx.simulate_keystrokes("up");
    draw(cx);
    let moved = cursor(&shell, cx);
    eprintln!("GH265 OBSERVED cursor_after_escape_up_while_pending={moved}");
    assert_eq!(moved, held, "the editor holds the keys until the reply");
    let before = lists(&shell, cx);
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(lists(&shell, cx), before + 1, "the folder is read again");
    // The re-read is unanswered: Down is held under the re-read and lands once the listing does.
    let at = cursor(&shell, cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.busy()),
        "the re-read is out"
    );
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        cursor(&shell, cx),
        at,
        "the held rows do not move under the re-read"
    );
    let queued = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().input.queued.is_some()
        })
    };
    assert!(
        queued(&shell, cx),
        "Down is held for the re-read, not dropped"
    );
    land_on(&shell, cx, 0, "/files", reread(), 4, 2);
    assert!(!queued(&shell, cx), "the landing spends the held key");
    // The re-read keeps the renamed file on row 2 with a row below it, so the held Down shows it moved one row.
    assert_eq!(
        (at, cursor(&shell, cx)),
        (2, 3),
        "the held Down moves one row past the renamed file"
    );
    close(&shell, cx);
}

fn reread() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("draft.txt", false),
        row("zeta.txt", false),
    ]
}
