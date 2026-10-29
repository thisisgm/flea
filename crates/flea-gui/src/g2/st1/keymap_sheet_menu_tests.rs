//! Boards keymap-query and keymap-dead in the live window: the sheet finds the cursor row's menu rows and says why a key is dead.
use super::tests::english;
use crate::{backend::Backend, state::Shell};
use gpui::TestAppContext;

const REGULAR: u32 = 0o100_644;
const SYMLINK: u32 = 0o120_777;

// Defaults hide Permissions and Copy As, so a row found here is one Settings hides.
fn shell(backend: Backend, mode: u32, start: usize, cx: &mut gpui::Context<Shell>) -> Shell {
    let mut shell = crate::state::tests::shell_with(backend, cx);
    shell.launch.settings.data = flea::uischema::defaults();
    shell.locale.catalog = english();
    // A file of this mode in every held row, and a backend whose probe found zip and 7z, as on the boards.
    let file = flea::protocol::RowFacts {
        name: "notes.txt".into(),
        mode,
        ..Default::default()
    };
    let host = shell.panes[0].as_mut().expect("the test pane");
    let held = &host.pane.held;
    host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
        listing: held.listing,
        total: held.total + start,
        start: held.start + start,
        rows: vec![file; held.rows.len()],
    });
    shell.output.archive[0] = Some(vec!["zip".into(), "7z".into()]);
    shell
}

fn searched(query: &str, cx: &mut TestAppContext) -> Vec<String> {
    searched_on(query, REGULAR, cx)
}

fn searched_on(query: &str, mode: u32, cx: &mut TestAppContext) -> Vec<String> {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let shell = shell(backend, mode, 0, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| shell.open_sheet(window, cx));
    cx.run_until_parked();
    let keys = query
        .chars()
        .map(String::from)
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&keys);
    cx.run_until_parked();
    entity.update(cx, |shell, cx| {
        let view = shell.settings.sheet.clone().expect("the sheet is open");
        let sheet = &view.read(cx).sheet;
        assert_eq!(sheet.query, query);
        sheet
            .ranked
            .iter()
            .map(|row| {
                let place = row.where_.resolve(&sheet.catalog);
                let place = if place.is_empty() {
                    String::new()
                } else {
                    format!(" {}", sheet.catalog.tr_args("settings.sheet.in", &[&place]))
                };
                let dead = if row.disabled {
                    format!(" [{}]", row.reason.resolve(&sheet.catalog))
                } else {
                    String::new()
                };
                format!(
                    "{}|{}{place}{dead}",
                    row.keys,
                    row.label.resolve(&sheet.catalog)
                )
            })
            .collect()
    })
}

#[gpui::test]
fn keymap_sheet_perm_finds_permissions_in_the_cursor_menu(cx: &mut TestAppContext) {
    assert_eq!(
        searched("perm", cx),
        ["shift-delete|Delete permanently", "|Permissions…"]
    );
}

#[gpui::test]
fn keymap_sheet_pa_lists_menu_leaves_and_dims_paste_with_its_reason(cx: &mut TestAppContext) {
    let rows = searched("pa", cx);
    for row in [
        "p / ctrl-v|Paste [Nothing to paste]",
        "P|Paste as [Nothing to paste]",
        "|With Password… in Compress",
        "|Path in Copy As",
        "|Folder Path in Copy As",
    ] {
        assert!(rows.iter().any(|drawn| drawn == row), "{row} in {rows:#?}");
    }
}

#[gpui::test]
fn keymap_sheet_dead_menu_row_carries_the_menus_own_reason(cx: &mut TestAppContext) {
    let rows = searched_on("perm", SYMLINK, cx);
    assert!(
        rows.iter().any(|drawn| drawn == "|Permissions… [symlink]"),
        "{rows:#?}"
    );
}

#[gpui::test]
fn keymap_sheet_reads_the_cursor_rows_menu_when_the_cursor_is_scrolled_out(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| shell(backend, REGULAR, 40, cx));
    entity.update(cx, |shell, _| {
        let host = shell.panes[0].as_ref().expect("the test pane");
        assert!(host.pane.held.row(host.pane.cursor).is_none());
        let actions: Vec<String> = shell
            .sheet_menus()
            .into_iter()
            .map(|entry| entry.action)
            .collect();
        assert!(
            actions.iter().any(|action| action == "rename"),
            "{actions:?}"
        );
    });
}

#[gpui::test]
fn keymap_sheet_question_mark_lists_its_own_row(cx: &mut TestAppContext) {
    assert_eq!(searched("?", cx), ["?|Keyboard Shortcuts"]);
}

#[gpui::test]
fn keymap_sheet_keyboard_lists_its_own_row(cx: &mut TestAppContext) {
    let rows = searched("keyboard", cx);
    assert!(
        rows.iter().any(|drawn| drawn == "?|Keyboard Shortcuts"),
        "{rows:#?}"
    );
}

#[gpui::test]
fn keymap_sheet_single_symbol_keys_list_their_rows(cx: &mut TestAppContext) {
    for (query, row) in [
        ("/", "/|Filter"),
        (":", ": / ctrl-l|Go to folder"),
        ("{", "{|Move tab left"),
        ("}", "}|Move tab right"),
    ] {
        let rows = searched(query, cx);
        assert!(rows.iter().any(|drawn| drawn == row), "{query}: {rows:#?}");
    }
}

#[gpui::test]
fn keymap_sheet_escape_closes_it_from_a_typed_query(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let shell = shell(backend, REGULAR, 0, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| shell.open_sheet(window, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("?");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        entity.read_with(cx, |shell, _| shell.settings.sheet.is_none()),
        "Escape closes the sheet"
    );
}
