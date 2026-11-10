//! New File chosen from the folder's background menu opens its card, creates the file through v0.3.12's newfile request, and refuses in the card.
use super::{
    fileops_menu_tests::{choose, frame, id_of, opened},
    fileops_tests::reply,
    test_shell::{land, lists, row},
};
use crate::{
    backend::Backend,
    g2::op1::model::{CopyFocus, Dialog},
    state::Shell,
    window::{navigate::Step, u07_tests::read},
};
use flea::ui_logic_op1::lines;
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

// Every line the pane's backend is asked for, in the order it was asked.
/// The pane's backend becomes a recorder the test reads back in order
fn asked(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> impl FnMut() -> Vec<String> {
    let (backend, mut read) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    move || std::iter::from_fn(&mut read).collect()
}

/// Right click on the folder's blank space, where the background menu opens.
pub(super) fn background_menu(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let centre = read(shell, cx, "rowCentre", &["2"]);
    let mut values = centre
        .split_whitespace()
        .map(|value| value.parse::<f32>().unwrap());
    let (x, y) = (values.next().unwrap(), values.next().unwrap());
    cx.simulate_mouse_down(
        point(px(x), px(y + 200.)),
        MouseButton::Right,
        Modifiers::default(),
    );
    cx.run_until_parked();
}

// The card on the window's operations overlay, if one is open.
/// The dialog the operations overlay is showing, if any
fn card(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<Dialog> {
    shell.read_with(cx, |shell, cx| {
        shell
            .operation_view(0)
            .and_then(|view| view.read(cx).model.dialog.clone())
    })
}

// The created line the notice shows, as the operation sentences publish it.
/// The notice the footer shows for the last operation
fn notice(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.update(cx, |shell, cx| {
        let catalog = shell.locale.catalog.clone();
        shell.status_messages(cx, |messages, _| messages.notice_text(&catalog))
    })
}

/// Opens the New File card from the folder's background menu
fn open_card(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    background_menu(shell, cx);
    choose(shell, cx, "newFile");
    frame(cx);
}

/// The row opens its card with the name prefilled and sends nothing until Create
#[gpui::test]
fn new_file_row_opens_its_card_with_the_name_prefilled_and_sends_nothing(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    assert!(sent().is_empty(), "the card waits for Create");
    match card(&shell, cx) {
        Some(Dialog::NewFile {
            folder,
            name,
            focus: CopyFocus::Field,
            busy: false,
            error,
            ..
        }) => assert_eq!(
            (folder.as_str(), name.as_str(), error.as_str()),
            ("/files", "New File", "")
        ),
        other => panic!("New File opens its card: {other:?}"),
    }
    let field = flea::jsondoc::parse(&read(&shell, cx, "keyDeliveryState", &[])).unwrap();
    assert_eq!(
        field
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("QQuickTextInput")
    );
    cx.simulate_keystrokes("tab");
    frame(cx);
    let button = flea::jsondoc::parse(&read(&shell, cx, "keyDeliveryState", &[])).unwrap();
    assert_eq!(
        button
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("DialogButton")
    );
    assert!(sent().is_empty(), "focus traversal never creates a file");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// Create makes the file and selects it once, with no rename armed
#[gpui::test]
fn new_file_create_makes_the_file_and_selects_it_without_a_rename(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    // The prefilled name opens whole, so the first typed text replaces it.
    cx.simulate_input("notes");
    cx.simulate_keystrokes("enter");
    let requests = sent();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let id = id_of(&requests[0]);
    assert_eq!(
        requests,
        [format!(
            r#"{{"c":"newfile","op":"newFile","id":{id},"path":"/files","name":"notes"}}"#
        )]
    );
    assert!(matches!(
        card(&shell, cx),
        Some(Dialog::NewFile { busy: true, .. })
    ));
    cx.simulate_keystrokes("enter");
    assert!(sent().is_empty(), "a create already out sends nothing more");
    let before = lists(&shell, cx);
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"newFile","ok":true,"path":"/files/notes"}}"#
        ),
    );
    assert!(card(&shell, cx).is_none(), "the card closes on its reply");
    let catalog = shell.read_with(cx, |shell, _| shell.locale.catalog.clone());
    assert_eq!(
        notice(&shell, cx),
        lines::made(&catalog, "default", "/files/notes")
    );
    assert_eq!(lists(&shell, cx), before + 1, "the folder is read again");
    let select = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.pending_select.clone()
    });
    assert_eq!(
        select.as_deref(),
        Some("/files/notes"),
        "the new file lands under the cursor"
    );
    assert!(
        shell.read_with(cx, |shell, _| shell.operations.fileops.arrival[0].is_none()),
        "New File never arms the inline rename of a new folder"
    );
    assert!(
        shell.read_with(cx, |shell, _| shell.rename.edit.is_none()),
        "nothing opens for rename before the new file lands"
    );
    let rows = vec![row("alpha", true), row("beta", true), row("notes", false)];
    land(&shell, cx, "/files", rows, 2);
    let cursor = shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor);
    assert_eq!(cursor, 2, "the new file holds the cursor");
    assert!(shell.read_with(cx, |shell, _| shell.rename.edit.is_none()));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// Escape and Cancel close the card without a request
#[gpui::test]
fn new_file_escape_and_cancel_close_without_a_request(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    cx.simulate_keystrokes("escape");
    assert!(card(&shell, cx).is_none(), "Escape closes the card");
    open_card(&shell, cx);
    // Tab moves the field to Cancel, and Enter on Cancel closes.
    cx.simulate_keystrokes("tab enter");
    assert!(card(&shell, cx).is_none(), "Cancel closes the card");
    assert!(sent().is_empty(), "closing sends no request");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// Pane.qml act: Recent is a history, so New File refuses there before any card opens.
/// Recent refuses New File before any card opens
#[gpui::test]
fn new_file_in_recent_refuses_before_any_card(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    let recent = flea::ui_logic_pk1::RECENT;
    let done = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.step(0, Step::Open(recent.into()), window, cx)
        })
    });
    assert_eq!(done, Ok(true));
    land(
        &shell,
        cx,
        recent,
        vec![row("alpha", true), row("beta", true), row("notes", false)],
        2,
    );
    // The walk into Recent asks the backend its own lines; only what New File asks counts here.
    let _ = sent();
    background_menu(&shell, cx);
    choose(&shell, cx, "newFile");
    frame(cx);
    assert!(
        card(&shell, cx).is_none(),
        "Recent cannot hold a new file, so no card opens"
    );
    let requests = sent();
    assert!(
        requests.is_empty(),
        "New File in Recent asks the backend nothing: {requests:?}"
    );
    assert_eq!(
        read(&shell, cx, "lastMessage", &[]),
        "Recent cannot hold a new file. Open a folder first"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// MenuActionDialog.qml:170 refuses "." and ".." with the slash sentence, so the card does too.
/// A dot name refuses in the card with the slash sentence
#[gpui::test]
fn new_file_dot_names_refuse_in_the_card_with_the_slash_sentence(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    for dot in [".", ".."] {
        open_card(&shell, cx);
        cx.simulate_input(dot);
        cx.simulate_keystrokes("enter");
        assert!(sent().is_empty(), "{dot} never reaches the backend");
        match card(&shell, cx) {
            Some(Dialog::NewFile {
                busy: false,
                name,
                error,
                ..
            }) => {
                assert_eq!(
                    (name.as_str(), error.as_str()),
                    (dot, "Enter a name without a /.")
                );
            }
            other => panic!("{dot} stays in the card: {other:?}"),
        }
        cx.simulate_keystrokes("escape");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// A refused create keeps the card open with its sentence
#[gpui::test]
fn new_file_refusal_keeps_the_card_open_with_its_sentence(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    cx.simulate_keystrokes("enter");
    let requests = sent();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let id = id_of(&requests[0]);
    let sentence = "Could not create /files/New File: File exists.";
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"newFile","ok":false,"error":"{sentence}"}}"#
        ),
    );
    match card(&shell, cx) {
        Some(Dialog::NewFile {
            busy: false, error, ..
        }) => assert_eq!(error, sentence),
        other => panic!("a refused create keeps its card open: {other:?}"),
    }
    assert!(sent().is_empty(), "a refusal sends nothing more");
    let catalog = shell.read_with(cx, |shell, _| shell.locale.catalog.clone());
    assert_ne!(
        notice(&shell, cx),
        lines::made(&catalog, "default", "/files/New File")
    );
    cx.simulate_keystrokes("escape");
    assert!(card(&shell, cx).is_none());
    assert!(sent().is_empty(), "Escape after a refusal sends no request");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// A name with a slash is refused in the card
#[gpui::test]
fn new_file_name_with_a_slash_is_refused_in_the_card(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    cx.simulate_input("a/b");
    cx.simulate_keystrokes("enter");
    assert!(
        sent().is_empty(),
        "a name with a slash never reaches the backend"
    );
    match card(&shell, cx) {
        Some(Dialog::NewFile {
            busy: false,
            name,
            error,
            ..
        }) => {
            assert_eq!(name, "a/b");
            assert_eq!(error, "Enter a name without a /.");
        }
        other => panic!("the slash refusal stays in the card: {other:?}"),
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// The TUI's create rule in the card: one trailing slash asks mkdir for the folder it names.
/// One trailing slash asks mkdir for the folder it names
#[gpui::test]
fn new_file_trailing_slash_makes_a_folder_through_mkdir(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    cx.simulate_input("photos/");
    cx.simulate_keystrokes("enter");
    let requests = sent();
    assert_eq!(requests.len(), 1, "{requests:?}");
    // A mkdir carries its correlation as request, never the newFile reply's id.
    let id = flea::json::field_usize(&requests[0], "request").unwrap();
    assert_eq!(
        requests,
        [format!(
            r#"{{"c":"mkdir","path":"/files","name":"photos","request":{id}}}"#
        )],
        "the slash decides the request and never reaches the name"
    );
    assert!(matches!(
        card(&shell, cx),
        Some(Dialog::NewFile { busy: true, .. })
    ));
    let before = lists(&shell, cx);
    // A correlated made line is the card's: the pane says nothing and re-reads nothing for it.
    reply(
        &shell,
        cx,
        &format!(r#"{{"t":"made","ok":true,"path":"/files/photos","request":{id}}}"#),
    );
    assert!(
        matches!(card(&shell, cx), Some(Dialog::NewFile { busy: true, .. })),
        "the made line alone keeps the card waiting for its receipt"
    );
    assert_eq!(lists(&shell, cx), before, "the pane leaves it to the card");
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"creation","id":{id},"kind":"made","ok":true,"path":"/files/photos"}}"#
        ),
    );
    assert!(card(&shell, cx).is_none(), "the receipt closes the card");
    let catalog = shell.read_with(cx, |shell, _| shell.locale.catalog.clone());
    assert_eq!(
        notice(&shell, cx),
        lines::made(&catalog, "default", "/files/photos")
    );
    assert_eq!(lists(&shell, cx), before + 1, "the folder is read again");
    let select = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.pending_select.clone()
    });
    assert_eq!(
        select.as_deref(),
        Some("/files/photos"),
        "the new folder lands under the cursor"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

/// A refused folder keeps the card open with its sentence
#[gpui::test]
fn new_file_folder_refusal_keeps_the_card_open_with_its_sentence(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    open_card(&shell, cx);
    cx.simulate_input("photos/");
    cx.simulate_keystrokes("enter");
    let requests = sent();
    assert_eq!(requests.len(), 1, "{requests:?}");
    let id = flea::json::field_usize(&requests[0], "request").unwrap();
    let sentence = "a folder or file with that name already exists";
    reply(
        &shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","op":"creation","id":{id},"kind":"made","ok":false,"error":"{sentence}"}}"#
        ),
    );
    match card(&shell, cx) {
        Some(Dialog::NewFile {
            busy: false, error, ..
        }) => assert_eq!(error, sentence),
        other => panic!("a refused folder keeps its card open: {other:?}"),
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

// A second slash and a lone one still name nothing the rule can file.
/// A second slash and a lone slash still refuse in the card
#[gpui::test]
fn new_file_folder_drafts_refuse_a_second_or_a_lone_slash(cx: &mut TestAppContext) {
    let (shell, mut cx) = opened(cx);
    let cx = &mut cx;
    let mut sent = asked(&shell, cx);
    for draft in ["photos//", "/"] {
        open_card(&shell, cx);
        cx.simulate_input(draft);
        cx.simulate_keystrokes("enter");
        assert!(sent().is_empty(), "{draft} never reaches the backend");
        match card(&shell, cx) {
            Some(Dialog::NewFile {
                busy: false,
                name,
                error,
                ..
            }) => {
                assert_eq!(name, draft);
                assert_eq!(error, "Enter a name without a /.");
            }
            other => panic!("{draft} stays in the card: {other:?}"),
        }
        cx.simulate_keystrokes("escape");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
