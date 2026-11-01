//! The plugin page title and its Add card use the window's Settings composition.
use super::*;
use crate::g2::st2::{
    seam::{PluginsWire, Tokens},
    view::PluginsPane,
};
use flea::ui_logic_st2::{
    builtin,
    wire::{listing_line, Listing, Plugin, Source},
};
use gpui::{point, px, size, AppContext, Entity, TextRun};

struct Wire;
impl PluginsWire for Wire {
    fn send(&self, _: String) -> Result<(), String> {
        Ok(())
    }
}

fn open(
    cx: &mut TestAppContext,
) -> (
    Entity<SettingsView>,
    Entity<PluginsPane>,
    &mut gpui::VisualTestContext,
) {
    let (settings, cx) = cx.add_window_view(|_, cx| {
        let mut view = SettingsView::new(panel("{}"), cx);
        view.panel.show_section("plugins");
        view
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let plugins = cx.update(|_, cx| {
        cx.new(|cx| {
            PluginsPane::new(
                english(),
                Rc::new(Wire),
                Tokens::interim(&crate::list::Palette::inherited(
                    &gpui::TextStyle::default(),
                    crate::palette::Theme::default(),
                )),
                8.,
                cx,
            )
        })
    });
    let rows = builtin::manifests()
        .into_iter()
        .map(|(manifest, enabled)| Plugin {
            manifest,
            enabled,
            source: Source::Builtin,
            failure: None,
            mark_path: String::new(),
            settings: vec![],
            shelf: None,
        })
        .collect();
    plugins.update(cx, |view, cx| {
        view.receive(
            &listing_line(&Listing {
                id: 1,
                plugins: rows,
            }),
            cx,
        )
    });
    settings.update(cx, |view, cx| {
        cx.observe(&plugins, |_, _, cx| cx.notify()).detach();
        view.plugins = Some(plugins.clone());
        cx.notify();
    });
    cx.update(|window, cx| plugins.read(cx).focus_handle().clone().focus(window, cx));
    cx.run_until_parked();
    (settings, plugins, cx)
}

#[gpui::test]
fn vf_look_shelf_page_paints_its_plugin_title(cx: &mut TestAppContext) {
    let (settings, plugins, cx) = open(cx);
    cx.simulate_keystrokes("down enter");
    settings.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.update(|window, cx| {
        assert_eq!(plugins.read(cx).page().title(), "Shelf");
        let settings = settings.read(cx);
        let title = settings.probe.bounds("title").unwrap();
        let mut font = settings.typeface.font.clone();
        font.weight = gpui::FontWeight::BOLD;
        let expected = window
            .text_system()
            .shape_line(
                "Shelf".into(),
                settings.typeface.caption,
                &[TextRun {
                    len: 5,
                    font,
                    color: gpui::Hsla::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width;
        assert!(
            (f32::from(title.size.width - expected)).abs() < 1.,
            "the painted header must say Shelf, not Plugins: {title:?}"
        );
    });
}

#[gpui::test]
fn vf_look_add_plugin_card_is_centered_on_the_window(cx: &mut TestAppContext) {
    let (_, plugins, cx) = open(cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(matches!(
        plugins.read_with(cx, |view, _| view.page().card().clone()),
        crate::g2::st2::page::Card::Add(_)
    ));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let mut cards: Vec<_> = window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                let b = quad.bounds;
                b.size.width.0 / scale > 400.
                    && b.size.width.0 / scale < 650.
                    && b.size.height.0 / scale > 100.
                    && b.size.height.0 / scale < 400.
                    && quad.border_widths.top.0 > 0.
                    && quad.border_widths.bottom.0 > 0.
                    && quad.border_widths.left.0 > 0.
                    && quad.border_widths.right.0 > 0.
            })
            .collect();
        // GPUI may paint one bordered card as several masked pieces of the same quad.
        cards.dedup_by(|a, b| a.bounds == b.bounds);
        assert_eq!(cards.len(), 1, "one framed plugin card: {cards:?}");
        let bounds = cards[0].bounds;
        assert_eq!(
            bounds.size.width.0 / scale,
            498.,
            "the HIG card keeps its width"
        );
        assert_eq!(
            bounds.center(),
            point(
                gpui::ScaledPixels(550. * scale),
                gpui::ScaledPixels(350. * scale)
            ),
            "the card centers on the window"
        );
    });
}
