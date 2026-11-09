//! Renames the window cannot make: a row it has not read, and a bulk card whose backend cannot take the request.
use super::{
    previewtypes_kinds_tests::Root,
    rename_tests::{close, draw, editing, notice, ready, receive, sent, Lines},
    test_shell::{land_on, open, row, set_cursor},
};
use crate::{
    backend::{Backend, Event as BackendEvent},
    state::Shell,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

#[gpui::test]
fn f2_on_a_row_the_window_has_not_read_says_so(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    // Ten rows more than the first window: the cursor below it is inside the listing, not yet read.
    let first = shell.read_with(cx, |shell, _| {
        let pending = shell.panes[0].as_ref().unwrap().pane.swap.pending.as_ref();
        pending.unwrap().first
    });
    let rows = (0..first)
        .map(|at| row(&format!("folder{at}"), true))
        .collect();
    land_on(&shell, cx, 0, "/files", rows, first + 10, 1);
    set_cursor(&shell, cx, first + 5);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut lines: Lines = Box::new(lines);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .held
            .total),
        first + 10
    );
    cx.simulate_keystrokes("f2");
    assert_eq!(editing(&shell, cx), None);
    let said = shell.read_with(cx, |shell, _| {
        shell.locale.catalog.tr("jobs.target.not_loaded")
    });
    assert_eq!(notice(&shell, cx), said);
    assert_eq!(sent(&mut lines), Vec::<String>::new());
    close(&shell, cx);
}

// The pane's error line, where a failed request is reported.
fn pane_error(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Option<String> {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().error.clone()
    })
}

fn stopped(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.locale.catalog.tr("window.backend_stopped")
    })
}

#[gpui::test]
fn several_marked_rows_with_no_backend_say_the_backend_stopped(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready(cx, "default");
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 2];
        host.input.selection.changed(&host.selected);
    });
    // With the backend, this selection takes F2 to the bulk card.
    cx.simulate_keystrokes("f2");
    assert_eq!(shell.read_with(cx, |shell, _| shell.rename.paths), Some(0));
    receive(
        &shell,
        cx,
        BackendEvent::Paths(vec!["/files/beta".into(), "/files/notes.txt".into()]),
    );
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the bulk card opened"
    );
    cx.simulate_keystrokes("escape");
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    assert_eq!(pane_error(&shell, cx), None);
    // The same selection with no backend asks nothing and says the backend stopped.
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = None
    });
    cx.simulate_keystrokes("f2");
    assert_eq!(shell.read_with(cx, |shell, _| shell.rename.paths), None);
    assert_eq!(pane_error(&shell, cx), Some(stopped(&shell, cx)));
    close(&shell, cx);
}

// The shipped German catalog, read under a sandbox root that holds only its roster and language file.
fn shipped_german(root: &Root) -> std::sync::Arc<flea::i18n::I18n> {
    let locale = root.0.join("locale");
    std::fs::create_dir_all(locale.join("default/i18n")).unwrap();
    std::fs::write(
        locale.join("default/i18n/locales.json"),
        include_str!("../../../../../src/i18n/fixtures/locales.json"),
    )
    .unwrap();
    std::fs::write(locale.join("language"), "de").unwrap();
    std::sync::Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: locale.clone(),
        language_file: locale.join("language"),
        catalogs: vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n")],
    }))
}

#[gpui::test]
fn a_drive_name_refusal_under_the_draft_reads_in_the_window_language(cx: &mut TestAppContext) {
    let root = Root::new("rename-de");
    let (shell, cx, mut lines) = ready(cx, "default");
    shell.update(cx, |shell, _| shell.locale.catalog = shipped_german(&root));
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input(":");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1);
    receive(
        &shell,
        cx,
        BackendEvent::Refused {
            operation: "rename".into(),
            path: "/files/notes.txt".into(),
            message: "':' is not allowed in a name on this vfat drive".into(),
            mode: 0,
        },
    );
    let reason = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().error.clone()
    });
    assert_eq!(
        reason,
        "„:“ ist in einem Namen auf diesem vfat-Laufwerk nicht erlaubt."
    );
    assert_eq!(editing(&shell, cx).unwrap().1, ":.txt", "the draft stays");
    close(&shell, cx);
}

#[gpui::test]
fn a_bulk_rename_the_backend_never_took_says_so_as_the_card_closes(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = super::rename_tests::ready_root(cx, "default");
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 2];
        host.input.selection.changed(&host.selected);
    });
    cx.simulate_keystrokes("f2");
    receive(
        &shell,
        cx,
        BackendEvent::Paths(vec!["/files/beta".into(), "/files/notes.txt".into()]),
    );
    draw(cx);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = None
    });
    assert!(
        shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)),
        "the card opened"
    );
    cx.simulate_input("e");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("x");
    cx.simulate_keystrokes("enter");
    assert!(!shell.read_with(cx, |shell, cx| shell.card_holds_keyboard(cx)));
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.rename.sent),
        [false; 2]
    );
    draw(cx);
    assert_eq!(pane_error(&shell, cx), Some(stopped(&shell, cx)));
    close(&shell, cx);
}
