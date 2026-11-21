use super::{frames, mounted};
use crate::g2::plugin_settings;
use flea::ui_logic_plugin_settings::{
    builtin, schema, store,
    wire::{self, Listing, Plugin, Source},
};
use gpui::{px, size, AppContext, TestAppContext};
use std::{rc::Rc, sync::Arc};

struct Wire;
impl plugin_settings::seam::PluginsWire for Wire {
    fn send(&self, _: String) -> Result<(), String> {
        Ok(())
    }
}

/// Frames every settings section, the search field, the sheet and the plugins pane
pub(super) fn check(cx: &mut TestAppContext, corner: f32) {
    let (shell, _, vcx) = mounted(cx, corner, |shell, window, cx| {
        shell.open_settings(window, cx);
        let view = shell.settings.panel.clone().unwrap();
        view.update(cx, |view, _| view.panel.show_section("display"));
        view
    });
    vcx.simulate_resize(size(px(1100.), px(700.)));
    frames(
        "settings-card-chips-checkboxes",
        &shell,
        vcx,
        corner,
        &[18.],
    );
    for section in flea::ui_logic_settings_pages::pages::SECTIONS {
        let (shell, view, vcx) = mounted(cx, corner, |shell, window, cx| {
            shell.open_settings(window, cx);
            shell.settings.panel.clone().unwrap()
        });
        shell.update(vcx, |shell, cx| {
            // Set the section and its fixed About state together, before observers can start desktop probes.
            shell.settings.about_open = section.id == "about";
            view.update(cx, |view, cx| {
                if section.id == "about" {
                    view.panel.make_default = crate::g2::settings_pages::claim::Claim {
                        probed: true,
                        ..Default::default()
                    }
                    .rows();
                }
                view.panel.show_section(section.id);
                cx.notify();
            });
        });
        if section.id == "about" {
            // The census pins the uninstalled state: the default switch is inert there, and its
            // frames draw disabled. The host's own package and handler must not decide a census.
            shell.update(vcx, |shell, _| {
                shell.settings.claim.handler.clear();
                shell.settings.claim.packaged = false;
                shell.settings.claim.probed = true;
                shell.settings.claim.revision += 1;
            });
            let rows = shell.read_with(vcx, |shell, _| shell.settings.claim.rows());
            view.update(vcx, |view, cx| {
                view.panel.make_default = rows;
                view.panel.rebuild();
                cx.notify();
            });
        }
        frames(
            &format!("settings-{}", section.id),
            &shell,
            vcx,
            corner,
            &[18.],
        );
    }
    let (shell, _, vcx) = mounted(cx, corner, |shell, window, cx| {
        shell.open_settings(window, cx);
        let view = shell.settings.panel.clone().unwrap();
        view.update(cx, |view, _| view.panel.query = Some("font".into()));
        view
    });
    frames("settings-search-field", &shell, vcx, corner, &[18.]);
    let (shell, _, vcx) = mounted(cx, corner, |shell, window, cx| {
        shell.open_sheet(window, cx);
        let view = shell.settings.sheet.clone().unwrap();
        view.update(cx, |view, _| view.sheet.query = "copy".into());
        view
    });
    frames("keyboard-sheet-and-query", &shell, vcx, corner, &[]);
    let state = flea::uischema::defaults();
    let mut plugins: Vec<Plugin> = builtin::manifests()
        .into_iter()
        .map(|(m, on)| Plugin {
            enabled: store::enabled(&state, &m.id, on),
            source: Source::Builtin,
            failure: None,
            mark_path: String::new(),
            settings: schema::settings(&m.schema, None),
            shelf: None,
            manifest: m,
        })
        .collect();
    let manifest = flea::ui_logic_plugin_settings::manifest::parse(r#"{"schemaVersion":1,"id":"test.corners","name":"Corners","version":"0.1.0","author":"Test","license":"MIT","description":"Corner controls","kinds":["menu-action"],"entryPoints":{"run":"bin/test"},"menuActions":[{"id":"run","label":"Run","glyph":"file","when":{"minItems":1}}],"access":["selection"],"schema":[{"key":"choice","type":"choice","label":"Choice","options":[{"value":"one","label":"One"},{"value":"two","label":"Two"}],"defaultValue":"one"}]}"#, false).unwrap();
    plugins.push(Plugin {
        settings: schema::settings(&manifest.schema, None),
        manifest,
        source: Source::Git("https://example.invalid/corners".into()),
        enabled: true,
        failure: None,
        mark_path: String::new(),
        shelf: None,
    });
    let listing = wire::listing_line(&Listing { id: 1, plugins });
    let (shell, panel, vcx) = mounted(cx, corner, |shell, window, cx| {
        let theme = super::theme();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let view = cx.new(|cx| {
            plugin_settings::view::PluginsPane::new(
                Arc::default(),
                Rc::new(Wire),
                plugin_settings::seam::Tokens::interim(&palette),
                shell.corner,
                cx,
            )
        });
        // Plugin dialogs now mount through SettingsView, above the plugin rows.
        shell.open_settings(window, cx);
        let panel = shell.settings.panel.clone().unwrap();
        panel.update(cx, |panel, cx| {
            cx.observe(&view, |_, _, cx| cx.notify()).detach();
            panel.panel.show_section("plugins");
            panel.plugins = Some(view.clone());
            cx.notify();
        });
        shell.settings.plugins.pane = Some(view);
        panel
    });
    let view = panel.read_with(vcx, |panel, _| panel.plugins.clone().unwrap());
    // 0401-settings-wide-b: keep the 630px floor and both 8px margins visible.
    vcx.simulate_resize(size(px(646.), px(656.)));
    view.update(vcx, |view, cx| view.receive(&listing, cx));
    frames("plugins-checkboxes", &shell, vcx, corner, &[18.]);
    view.update(vcx, |view, cx| {
        view.key(plugin_settings::page::Key::Enter, cx);
        assert!(matches!(
            view.page().card(),
            plugin_settings::page::Card::Add(_)
        ));
    });
    frames("plugins-add-card-field-buttons", &shell, vcx, corner, &[]);
    view.update(vcx, |view, cx| {
        view.key(plugin_settings::page::Key::Escape, cx);
        for _ in 0..view.page().rows().len() {
            view.key(plugin_settings::page::Key::Down, cx);
        }
        view.key(plugin_settings::page::Key::Enter, cx);
        assert_eq!(
            view.page().open_plugin().unwrap().manifest.id,
            "test.corners"
        );
        assert!(view.page().rows().iter().any(|row| matches!(
            row,
            flea::ui_logic_plugin_settings::pages::Row::Choice { .. }
        )));
    });
    frames("plugins-choice-chips", &shell, vcx, corner, &[18.]);
    view.update(vcx, |view, cx| {
        for _ in 0..view.page().rows().len() {
            if view.page().cursor().is_some_and(|at| {
                view.page().rows()[at].id() == flea::ui_logic_plugin_settings::pages::REMOVE_ID
            }) {
                break;
            }
            view.key(plugin_settings::page::Key::Down, cx);
        }
        assert_eq!(
            view.page().rows()[view.page().cursor().unwrap()].id(),
            flea::ui_logic_plugin_settings::pages::REMOVE_ID
        );
        view.key(plugin_settings::page::Key::Enter, cx);
        assert!(matches!(
            view.page().card(),
            plugin_settings::page::Card::Remove { .. }
        ));
    });
    frames("plugins-remove-card-buttons", &shell, vcx, corner, &[]);
}
