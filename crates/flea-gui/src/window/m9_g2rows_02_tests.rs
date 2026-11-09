//! Mounted Settings search acceptance through the real keyboard, pointer and session store.
use super::configured;
use crate::{backend::Backend, g2::settings_pages::Panel};
use flea::{
    jsondoc::Json,
    ui_logic_settings_pages::{pages::SECTIONS, rows::Kind, search::Run},
};
use gpui::{Modifiers, TestAppContext};

#[path = "m9_g2rows_02_fixtures.rs"]
mod fixtures;
use fixtures::{fixture, german};

fn search_rows(panel: &Panel) -> Vec<(String, Vec<String>)> {
    panel
        .results
        .iter()
        .map(|group| {
            let rows = panel.everything_rows(group.section);
            (
                SECTIONS[group.section].id.to_owned(),
                group
                    .hits
                    .iter()
                    .map(|hit| rows[hit.row].label.resolve(&panel.catalog))
                    .collect(),
            )
        })
        .collect()
}

fn next_update(
    updates: &async_channel::Receiver<crate::window::settings::store::Update>,
) -> crate::window::settings::store::Update {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Ok(update) = updates.try_recv() {
            if !matches!(update, crate::window::settings::store::Update::Corner(_)) {
                return update;
            }
        }
        assert!(
            std::time::Instant::now() < until,
            "the mounted settings worker never answered"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[gpui::test]
fn m9_g2rows_02_settings_heading_hint_and_row_search(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(", / t h u m b");
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.title(), "Settings");
        assert_eq!(
            search_rows(panel),
            [
                (
                    "preview".into(),
                    vec![
                        "Thumbnails",
                        "Thumbnail size",
                        "On network shares",
                        "On phones",
                        "On USB drives",
                        "Off still shows thumbnails that were already made.",
                        "Zoom the grid with ctrl and scroll"
                    ]
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
                ),
                ("menus".into(), vec!["Show Thumbnails".into()]),
            ]
        );
        let group = &panel.results[0];
        assert_eq!(
            group.heading_run,
            Some(Run {
                start: 0,
                length: 5
            })
        );
        assert_eq!(
            group.hits[2].run, None,
            "heading inclusion does not wash unrelated labels"
        );
        assert_eq!(panel.hit_target(), Some((group.section, group.hits[0].row)));
    });
    cx.simulate_keystrokes("escape / a l r e a d y");
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(
            search_rows(panel),
            [(
                "preview".into(),
                vec!["Off still shows thumbnails that were already made.".into()]
            )]
        );
    });
    cx.simulate_keystrokes("escape / z z z z z enter");
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert!(panel.results.is_empty());
        assert_eq!(panel.query.as_deref(), Some("zzzzz"));
        assert_eq!(panel.section, "view");
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 0);
    });
}

#[gpui::test]
fn m9_g2rows_02_enter_targets_real_setting_and_escape_returns(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(", / a l r e a d y enter");
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.query, None);
        assert_eq!(panel.section, "preview");
        assert_eq!(
            panel.rows[panel.cursor].label.resolve(&panel.catalog),
            "Off still shows thumbnails that were already made.",
            "Enter highlights the exact matched row, including a read-only hint"
        );
    });
    cx.simulate_keystrokes("/ t h u m b escape");
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert!(shell.settings.opened);
        assert_eq!(panel.query, None);
        assert_eq!(panel.rows[panel.cursor].kind, Kind::Hint);
    });
    cx.simulate_keystrokes("escape");
    shell.update_in(cx, |shell, window, _| {
        assert!(!shell.settings.opened);
        assert!(shell.focus.is_focused(window));
    });
}

#[gpui::test]
fn m9_g2rows_02_filtered_setting_persists_and_relocalizes(cx: &mut TestAppContext) {
    let root = fixture();
    let stored = flea::uistore::Store::at(&root.join("state"), &root.join("config"));
    stored.update(&Json::Obj(Vec::new())).unwrap();
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    let updates = shell.update_in(cx, |shell, window, cx| {
        let updates = shell.m9_g2rows_02_writer(&root);
        shell.receive_settings(next_update(&updates), window, cx);
        updates
    });
    cx.simulate_keystrokes(", / t h u m b");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let at = shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .read(cx)
            .probe
            .bounds("row:preview.thumbUsb")
            .unwrap()
            .center()
    });
    cx.simulate_click(at, Modifiers::none());
    let saved = next_update(&updates);
    assert!(matches!(
        saved,
        crate::window::settings::store::Update::Loaded {
            saved: true,
            through: 1,
            ..
        }
    ));
    shell.update_in(cx, |shell, window, cx| {
        shell.receive_settings(saved, window, cx)
    });
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(
            panel.query.as_deref(),
            Some("thumb"),
            "a filtered control keeps its query"
        );
        assert_eq!(
            panel
                .document()
                .get("preview")
                .and_then(|p| p.get("thumbUsb"))
                .and_then(Json::as_bool),
            Some(false)
        );
        assert_eq!(
            shell.launch.settings.data,
            *panel.document(),
            "the host and search use the same stored document"
        );
        assert!(
            !panel
                .everything_rows(panel.results[0].section)
                .iter()
                .find(|row| row.id == "preview.thumbUsb")
                .unwrap()
                .on
        );
        assert_eq!(
            stored.read().get("preview"),
            shell.launch.settings.data.get("preview"),
            "the mounted worker persisted the complete preview preferences"
        );
    });
    cx.simulate_keystrokes("escape escape ,");
    shell.update_in(cx, |shell, _window, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                view.panel.show_section("preview");
                assert!(
                    !view
                        .panel
                        .rows
                        .iter()
                        .find(|row| row.id == "preview.thumbUsb")
                        .unwrap()
                        .on
                );
                view.panel.key("/", "/", false);
                for ch in "thumb".chars() {
                    let text = ch.to_string();
                    view.panel.key(&text, &text, false);
                }
                cx.notify();
            });
        shell.locale.catalog = german(&root);
        shell.settings_translated(cx);
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .update(cx, |view, cx| {
                assert_eq!(view.panel.catalog.language(), "de");
                assert_eq!(view.panel.query.as_deref(), Some("thumb"));
                assert!(
                    view.panel.results.is_empty(),
                    "old English hits disappear with the catalog"
                );
                for _ in 0..5 {
                    view.panel.key("backspace", "", false);
                }
                for ch in "vor".chars() {
                    let text = ch.to_string();
                    view.panel.key(&text, &text, false);
                }
                let group = view
                    .panel
                    .results
                    .iter()
                    .find(|group| {
                        SECTIONS[group.section].id == "preview"
                            && group.heading.is_some_and(|at| {
                                view.panel.everything_rows(group.section)[at]
                                    .label
                                    .resolve(&view.panel.catalog)
                                    == "Vorschaubilder"
                            })
                    })
                    .unwrap();
                let rows = view.panel.everything_rows(group.section);
                assert_eq!(
                    group.heading_run,
                    Some(Run {
                        start: 0,
                        length: 3
                    })
                );
                let usb = rows
                    .iter()
                    .find(|row| row.id == "preview.thumbUsb")
                    .unwrap();
                assert_eq!(usb.label.resolve(&view.panel.catalog), "Auf USB-Laufwerken");
                assert_eq!(usb.kind, Kind::Check);
                assert!(!usb.on);
                cx.notify();
            });
        let face = crate::presentation::Fonts::from_sources(
            "[font]\nbase-size=20",
            "",
            "monospace".into(),
        )
        .typeface(&shell.launch.settings.data);
        cx.set_global(crate::palette::Theme::from_text(
            "background='#111111'\nforeground='#cccccc'\naccent='#999999'",
        ));
        shell.restyle_settings(&face, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.read_with(cx, |shell, cx| {
        let view = shell.settings.panel.as_ref().unwrap().read(cx);
        assert_eq!(view.typeface.body, gpui::px(20.));
        assert_eq!(view.panel.query.as_deref(), Some("vor"));
        assert!(!view.probe.rect("row:preview.thumbUsb").is_empty());
    });
}

#[gpui::test]
fn m9_g2rows_02_clear_and_rail_restore_the_section(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = configured(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.simulate_keystrokes(", / t h u m b");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let clear = shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .read(cx)
            .probe
            .bounds("search-clear")
            .expect("the clear mark is an actual control")
            .center()
    });
    cx.simulate_click(clear, Modifiers::none());
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.query.as_deref(), Some(""));
        assert!(panel.results.is_empty());
        assert_eq!(panel.title(), "View");
    });
    cx.simulate_keystrokes("t h u m b");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rail = shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .panel
            .as_ref()
            .unwrap()
            .read(cx)
            .probe
            .bounds("rail:preview")
            .unwrap()
            .center()
    });
    cx.simulate_click(rail, Modifiers::none());
    shell.read_with(cx, |shell, cx| {
        let panel = &shell.settings.panel.as_ref().unwrap().read(cx).panel;
        assert_eq!(panel.query, None);
        assert_eq!(panel.section, "preview");
        assert_eq!(panel.title(), "Preview");
    });
}
