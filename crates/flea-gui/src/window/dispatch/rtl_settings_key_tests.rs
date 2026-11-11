use crate::g2::{
    open_with_properties, plugin_settings,
    settings_pages::{Facts, MemoryStore, Panel, SettingsView, Side},
};
use flea::jsondoc::{parse, render};
use gpui::{AppContext as _, TestAppContext};
use std::{cell::RefCell, rc::Rc};

fn panel() -> Panel {
    Panel::new(
        Box::new(MemoryStore::new(parse("{}").unwrap())),
        Facts::default(),
        open_with_properties::tests::english().into(),
    )
}

#[gpui::test]
fn i50_rtl_the_settings_view_routes_arrows_and_h_l_through_the_production_handler(
    cx: &mut TestAppContext,
) {
    let mut expected = panel();
    expected.key("right", "", false);
    let right = render(expected.document());
    let mut opposite = panel();
    opposite.key("left", "", false);
    assert_ne!(
        right,
        render(opposite.document()),
        "fixture must distinguish the two directions"
    );
    let (view, cx) = cx.add_window_view(|_, cx| SettingsView::new(panel(), cx));
    cx.update(|window, app| {
        window.set_layout_mirrored(true);
        let focus = view.read(app).focus.clone();
        window.focus(&focus, app);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("left");
    assert_eq!(
        view.read_with(cx, |view, _| render(view.panel.document())),
        right
    );
    for (physical, logical, text) in [("right", "left", ""), ("h", "l", "l"), ("l", "h", "h")] {
        expected.key(logical, text, false);
        cx.simulate_keystrokes(physical);
        assert_eq!(
            view.read_with(cx, |view, _| render(view.panel.document())),
            render(expected.document())
        );
    }
}

#[derive(Default)]
struct Wire(RefCell<Vec<String>>);
impl plugin_settings::seam::PluginsWire for Wire {
    fn send(&self, line: String) -> Result<(), String> {
        self.0.borrow_mut().push(line);
        Ok(())
    }
}

fn plugin() -> flea::ui_logic_plugin_settings::wire::Plugin {
    use flea::ui_logic_plugin_settings::{
        manifest, schema,
        wire::{Plugin, Source},
    };
    let manifest = manifest::parse(
        r#"{
        "schemaVersion":1,"id":"gm.rtl-keys","name":"RTL keys","version":"1.0.0",
        "author":"GM","license":"MIT","description":"Keyboard fixture",
        "kinds":["menu-action"],"entryPoints":{"run":"bin/rtl-keys"},
        "menuActions":[{"id":"run","label":"Run","glyph":"terminal","when":{"minItems":1}}],
        "access":["selection"],
        "schema":[{"key":"choice","type":"choice","label":"Choice","options":[
            {"value":"a","label":"A"},{"value":"b","label":"B"},{"value":"c","label":"C"}
        ],"defaultValue":"b"}]
    }"#,
        false,
    )
    .unwrap();
    let settings = schema::settings(&manifest.schema, None);
    Plugin {
        manifest,
        source: Source::Builtin,
        enabled: true,
        failure: None,
        mark_path: String::new(),
        settings,
        shelf: None,
    }
}

#[gpui::test]
fn i50_rtl_the_settings_plugin_adapter_changes_a_real_choice_in_reading_order(
    cx: &mut TestAppContext,
) {
    use flea::ui_logic_plugin_settings::{
        pages::Row,
        wire::{listing_line, Listing},
    };
    let wire = Rc::new(Wire::default());
    let sink = wire.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let catalog = std::sync::Arc::new(open_with_properties::tests::english());
        let palette =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        let pane = cx.new(|cx| {
            plugin_settings::view::PluginsPane::new(
                catalog,
                sink,
                plugin_settings::seam::Tokens::interim(&palette),
                0.,
                cx,
            )
        });
        let mut view = SettingsView::new(panel(), cx);
        view.panel.show_section("plugins");
        view.panel.side = Side::Pane;
        view.plugins = Some(pane);
        view
    });
    let pane = view.read_with(cx, |view, _| view.plugins.clone().unwrap());
    pane.update(cx, |pane, cx| {
        pane.receive(
            &listing_line(&Listing {
                id: 1,
                plugins: vec![plugin()],
            }),
            cx,
        );
        let at = pane
            .page()
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Plugin { id, .. } if id == "gm.rtl-keys"))
            .unwrap();
        for _ in 0..pane.page().rows().len() {
            if pane.page().cursor() == Some(at) {
                break;
            }
            pane.key(plugin_settings::page::Key::Down, cx);
        }
        assert_eq!(pane.page().cursor(), Some(at));
        pane.key(plugin_settings::page::Key::Enter, cx);
        let at = pane
            .page()
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Choice { id, .. } if id == "gm.rtl-keys:choice"))
            .unwrap();
        for _ in 0..pane.page().rows().len() {
            if pane.page().cursor() == Some(at) {
                break;
            }
            pane.key(plugin_settings::page::Key::Down, cx);
        }
        assert_eq!(pane.page().cursor(), Some(at));
    });
    cx.update(|window, app| {
        window.set_layout_mirrored(true);
        let focus = view.read(app).focus.clone();
        window.focus(&focus, app);
    });
    cx.run_until_parked();
    wire.0.borrow_mut().clear();
    cx.simulate_keystrokes("left");
    let lines = wire.0.borrow();
    assert_eq!(lines.len(), 1);
    let sent = parse(&lines[0]).unwrap();
    assert_eq!(sent.get("op").and_then(|v| v.as_str()), Some("set"));
    assert_eq!(
        sent.get("plugin").and_then(|v| v.as_str()),
        Some("gm.rtl-keys")
    );
    assert_eq!(sent.get("value").and_then(|v| v.as_str()), Some("c"));
}
