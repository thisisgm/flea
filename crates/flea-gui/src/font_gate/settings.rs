use super::*;
use crate::g2::plugin_settings;
use flea::ui_logic_plugin_settings::{
    builtin, schema, store,
    wire::{self, Listing, Plugin, Source},
};

fn listing() -> Listing {
    let state = flea::uischema::defaults();
    let mut plugins: Vec<_> = builtin::manifests()
        .into_iter()
        .map(|(manifest, on)| Plugin {
            enabled: store::enabled(&state, &manifest.id, on),
            settings: schema::settings(&manifest.schema, None),
            manifest,
            source: Source::Builtin,
            failure: None,
            mark_path: String::new(),
            shelf: None,
        })
        .collect();
    let manifest = flea::ui_logic_plugin_settings::manifest::parse(r#"{"schemaVersion":1,"id":"font.gate","name":"Font gate plugin","version":"0.1.0","author":"Test","license":"MIT","description":"Font gate settings","kinds":["menu-action"],"entryPoints":{"run":"bin/test"},"menuActions":[{"id":"run","label":"Run","glyph":"file","when":{"minItems":1}}],"access":["selection"],"schema":[{"key":"choice","type":"choice","label":"Font gate choice","options":[{"value":"one","label":"One"},{"value":"two","label":"Two"}],"defaultValue":"one"}]}"#, false).unwrap();
    plugins.push(Plugin {
        settings: schema::settings(&manifest.schema, None),
        manifest,
        enabled: true,
        source: Source::Git("https://example.invalid/font-gate".into()),
        failure: None,
        mark_path: String::new(),
        shelf: None,
    });
    Listing { id: 1, plugins }
}

fn plugins_page(
    shell: &mut Shell,
    window: &mut Window,
    cx: &mut Context<Shell>,
    line: &str,
    then: impl FnOnce(
        &mut plugin_settings::view::PluginsPane,
        &mut Context<plugin_settings::view::PluginsPane>,
    ),
) {
    shell.open_settings(window, cx);
    shell
        .settings
        .panel
        .as_ref()
        .unwrap()
        .update(cx, |view, cx| {
            view.panel.show_section("plugins");
            cx.notify();
        });
    shell.sync_settings(window, cx);
    shell
        .settings
        .plugins
        .pane
        .as_ref()
        .unwrap()
        .update(cx, |view, cx| {
            view.receive(line, cx);
            then(view, cx);
        });
}

pub(super) fn check() -> Vec<String> {
    let inventory = listing();
    let line = wire::listing_line(&inventory);
    let mut failed = Vec::new();
    let names: Vec<_> = inventory
        .plugins
        .iter()
        .map(|plugin| plugin.manifest.name.as_str())
        .collect();
    surfaces::case_with(
        "settings-plugins",
        &names,
        &mut failed,
        |shell, window, cx| plugins_page(shell, window, cx, &line, |_, _| {}),
    );
    for id in inventory
        .plugins
        .iter()
        .map(|plugin| Some(plugin.manifest.id.as_str()))
        .chain([None, Some("remove")])
    {
        let name = format!("settings-plugin-{}", id.unwrap_or("add"));
        // Shared chrome cannot show that a built-in page drew; its own description can.
        let extra: Vec<_> = inventory
            .plugins
            .iter()
            .filter(|plugin| Some(plugin.manifest.id.as_str()) == id)
            .filter(|plugin| plugin.source == Source::Builtin)
            .flat_map(|plugin| ["Back to Plugins", plugin.manifest.description.as_str()])
            .collect();
        surfaces::case_with(&name, &extra, &mut failed, |shell, window, cx| {
            plugins_page(shell, window, cx, &line, |view, cx| {
                if let Some(id) = id {
                    let target = if id == "remove" { "font.gate" } else { id };
                    for _ in 0..=view.page().rows().len() {
                        if view
                            .page()
                            .cursor()
                            .is_some_and(|at| view.page().rows()[at].id() == target)
                        {
                            break;
                        }
                        view.key(plugin_settings::page::Key::Down, cx);
                    }
                    view.key(plugin_settings::page::Key::Enter, cx);
                    assert_eq!(view.page().open_plugin().unwrap().manifest.id, target);
                    if id == "remove" {
                        for _ in 0..=view.page().rows().len() {
                            if view.page().cursor().is_some_and(|at| {
                                view.page().rows()[at].id()
                                    == flea::ui_logic_plugin_settings::pages::REMOVE_ID
                            }) {
                                break;
                            }
                            view.key(plugin_settings::page::Key::Down, cx);
                        }
                        view.key(plugin_settings::page::Key::Enter, cx);
                        assert!(matches!(
                            view.page().card(),
                            plugin_settings::page::Card::Remove { .. }
                        ));
                    }
                } else {
                    view.key(plugin_settings::page::Key::Enter, cx);
                    assert!(matches!(
                        view.page().card(),
                        plugin_settings::page::Card::Add(_)
                    ));
                }
            })
        });
    }
    failed
}
