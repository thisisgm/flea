//! F2, every preset's rename key, the editor's commit, refusals and replies, an outside click, and the bulk card, on the production Shell.
#[path = "rename_hidden_tests.rs"]
mod hidden;
#[path = "rename_mouse_tests.rs"]
mod mouse;
#[path = "uish_def3_tests.rs"]
mod uish_def3_tests;
use super::test_shell::{keystroke, land, lists, open, row, set_cursor};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
    window::{input::event_input, u07_tests::press},
};
use flea::keys::{self, Context, Frontend, Match};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

pub(super) type Lines = Box<dyn FnMut() -> Option<String>>;

// The listing landed with the cursor on notes.txt, and the backend's request lines read back.
pub(super) fn ready<'a>(
    cx: &'a mut TestAppContext,
    preset: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext, Lines) {
    ready_with_root(cx, preset, false)
}

pub(super) fn ready_root<'a>(
    cx: &'a mut TestAppContext,
    preset: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext, Lines) {
    ready_with_root(cx, preset, true)
}

fn ready_with_root<'a>(
    cx: &'a mut TestAppContext,
    preset: &str,
    root: bool,
) -> (Entity<Shell>, &'a mut VisualTestContext, Lines) {
    let (shell, cx) = if root {
        super::test_shell::open_root(cx, preset)
    } else {
        open(cx, preset)
    };
    land(&shell, cx, "/files", home(), 1);
    set_cursor(&shell, cx, 2);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    (shell, cx, Box::new(lines))
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

pub(super) fn editing(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
) -> Option<(usize, String, String)> {
    shell.read_with(cx, |shell, _| {
        let edit = shell.rename.edit.as_ref()?;
        let row = shell.renaming_row(0)?;
        let selected = edit.entry.text[edit.entry.selected.clone()].to_owned();
        Some((row, edit.entry.text.clone(), selected))
    })
}

fn error(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().error.clone()
    })
}

// The footer's message line, where every rename sentence is said.
pub(super) fn notice(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.update(cx, |shell, cx| {
        shell.status_messages(cx, |messages, _| messages.notice().to_owned())
    })
}

pub(super) fn sent(lines: &mut Lines) -> Vec<String> {
    std::iter::from_fn(lines).collect()
}

pub(super) fn close(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
}

pub(super) fn receive(shell: &Entity<Shell>, cx: &mut VisualTestContext, event: BackendEvent) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.receive(0, event, window, cx)));
    cx.run_until_parked();
}

#[gpui::test]
fn f2_opens_the_stem_typing_replaces_it_and_enter_sends_the_rename(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    assert_eq!(
        editing(&shell, cx),
        Some((2, "notes.txt".into(), "notes".into())),
        "the editor opens on the cursor row with its stem selected"
    );
    let (frame, name_x, row_top) = shell.read_with(cx, |shell, _| {
        let list = shell.panes[0].as_ref().unwrap().list.borrow();
        let drawn = &list.drawn;
        let top = drawn.bounds.origin.y + gpui::px(2. * drawn.metrics.row - drawn.scroll_y);
        let frame = shell.rename.edit.as_ref().unwrap().line.frame;
        (
            frame,
            drawn.bounds.origin.x + gpui::px(drawn.layout.name.x),
            top,
        )
    });
    let frame = frame.expect("the field drew this frame");
    assert_eq!(frame.origin.x, name_x, "the frame sits on the label column");
    assert!(
        frame.origin.y > row_top,
        "the frame sits inside the cursor row"
    );
    let gate = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            let gate = shell.panes[0].as_ref().unwrap().nav.gate;
            (gate.editing, gate.rename_pending)
        })
    };
    assert_eq!(
        gate(cx),
        (true, false),
        "an open editor holds a sort and a re-read"
    );
    cx.simulate_input("draft");
    assert_eq!(editing(&shell, cx).unwrap().1, "draft.txt");
    let out = lists(&shell, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        gate(cx),
        (true, true),
        "the draft stays up with the request out"
    );
    assert_eq!(
        sent(&mut lines),
        [r#"{"c":"rename","path":"/files/notes.txt","to":"draft.txt","menuId":0}"#]
    );
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    assert_eq!(editing(&shell, cx), None, "the reply closes the editor");
    assert_eq!(gate(cx), (false, false), "the reply releases the gate");
    assert_eq!(lists(&shell, cx), out + 1, "the folder is read again");
    let select = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.pending_select.clone()
    });
    assert_eq!(
        select.as_deref(),
        Some("/files/draft.txt"),
        "the new name lands under the cursor"
    );
    close(&shell, cx);
}

#[gpui::test]
fn escape_abandons_an_unchanged_enter_sends_nothing_and_a_bad_name_stays_open(
    cx: &mut TestAppContext,
) {
    let (shell, cx, mut lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("x");
    cx.simulate_keystrokes("escape");
    assert_eq!(editing(&shell, cx), None);
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(editing(&shell, cx), None, "an unchanged name abandons");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("a/b");
    cx.simulate_keystrokes("enter");
    let invalid = shell.read_with(cx, |shell, _| {
        shell.locale.catalog.tr("jobs.rename.invalid")
    });
    assert_eq!(
        error(&shell, cx),
        invalid,
        "a refused name keeps the draft open"
    );
    assert_eq!(editing(&shell, cx).unwrap().1, "a/b.txt");
    assert_eq!(
        sent(&mut lines),
        Vec::<String>::new(),
        "nothing reached the backend"
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_refused_write_shows_its_reason_under_the_draft(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("alpha");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1);
    receive(
        &shell,
        cx,
        BackendEvent::Refused {
            operation: "rename".into(),
            path: "/files/notes.txt".into(),
            message: "file exists".into(),
            mode: 0,
        },
    );
    let exists = shell.read_with(cx, |shell, _| {
        shell
            .locale
            .catalog
            .tr_args("jobs.rename.exists", &["alpha.txt"])
    });
    assert_eq!(error(&shell, cx), exists);
    assert_eq!(
        editing(&shell, cx).unwrap().1,
        "alpha.txt",
        "the draft stays"
    );
    // The draft can be fixed and sent again.
    cx.simulate_input("2");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(&mut lines),
        [r#"{"c":"rename","path":"/files/notes.txt","to":"alpha2.txt","menuId":0}"#]
    );
    close(&shell, cx);
}

#[gpui::test]
fn a_press_on_another_row_commits_the_open_rename(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("memo");
    let centre = crate::window::u07_tests::read(&shell, cx, "rowCentre", &["0"]);
    press(cx, &centre, 1);
    assert_eq!(
        sent(&mut lines),
        [r#"{"c":"rename","path":"/files/notes.txt","to":"memo.txt","menuId":0}"#]
    );
    close(&shell, cx);
}

#[gpui::test]
fn every_preset_rename_key_opens_the_editor(cx: &mut TestAppContext) {
    let mut opened = 0;
    for preset in keys::PRESETS {
        for binding in keys::binding_rows(preset, Frontend::Gui).filter(|b| b.action == "rename") {
            let event = keystroke(binding);
            if keys::lookup(
                preset,
                Context::Listing,
                Frontend::Gui,
                event_input(&event, Modifiers::none()),
            ) != Match::Action("rename")
            {
                continue;
            }
            let (shell, cx, _) = ready(cx, preset);
            cx.simulate_event(event.clone());
            cx.run_until_parked();
            draw(cx);
            assert_eq!(
                editing(&shell, cx).map(|(row, ..)| row),
                Some(2),
                "{preset} {:?} opens the editor",
                event.keystroke
            );
            opened += 1;
            close(&shell, cx);
        }
    }
    assert!(
        opened >= 4,
        "F2, the mac Return and Enter, and r reach rename"
    );
}

// Each reshape resolves per-character bidi levels, so a Hebrew letter inside a Latin name keeps its own caret side.
#[gpui::test]
fn the_rename_field_resolves_bidi_levels_on_each_reshape(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("a\u{5e9}b");
    draw(cx);
    let (text, right_to_left, levels) = shell.read_with(cx, |shell, _| {
        let edit = shell.rename.edit.as_ref().unwrap();
        let line = &edit.line.line;
        (
            edit.entry.text.clone(),
            line.right_to_left,
            line.bidi_levels.clone(),
        )
    });
    assert_eq!(text, "a\u{5e9}b.txt");
    let mut expected = crate::path::field::Line::default();
    expected.set_text_direction(&text);
    assert!(
        !right_to_left,
        "a Latin first letter keeps the paragraph left to right"
    );
    assert_eq!(
        levels, expected.bidi_levels,
        "the drawn line holds the reshaped text's levels"
    );
    assert!(
        levels[1].is_rtl(),
        "the Hebrew letter resolves right to left"
    );
    close(&shell, cx);
}

// EY-464: a backend that dies with the rename out says the rename sentence in the pane's state slot too (PaneWire.qml onFailed).
#[gpui::test]
fn ey464_a_backend_that_dies_with_a_rename_out_names_the_rename_in_the_state_slot(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    receive(
        &shell,
        cx,
        BackendEvent::Failed(crate::backend::Failure {
            key: "window.backend_eof",
            detail: String::new(),
        }),
    );
    draw(cx);
    let (stopped, drawn, error) = shell.update(cx, |shell, cx| {
        let stopped = shell.locale.catalog.tr("jobs.rename.stopped");
        let drawn = shell.surfaces[0]
            .facts(cx)
            .map(|facts| facts.sentence.to_string());
        let errors = shell.status_messages(cx, |messages, _| {
            messages
                .errors()
                .iter()
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
        });
        (stopped, drawn, errors)
    });
    assert_eq!(
        drawn.as_deref(),
        Some(stopped.as_str()),
        "the state slot names the rename"
    );
    assert_eq!(error, [stopped], "the status bar says that sentence alone");
    assert_eq!(editing(&shell, cx), None, "the editor is closed");
    close(&shell, cx);
}
