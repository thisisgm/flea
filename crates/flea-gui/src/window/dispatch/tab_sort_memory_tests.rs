//! A tab coming back on its folder shows its own order and leaves the last explicit sort remembered.
use super::{
    railrecent_tests::{frame, read},
    test_shell::{land, open_root_with, row},
};
use crate::{navigation::folder_state::Order, state::Shell};
use flea::jsondoc::Json;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn pair(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let listing = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.greatest_listing + 1
    });
    land(
        shell,
        cx,
        "/files",
        vec![row("A", true), row("notes.txt", false)],
        listing,
    );
    frame(cx);
}

fn sort_writes(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Vec<String> {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let writer = host.nav.writer.borrow();
        writer
            .patches
            .iter()
            .filter(|patch| patch.contains("\"folderSorts\"") || patch.contains("\"sort\""))
            .cloned()
            .collect()
    })
}

#[gpui::test]
fn b16_a_tab_switch_keeps_the_last_explicit_sort_and_its_direction(cx: &mut TestAppContext) {
    let desc = Order {
        key: "name",
        desc: true,
    };
    for remember in [true, false] {
        let settings = format!(
            r#""keys":"default","view":"list","newTab":"current","rememberSort":{remember},"sort":{{"key":"name","reverse":false}}"#
        );
        let (shell, cx) = open_root_with(cx, &settings, None);
        pair(&shell, cx);
        cx.simulate_keystrokes("t");
        frame(cx);
        if read(&shell, cx, "listInFlight") == "true" {
            pair(&shell, cx);
        }
        assert_eq!(read(&shell, cx, "tabCount"), "2");
        assert_eq!(read(&shell, cx, "tabIndex"), "1");
        cx.simulate_keystrokes("shift-s");
        assert_eq!(read(&shell, cx, "sortMark"), "name:desc");
        pair(&shell, cx);
        let chosen = sort_writes(&shell, cx);
        assert_eq!(chosen.len(), 1, "remember={remember}: {chosen:?}");
        // Back on the first tab, which still shows the folder ascending as it left it.
        cx.simulate_keystrokes("1");
        frame(cx);
        assert_eq!(read(&shell, cx, "tabIndex"), "0");
        assert_eq!(read(&shell, cx, "sortMark"), "name:asc");
        pair(&shell, cx);
        assert_eq!(
            sort_writes(&shell, cx),
            chosen,
            "remember={remember}: a tab switch wrote a sort"
        );
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let folders = host.nav.folders.borrow();
            assert_eq!(folders.order_for("/files"), desc, "remember={remember}");
            let global = shell.launch.settings.data.get("sort").unwrap();
            assert_eq!(
                global.get("reverse").and_then(Json::as_bool),
                Some(!remember),
                "remember={remember}"
            );
        });
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    }
}

#[path = "dual_folder_sort_tests.rs"]
mod dual_folder_sort_tests;
