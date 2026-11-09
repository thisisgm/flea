use crate::{
    backend::{Backend, Event},
    g2::{st1::Side, st2::page::Card},
    state::Shell,
};
use gpui::{AppContext, TestAppContext};

fn shell(backend: Backend, cx: &mut gpui::Context<Shell>) -> Shell {
    let mut shell = crate::state::tests::shell_with(backend, cx);
    shell.launch.settings.data = flea::uischema::defaults();
    shell
}

#[gpui::test]
fn s6_unreadable_settings_names_favourites_and_escape_clears_it(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let shell = shell(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| {
        shell.receive_settings(super::store::Update::Notice("settings.error.favourites_unread"), window, cx);
        assert_eq!(
            shell.settings_message(cx).as_deref(),
            Some("Favorites could not be refreshed because ui.json could not be read. The old list stays.")
        );
    });
    cx.simulate_keystrokes("escape");
    entity.read_with(cx, |shell, cx| {
        assert!(shell.settings_message(cx).is_none())
    });
}

#[gpui::test]
fn u11_leaving_dual_remembers_the_destination_folders_view(cx: &mut TestAppContext) {
    let (a, _requests_a) = Backend::test_channel();
    let (b, _requests_b) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell(a, cx));
        entity.update(cx, |shell, cx| {
            let mut other =
                crate::window::pane::PaneHost::new("/other".into(), None, 620., &shell.locale);
            other.backend = Some(b);
            shell.panes[1] = Some(other);
            shell.view = crate::window::startup::View::Dual;
            shell.focus_side = 1;
            shell.launch.settings.data =
                flea::uistate::from_file(r#"{"rememberViewPerFolder":true}"#);
            shell.choose_settings_view(crate::window::startup::View::Grid, window, cx);
            let views = shell.launch.settings.data.get("folderViews").unwrap();
            assert_eq!(
                views.get("/files").and_then(flea::jsondoc::Json::as_str),
                Some("grid")
            );
            assert!(views.get("/other").is_none());
            assert_eq!(shell.focus_side, 0);
        });
    });
}

#[gpui::test]
fn u11_lazy_settings_and_sheet_use_and_follow_the_effective_typeface(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell(backend, cx));
        entity.update(cx, |shell, cx| {
            let document = flea::uistate::from_file(r#"{"display":{"textSize":{"mode":20}}}"#);
            shell.apply_settings(document, window, cx);
            shell.open_settings(window, cx);
            assert_eq!(
                f32::from(
                    shell
                        .settings
                        .panel
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .typeface
                        .body
                ),
                20.
            );
            shell.open_sheet(window, cx);
            assert_eq!(
                f32::from(
                    shell
                        .settings
                        .sheet
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .typeface
                        .body
                ),
                20.
            );
            shell.apply_settings(
                flea::uistate::from_file(r#"{"keys":"windows"}"#),
                window,
                cx,
            );
            assert_eq!(
                f32::from(
                    shell
                        .settings
                        .sheet
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .typeface
                        .body
                ),
                14.
            );
            let sheet = &shell.settings.sheet.as_ref().unwrap().read(cx).sheet;
            assert_eq!(sheet.preset, "windows");
            assert!(sheet
                .rows
                .iter()
                .any(|row| row.action == "toggleHidden" && row.keys.contains("ctrl-h")));
        });
    });
}

#[gpui::test]
fn u11_moving_a_favourite_keeps_the_keyboard_on_that_place(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell(backend, cx);
        shell.launch.settings.data = flea::uistate::from_file(r#"{"places":{"favourites":[{"path":"/files","label":"Files"},{"path":"/other","label":"Other"}]}}"#);
        shell.open_settings(window, cx);
        shell
    });
    entity.update(cx, |shell, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                view.panel.show_section("places");
                view.panel.cursor = view
                    .panel
                    .rows
                    .iter()
                    .position(|row| row.favourite == Some(0))
                    .unwrap();
                cx.notify();
            });
    });
    cx.simulate_keystrokes("shift-j");
    entity.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.rows[panel.cursor].favourite, Some(1));
        assert_eq!(
            panel.rows[panel.cursor].label.resolve(&panel.catalog),
            "Files"
        );
    });
    cx.simulate_keystrokes("x");
    entity.read_with(cx, |shell, _| {
        let paths =
            flea::ui_logic_st1::pages::at(&shell.launch.settings.data, &["places", "favourites"])
                .unwrap()
                .as_array()
                .unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].get("path").and_then(flea::jsondoc::Json::as_str),
            Some("/other")
        );
    });
}

#[gpui::test]
fn u11_plugins_focus_card_keys_and_backend_eof_follow_the_mounted_producer(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    // Settings > Plugins talks to the app-wide plugin owner, so its end is what resets a busy card.
    let (owner, _owner_lines) = Backend::test_lines();
    let owner_output = owner.test_output();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        crate::g2::plugins::service(cx).update(cx, |host, cx| host.test_attach(owner, cx));
        let mut shell = shell(backend, cx);
        shell.open_settings(window, cx);
        shell
    });
    entity.update_in(cx, |shell, window, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                view.panel.show_section("plugins");
                cx.notify();
            });
        shell.sync_settings_plugins(window, cx);
        shell.settings_plugins_reply(r#"{"t":"plugins","id":1,"plugins":[]}"#, cx);
        let plugin = shell.settings.plugins.pane.as_ref().unwrap();
        assert!(plugin.read(cx).focus_handle().is_focused(window));
    });
    cx.simulate_keystrokes("tab");
    entity.update_in(cx, |shell, window, cx| {
        let panel = shell.settings.panel.as_ref().unwrap().read(cx);
        assert_eq!(panel.panel.side, Side::Rail);
        assert!(panel.focus.is_focused(window));
    });
    cx.simulate_keystrokes("tab enter");
    entity.read_with(cx, |shell, cx| {
        assert!(matches!(
            shell
                .settings
                .plugins
                .pane
                .as_ref()
                .unwrap()
                .read(cx)
                .page()
                .card(),
            Card::Add(_)
        ));
    });
    cx.simulate_keystrokes("tab down");
    entity.read_with(cx, |shell, cx| {
        let panel = shell.settings.panel.as_ref().unwrap().read(cx);
        assert_eq!(panel.panel.section, "plugins");
        assert_eq!(panel.panel.side, Side::Pane);
    });
    cx.simulate_keystrokes("a b c enter");
    entity.update_in(cx, |shell, window, cx| {
        let plugin = shell.settings.plugins.pane.as_ref().unwrap().clone();
        assert!(matches!(plugin.read(cx).page().card(), Card::Add(card) if card.busy));
        // A pane backend's end is not the plugin owner's, so the card stays busy.
        shell.receive(0, Event::Drained, window, cx);
        assert!(matches!(plugin.read(cx).page().card(), Card::Add(card) if card.busy));
    });
    owner_output.try_send(Event::Drained).unwrap();
    cx.run_until_parked();
    entity.read_with(cx, |shell, cx| {
        let plugin = shell.settings.plugins.pane.as_ref().unwrap();
        assert!(matches!(plugin.read(cx).page().card(), Card::Add(card) if !card.busy));
    });
}

#[gpui::test]
fn u11_advreview_make_default_preserves_the_reader_contract(cx: &mut TestAppContext) {
    use crate::ipc::Adapter;
    use flea::jsondoc::{self, Json};
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.open_settings(window, cx);
            shell.settings.about_open = true;
            shell.settings.claim = crate::g2::st1::claim::Claim {
                handler: crate::g2::st1::claim::DESKTOP.into(),
                packaged: true,
                probed: true,
                ..Default::default()
            };
            shell.sync_settings_claim(cx);
            shell
                .settings
                .panel
                .as_ref()
                .unwrap()
                .update(cx, |view, _| {
                    view.panel.show_section("about");
                });
            let adapter = crate::ipc::settings::SettingsAdapter {
                host: &shell.settings,
                document: &shell.launch.settings.data,
                hidden: false,
                cx,
            };
            let reply = adapter.call("settingsModel", &[]).unwrap();
            let document = jsondoc::parse(&reply.stdout).unwrap();
            let rows = document.as_array().unwrap();
            let at = rows
                .iter()
                .position(|r| r.get("id").and_then(Json::as_str) == Some("makeDefault"))
                .unwrap();
            let caption = rows[at].get("caption");
            let elide = rows[at + 1].get("elide").and_then(Json::as_str);
            eprintln!("makeDefault caption={caption:?}; note elide={elide:?}");
            assert!(
                caption.is_none_or(|value| *value == Json::Null) && elide == Some("right"),
                "tests/ui-makedefault.sh requires caption == null and note.elide == right"
            );
        });
    });
}

#[gpui::test]
fn u11_advreview_queued_success_keeps_a_failed_session_patch(cx: &mut TestAppContext) {
    use flea::jsondoc::{self, Json};
    let (backend, _requests) = Backend::test_channel();
    let (writes, queued) = std::sync::mpsc::channel();
    let (_updates, updates) = async_channel::unbounded();
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let entity = cx.new(|cx| shell(backend, cx));
        entity.update(cx, |shell, cx| {
            shell.settings.worker = Some(super::store::Worker { writes, updates });
            shell.change_settings(
                jsondoc::parse(r#"{"density":"normal"}"#).unwrap(),
                window,
                cx,
            );
            shell.change_settings(jsondoc::parse(r#"{"hidden":true}"#).unwrap(), window, cx);
            let first = queued.try_recv().unwrap();
            let second = queued.try_recv().unwrap();
            eprintln!(
                "queued id1={} patch1={}; id2={} patch2={}",
                first.0,
                jsondoc::render(&first.1).trim(),
                second.0,
                jsondoc::render(&second.1).trim()
            );
            shell.receive_settings(
                super::store::Update::Failed {
                    through: 1,
                    keep: true,
                },
                window,
                cx,
            );
            shell.receive_settings(
                super::store::Update::Loaded {
                    document: flea::uistate::from_file(r#"{"hidden":true}"#),
                    through: 2,
                    saved: true,
                },
                window,
                cx,
            );
            eprintln!(
                "after replies density={:?}, failed patches={}",
                shell.launch.settings.data.get("density"),
                shell.settings.failed.len()
            );
            assert_eq!(
                shell
                    .launch
                    .settings
                    .data
                    .get("density")
                    .and_then(Json::as_str),
                Some("normal"),
                "an unrelated queued save must not discard a failed session preference"
            );
        });
    });
}

#[gpui::test]
fn u11_advreview_settings_view_can_step_with_folder_memory(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, cx) = cx.add_window_view(|window, cx| {
        let mut shell = shell(backend, cx);
        shell.launch.settings.data =
            flea::uistate::from_file(r#"{"view":"list","rememberViewPerFolder":true}"#);
        shell.open_settings(window, cx);
        shell
    });
    cx.simulate_keystrokes("l l");
    entity.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        let choice = panel.rows.iter().find(|row| row.id == "view").unwrap();
        eprintln!(
            "after l l window={:?}, selected={}, folderViews={:?}",
            shell.view,
            choice.selected,
            shell.launch.settings.data.get("folderViews")
        );
        assert_eq!(
            shell.view,
            crate::window::startup::View::Grid,
            "two right steps from List must reach Grid with folder memory enabled"
        );
        assert_eq!(choice.selected, "grid");
    });
}
