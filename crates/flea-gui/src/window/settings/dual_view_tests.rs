//! S2-10: Dual is remembered as the standing view, as v0.3.12's Pane.qml chooseView wrote it, so a relaunch reopens the pair.
use crate::{backend::Backend, window::startup::View};
use flea::jsondoc::Json;
use gpui::TestAppContext;

fn standing(data: &Json) -> Option<&str> {
    data.get("view").and_then(Json::as_str)
}

#[gpui::test]
fn s2_10_choosing_dual_writes_the_standing_view_a_relaunch_reads(cx: &mut TestAppContext) {
    // Dual constructs a second pane; keep both backends on the test scheduler.
    cx.update(crate::state::constructor_fixture::isolated_services);
    for per_folder in [false, true] {
        let (backend, _requests) = Backend::test_channel();
        let (entity, cx) = cx.add_window_view(|_, cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.launch.settings.data = flea::uistate::from_file(&format!(
                r#"{{"view":"list","rememberViewPerFolder":{per_folder},"folderViews":{{}}}}"#
            ));
            shell
        });
        entity.update_in(cx, |shell, window, cx| {
            shell.choose_settings_view(View::Dual, window, cx);
            let data = &shell.launch.settings.data;
            assert_eq!(standing(data), Some("dual"), "per folder {per_folder}");
            let folders = data.get("folderViews").and_then(Json::as_object);
            assert!(
                folders.is_none_or(|folders| folders.is_empty()),
                "Dual is no folder's view: {folders:?}"
            );
            let relaunch = crate::window::startup::Settings::from_json(data.clone());
            assert_eq!(relaunch.view, View::Dual, "a relaunch opens the pair");
        });
    }
}

#[gpui::test]
fn s2_10_leaving_dual_sets_the_standing_view_even_per_folder(cx: &mut TestAppContext) {
    cx.update(crate::state::constructor_fixture::isolated_services);
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|_, cx| {
        let mut shell = crate::state::tests::shell_with(backend, cx);
        shell.launch.settings.data = flea::uistate::from_file(
            r#"{"view":"dual","rememberViewPerFolder":true,"folderViews":{}}"#,
        );
        shell.view = View::Dual;
        shell
    });
    entity.update_in(cx, |shell, window, cx| {
        shell.choose_settings_view(View::Columns, window, cx);
        let data = &shell.launch.settings.data;
        assert_eq!(standing(data), Some("columns"), "Dual no longer stands");
        let path = shell.panes[0].as_ref().unwrap().path.clone();
        assert_eq!(
            flea::ui_logic_settings_pages::pages::at(data, &["folderViews", path.as_str()])
                .and_then(Json::as_str),
            Some("columns"),
            "the folder keeps its own view too"
        );
        // Outside Dual a per-folder choice leaves the standing view alone, as before.
        shell.choose_settings_view(View::Grid, window, cx);
        assert_eq!(standing(&shell.launch.settings.data), Some("columns"));
    });
}
