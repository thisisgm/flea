use super::frames;
use crate::g2::{st1, st2};
use flea::ui_logic_st2::{
    builtin, schema, store,
    wire::{self, Listing, Plugin, Source},
};
use gpui::{px, size, TestAppContext};
use std::{rc::Rc, sync::Arc};

struct Wire;
impl st2::seam::PluginsWire for Wire {
    fn send(&self, _: String) -> Result<(), String> {
        Ok(())
    }
}

pub(super) fn check(cx: &mut TestAppContext, corner: f32) {
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let mut panel = st1::Panel::new(
            Box::new(st1::MemoryStore::new(flea::uischema::defaults())),
            st1::Facts {
                corner,
                ..Default::default()
            },
            Arc::default(),
        );
        panel.show_section("display");
        st1::SettingsView::new(panel, cx)
    });
    vcx.simulate_resize(size(px(1100.), px(700.)));
    frames("settings-card-chips-checkboxes", vcx, corner, &[18.]);
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let mut panel = st1::Panel::new(
            Box::new(st1::MemoryStore::new(flea::uischema::defaults())),
            st1::Facts {
                corner,
                ..Default::default()
            },
            Arc::default(),
        );
        panel.query = Some("font".into());
        st1::SettingsView::new(panel, cx)
    });
    frames("settings-search-field", vcx, corner, &[18.]);
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let mut view = st1::SheetView::new(
            st1::Sheet::new("default", Arc::default(), st1::Sources::default()),
            cx,
        );
        view.corner = corner;
        view.sheet.query = "copy".into();
        view
    });
    frames("keyboard-sheet-and-query", vcx, corner, &[]);
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
    let manifest = flea::ui_logic_st2::manifest::parse(r#"{"schemaVersion":1,"id":"test.corners","name":"Corners","version":"0.1.0","author":"Test","license":"MIT","description":"Corner controls","kinds":["menu-action"],"entryPoints":{"run":"bin/test"},"menuActions":[{"id":"run","label":"Run","glyph":"file","when":{"minItems":1}}],"access":["selection"],"schema":[{"key":"choice","type":"choice","label":"Choice","options":[{"value":"one","label":"One"},{"value":"two","label":"Two"}],"defaultValue":"one"}]}"#, false).unwrap();
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
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let theme = crate::palette::Theme::default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        st2::view::PluginsPane::new(
            Arc::default(),
            Rc::new(Wire),
            st2::seam::Tokens::interim(&palette),
            corner,
            cx,
        )
    });
    vcx.simulate_resize(size(px(462.), px(656.)));
    view.update(vcx, |view, cx| view.receive(&listing, cx));
    frames("plugins-checkboxes", vcx, corner, &[18.]);
    view.update(vcx, |view, cx| view.key(st2::page::Key::Enter, cx));
    frames("plugins-add-card-field-buttons", vcx, corner, &[]);
    view.update(vcx, |view, cx| {
        view.key(st2::page::Key::Escape, cx);
        for _ in 0..view.page().rows().len() {
            view.key(st2::page::Key::Down, cx);
        }
        view.key(st2::page::Key::Enter, cx);
        assert_eq!(
            view.page().open_plugin().unwrap().manifest.id,
            "test.corners"
        );
        assert!(view
            .page()
            .rows()
            .iter()
            .any(|row| matches!(row, flea::ui_logic_st2::pages::Row::Choice { .. })));
    });
    frames("plugins-choice-chips", vcx, corner, &[18.]);
    view.update(vcx, |view, cx| {
        for _ in 0..view.page().rows().len() {
            if view.page().cursor().is_some_and(|at| {
                view.page().rows()[at].id() == flea::ui_logic_st2::pages::REMOVE_ID
            }) {
                break;
            }
            view.key(st2::page::Key::Down, cx);
        }
        assert_eq!(
            view.page().rows()[view.page().cursor().unwrap()].id(),
            flea::ui_logic_st2::pages::REMOVE_ID
        );
        view.key(st2::page::Key::Enter, cx);
        assert!(matches!(view.page().card(), st2::page::Card::Remove { .. }));
    });
    frames("plugins-remove-card-buttons", vcx, corner, &[]);
}
