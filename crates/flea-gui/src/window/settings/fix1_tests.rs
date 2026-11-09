use super::store::Update;
use crate::{backend::Backend, window::startup::View};
use flea::jsondoc::{self, Json};
use gpui::{AppContext, TestAppContext};

fn shell(backend: Backend, cx: &mut gpui::Context<crate::state::Shell>) -> crate::state::Shell {
    let mut shell = crate::state::tests::shell_with(backend, cx);
    shell.launch.settings.data = flea::uischema::defaults();
    shell
}

fn patch(text: &str) -> Json {
    jsondoc::parse(text).unwrap()
}

#[gpui::test]
fn u11_fix1_a_queued_retry_never_restores_an_older_choice(cx: &mut TestAppContext) {
    for queued_before_failure in [false, true] {
        let (backend, _requests) = Backend::test_channel();
        let (writes, queued) = std::sync::mpsc::channel();
        let (_updates, updates) = async_channel::unbounded();
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let entity = cx.new(|cx| shell(backend, cx));
            entity.update(cx, |shell, cx| {
                shell.settings.worker = Some(super::store::Worker { writes, updates });
                shell.change_settings(patch(r#"{"density":"normal"}"#), window, cx);
                queued.try_recv().unwrap();
                if !queued_before_failure {
                    shell.receive_settings(
                        Update::Failed {
                            through: 1,
                            keep: true,
                        },
                        window,
                        cx,
                    );
                }
                shell.change_settings(patch(r#"{"density":"comfortable"}"#), window, cx);
                let newer = queued.try_recv().unwrap();
                if queued_before_failure {
                    shell.receive_settings(
                        Update::Failed {
                            through: 1,
                            keep: true,
                        },
                        window,
                        cx,
                    );
                }
                shell.change_settings(patch(r#"{"hidden":true}"#), window, cx);
                let retry = queued.try_recv().unwrap();
                assert_eq!(
                    retry.1.get("density").and_then(Json::as_str),
                    Some("comfortable")
                );
                let disk = flea::uistate::patched(&flea::uischema::defaults(), &newer.1).unwrap();
                shell.receive_settings(
                    Update::Loaded {
                        document: disk.clone(),
                        through: 2,
                        saved: true,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    shell
                        .launch
                        .settings
                        .data
                        .get("density")
                        .and_then(Json::as_str),
                    Some("comfortable")
                );
                shell.receive_settings(
                    Update::Loaded {
                        document: flea::uistate::patched(&disk, &retry.1).unwrap(),
                        through: 3,
                        saved: true,
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    shell
                        .launch
                        .settings
                        .data
                        .get("density")
                        .and_then(Json::as_str),
                    Some("comfortable")
                );
                assert!(shell.settings.failed.is_empty());
                assert!(queued.try_recv().is_err());
            });
        });
    }
}

#[gpui::test]
fn u11_fix1_claim_note_stays_one_line_while_regular_hints_wrap(cx: &mut TestAppContext) {
    use flea::ui_logic_settings_pages::{rows::Row, Text};
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell(backend, cx);
        shell.sync_font_settings(cx);
        shell.open_settings(window, cx);
        shell.settings.about_open = true;
        shell.settings.claim.note = Some((
            Text::data("Long claim outcome ".repeat(60)),
            Default::default(),
        ));
        shell.sync_settings_claim(cx);
        let panel = shell.settings.panel.as_ref().unwrap();
        panel.update(cx, |view, _| view.panel.show_section("about"));
        shell
    });
    // The first layout settles the text scale, whose rebuild drops any row appended before it.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    entity.update(cx, |shell, cx| {
        let panel = shell.settings.panel.as_ref().unwrap();
        panel.update(cx, |view, cx| {
            view.panel.rows.push(Row {
                id: "wrap-control".into(),
                caption: Text::data("Caption control"),
                ..Row::hint(Text::data("Regular wrapping hint ".repeat(60)))
            });
            cx.notify();
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    entity.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        let at = panel
            .rows
            .iter()
            .position(|row| row.id == "makeDefault")
            .unwrap()
            + 1;
        assert_eq!(
            panel.lines[at], 1,
            "the actual claim note must measure as one eliding line"
        );
        assert_eq!(panel.heights()[at], 24.);
        assert_eq!(
            crate::g2::settings_pages::geometry::row_height(
                &panel.rows[at],
                false,
                80,
                crate::g2::settings_pages::geometry::Scale::default(),
            ),
            24.
        );
        let wrap = panel.rows.iter().position(|row| row.id == "wrap-control");
        let wrap = wrap.expect("the wrapping control row is laid out");
        assert!(panel.lines[wrap] > 1, "ordinary hints must still wrap");
        use crate::ipc::Adapter;
        let reply = crate::ipc::settings::SettingsAdapter {
            host: &shell.settings,
            document: &shell.launch.settings.data,
            hidden: false,
            cx,
        }
        .call("settingsModel", &[])
        .unwrap();
        let model = jsondoc::parse(&reply.stdout).unwrap();
        let control = model.as_array().unwrap().last().unwrap();
        assert_eq!(
            control.get("caption").and_then(Json::as_str),
            Some("Caption control")
        );
        assert_eq!(control.get("elide"), Some(&Json::Null));
    });
}

#[gpui::test]
fn u11_fix1_failed_retry_waits_for_its_own_acknowledgement(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (writes, queued) = std::sync::mpsc::channel();
    let (_updates, updates) = async_channel::unbounded();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.settings.worker = Some(super::store::Worker { writes, updates });
            shell.change_settings(patch(r#"{"density":"normal"}"#), window, cx);
            shell.change_settings(patch(r#"{"hidden":true}"#), window, cx);
            assert_eq!(
                queued.try_recv().unwrap().1,
                patch(r#"{"density":"normal"}"#)
            );
            assert_eq!(queued.try_recv().unwrap().1, patch(r#"{"hidden":true}"#));
            shell.receive_settings(
                Update::Failed {
                    through: 1,
                    keep: true,
                },
                window,
                cx,
            );
            shell.receive_settings(
                Update::Loaded {
                    document: flea::uistate::from_file(r#"{"hidden":true}"#),
                    through: 2,
                    saved: true,
                },
                window,
                cx,
            );
            assert_eq!(shell.settings.failed.len(), 1);
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("density")
                    .and_then(Json::as_str),
                Some("normal")
            );
            assert!(
                queued.try_recv().is_err(),
                "replies must not start an automatic retry"
            );
            shell.change_settings(patch(r#"{"keys":"windows"}"#), window, cx);
            let retry = queued.try_recv().unwrap();
            assert_eq!(retry.0, 3);
            assert_eq!(
                retry.1.get("density").and_then(Json::as_str),
                Some("normal")
            );
            shell.change_settings(patch(r#"{"foldersFirst":false}"#), window, cx);
            queued.try_recv().unwrap();
            shell.receive_settings(
                Update::Failed {
                    through: 3,
                    keep: true,
                },
                window,
                cx,
            );
            shell.receive_settings(
                Update::Loaded {
                    document: flea::uistate::from_file(
                        r#"{"density":"normal","hidden":true,"foldersFirst":false}"#,
                    ),
                    through: 4,
                    saved: true,
                },
                window,
                cx,
            );
            assert_eq!(
                shell.settings.failed.len(),
                1,
                "request 4 included failure 1, but not later failure 3"
            );
            assert_eq!(shell.settings.failed[0].0, 3);
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("keys")
                    .and_then(Json::as_str),
                Some("windows")
            );
            assert!(queued.try_recv().is_err());
            shell.change_settings(patch(r#"{"hiddenLast":true}"#), window, cx);
            let retry = queued.try_recv().unwrap();
            assert_eq!(retry.1.get("keys").and_then(Json::as_str), Some("windows"));
            let stored = flea::uistate::patched(&shell.launch.settings.data, &retry.1).unwrap();
            shell.receive_settings(
                Update::Loaded {
                    document: stored,
                    through: 5,
                    saved: true,
                },
                window,
                cx,
            );
            assert!(shell.settings.failed.is_empty());
            assert!(shell.settings.pending.is_empty());
            shell.receive_settings(
                Update::Loaded {
                    document: flea::uistate::from_file(r#"{"density":"tight","keys":"default"}"#),
                    through: 5,
                    saved: false,
                },
                window,
                cx,
            );
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("density")
                    .and_then(Json::as_str),
                Some("tight")
            );
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("keys")
                    .and_then(Json::as_str),
                Some("default")
            );
            assert!(queued.try_recv().is_err());
        });
    });
}

#[gpui::test]
fn u11_fix1_newer_success_supersedes_only_its_written_fields(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (writes, _queued) = std::sync::mpsc::channel();
    let (_updates, updates) = async_channel::unbounded();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.settings.worker = Some(super::store::Worker { writes, updates });
            shell.change_settings(patch(r#"{"density":"normal","display":{"textSize":{"mode":20},"hyprlandIcons":true}}"#), window, cx);
            shell.change_settings(patch(r#"{"density":"comfortable","display":{"textSize":{"mode":14}}}"#), window, cx);
            shell.receive_settings(Update::Failed { through: 1, keep: true }, window, cx);
            shell.receive_settings(Update::Loaded {
                document: flea::uistate::from_file(r#"{"density":"comfortable","display":{"textSize":{"mode":14}}}"#), through: 2, saved: true,
            }, window, cx);
            assert_eq!(shell.launch.settings.data.get("density").and_then(Json::as_str), Some("comfortable"));
            let size = flea::ui_logic_settings_pages::pages::at(&shell.launch.settings.data, &["display", "textSize"]).unwrap();
            assert_eq!(size.get("mode").and_then(Json::as_f64), Some(14.));
            assert_eq!(flea::ui_logic_settings_pages::pages::at(&shell.launch.settings.data, &["display", "hyprlandIcons"]).and_then(Json::as_bool), Some(true));
            assert_eq!(shell.settings.failed[0].1, patch(r#"{"display":{"hyprlandIcons":true}}"#));
            shell.change_settings(patch(r#"{"hidden":true}"#), window, cx);
            assert_eq!(shell.launch.settings.data.get("density").and_then(Json::as_str), Some("comfortable"));
        });
    });
}

#[gpui::test]
fn u11_fix1_settings_view_writes_global_and_pane_choices_keep_folder_memory(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell(backend, cx);
        shell.launch.settings.data = flea::uistate::from_file(
            r#"{"view":"list","rememberViewPerFolder":true,"folderViews":{"/other":"columns"}}"#,
        );
        shell.open_settings(window, cx);
        shell
    });
    cx.simulate_keystrokes("right right");
    entity.update_in(cx, |shell, window, cx| {
        assert_eq!(shell.view, View::Grid);
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("view")
                .and_then(Json::as_str),
            Some("grid")
        );
        let folders = shell.launch.settings.data.get("folderViews").unwrap();
        assert!(folders.get("/files").is_none());
        assert_eq!(
            folders.get("/other").and_then(Json::as_str),
            Some("columns")
        );
        shell.choose_settings_view(View::Columns, window, cx);
        assert_eq!(shell.view, View::Columns);
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("view")
                .and_then(Json::as_str),
            Some("grid")
        );
        assert_eq!(
            flea::ui_logic_settings_pages::pages::at(
                &shell.launch.settings.data,
                &["folderViews", "/files"]
            )
            .and_then(Json::as_str),
            Some("columns")
        );
        shell.apply_settings(
            flea::uistate::from_file(r#"{"view":"grid","rememberViewPerFolder":false}"#),
            window,
            cx,
        );
        shell.choose_settings_view(View::List, window, cx);
        assert_eq!(
            shell
                .launch
                .settings
                .data
                .get("view")
                .and_then(Json::as_str),
            Some("list")
        );
    });
}
