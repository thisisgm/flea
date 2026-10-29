//! Pane state changes observed after input and backend replies through the mounted Root.
use super::{
    menus_tests::settle,
    test_shell::{land, open_root_with, row},
};
use gpui::TestAppContext;

#[gpui::test]
#[ignore = "BLOCKED-DESIGN: no HIG empty board draws the key-hint line; waits for a design ruling after 0.4.0"]
fn pb_pane_empty_folder_hint_follows_the_key_hints_setting(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","keyHints":true"#, None);
    land(&shell, cx, "/files", vec![], 1);
    settle(cx);
    assert!(
        cx.debug_bounds("fb1-empty-key-hint").is_some(),
        "the empty folder draws its hint"
    );
    let hint = shell.read_with(cx, |shell, cx| {
        shell.surfaces[0]
            .hero
            .as_ref()
            .unwrap()
            .read(cx)
            .painted
            .get("hint")
            .unwrap()
            .text
            .to_string()
    });
    assert_eq!(hint, "Press N for a new folder.");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_empty_folder_hint_stays_off_by_default(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list""#, None);
    land(&shell, cx, "/files", vec![], 1);
    settle(cx);
    assert!(cx.debug_bounds("fb1-empty-key-hint").is_none());
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_global_sort_is_saved_after_landing_with_remember_sort_off(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","rememberSort":false"#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    cx.simulate_keystrokes("s");
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .writer
            .borrow()
            .patches
            .clone()
    });
    assert!(
        !before.iter().any(|patch| patch.contains("\"sort\"")),
        "an unlanded order is not persisted"
    );
    land(&shell, cx, "/files", vec![row("folder", true)], 2);
    let patches = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .writer
            .borrow()
            .patches
            .clone()
    });
    assert!(
        patches
            .iter()
            .any(|patch| patch == r#"{"sort":{"key":"size","reverse":false}}"#),
        "landed sort patches: {patches:?}"
    );
    cx.simulate_keystrokes("enter");
    land(&shell, cx, "/files/folder", vec![row("file", false)], 3);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .drawn_order()
            .key),
        "size"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_pane_remembered_sort_does_not_replace_the_global_default(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","rememberSort":true"#, None);
    land(&shell, cx, "/files", vec![row("folder", true)], 1);
    cx.simulate_keystrokes("s");
    land(&shell, cx, "/files", vec![row("folder", true)], 2);
    let patches = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .nav
            .writer
            .borrow()
            .patches
            .clone()
    });
    assert!(patches
        .iter()
        .any(|patch| patch.contains("\"folderSorts\"")));
    assert!(!patches.iter().any(|patch| patch.contains("\"sort\"")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
