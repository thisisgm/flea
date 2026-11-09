//! The plugin page title and its Add card use the window's Settings composition.
use super::*;
use crate::g2::plugin_settings::{
    seam::{PluginsWire, Tokens},
    view::PluginsPane,
};
use flea::ui_logic_plugin_settings::{
    builtin,
    wire::{listing_line, Listing, Plugin, Source},
};
use gpui::{point, px, size, AppContext, Entity, TextRun};
use std::cell::RefCell;

struct Wire;
impl PluginsWire for Wire {
    fn send(&self, _: String) -> Result<(), String> {
        Ok(())
    }
}

/// Every line the plugin pane sends, for a test that asserts what a press asked for.
#[derive(Default)]
struct Recording(RefCell<Vec<String>>);
impl PluginsWire for Recording {
    fn send(&self, line: String) -> Result<(), String> {
        self.0.borrow_mut().push(line);
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
    open_on(cx, Rc::new(Wire), Vec::new())
}

fn open_on(
    cx: &mut TestAppContext,
    wire: Rc<dyn PluginsWire>,
    extra: Vec<Plugin>,
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
                wire,
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
        .chain(extra)
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
        crate::g2::plugin_settings::page::Card::Add(_)
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

/// The Add card open over the plugin list with a source typed, and the lines its opening sent.
fn add_card_typed(
    cx: &mut TestAppContext,
) -> (
    Entity<PluginsPane>,
    &mut gpui::VisualTestContext,
    Rc<Recording>,
) {
    let wire = Rc::new(Recording::default());
    let (_, plugins, cx) = open_on(cx, wire.clone(), Vec::new());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("https://example.invalid/plugin.git");
    cx.run_until_parked();
    assert!(matches!(
        plugins.read_with(cx, |view, _| view.page().card().clone()),
        crate::g2::plugin_settings::page::Card::Add(card) if card.text == "https://example.invalid/plugin.git"
    ));
    wire.0.borrow_mut().clear();
    (plugins, cx, wire)
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
}

/// What Enter sends on an open card, for a click on the same card in another window to match.
fn entered(cx: &mut gpui::VisualTestContext, wire: &Recording) -> Vec<String> {
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let lines = wire.0.borrow().clone();
    lines
}

#[gpui::test]
fn b02_a_click_on_the_add_plugin_cancel_closes_the_card_and_sends_nothing(cx: &mut TestAppContext) {
    let (plugins, cx, wire) = add_card_typed(cx);
    click(cx, "plugins-card-cancel");
    assert_eq!(
        plugins.read_with(cx, |view, _| view.page().card().clone()),
        crate::g2::plugin_settings::page::Card::None,
        "Cancel closes the card, as Escape does"
    );
    assert!(
        wire.0.borrow().is_empty(),
        "nothing is added: {:?}",
        wire.0.borrow()
    );
}

#[gpui::test]
fn b02_a_click_on_add_plugin_sends_what_enter_sends(cx: &mut TestAppContext) {
    let enter = {
        let (_, cx, wire) = add_card_typed(cx);
        entered(cx, &wire)
    };
    let (plugins, cx, wire) = add_card_typed(cx);
    click(cx, "plugins-card-action");
    let clicked = wire.0.borrow().clone();
    assert_eq!(clicked.len(), 1, "one request: {clicked:?}");
    assert!(
        clicked[0].contains(r#""op":"add""#)
            && clicked[0].contains(r#""source":"https://example.invalid/plugin.git""#),
        "the click asks for the typed source: {clicked:?}"
    );
    assert_eq!(clicked, enter, "the click sends exactly what Enter sends");
    let busy = plugins.read_with(
        cx,
        |view, _| matches!(view.page().card(), crate::g2::plugin_settings::page::Card::Add(card) if card.busy),
    );
    assert!(busy, "the card waits on its answer, as after Enter");
}

/// Moves the plugin pane's cursor onto the row with this id, a key at a time.
fn walk_to(cx: &mut gpui::VisualTestContext, plugins: &Entity<PluginsPane>, id: &str) {
    for _ in 0..64 {
        let (cursor, at) = plugins.read_with(cx, |view, _| {
            let page = view.page();
            (
                page.cursor(),
                page.rows().iter().position(|row| row.id() == id),
            )
        });
        let at = at.unwrap_or_else(|| panic!("no row {id}"));
        match cursor {
            Some(cursor) if cursor == at => return,
            Some(cursor) if cursor > at => cx.simulate_keystrokes("up"),
            _ => cx.simulate_keystrokes("down"),
        }
        cx.run_until_parked();
    }
    panic!("the cursor never reached {id}");
}

/// The Remove card open over a plugin installed from a folder, and nothing sent since it opened.
fn remove_card(
    cx: &mut TestAppContext,
) -> (
    Entity<PluginsPane>,
    &mut gpui::VisualTestContext,
    Rc<Recording>,
) {
    let (mut manifest, _) = builtin::manifests()
        .into_iter()
        .next()
        .expect("a built-in manifest to copy");
    manifest.id = "test.local".into();
    let local = Plugin {
        manifest,
        enabled: true,
        source: Source::Folder("/plugins/local".into()),
        failure: None,
        mark_path: String::new(),
        settings: vec![],
        shelf: None,
    };
    let wire = Rc::new(Recording::default());
    let (_, plugins, cx) = open_on(cx, wire.clone(), vec![local]);
    walk_to(cx, &plugins, "test.local");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    walk_to(
        cx,
        &plugins,
        flea::ui_logic_plugin_settings::pages::REMOVE_ID,
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(matches!(
        plugins.read_with(cx, |view, _| view.page().card().clone()),
        crate::g2::plugin_settings::page::Card::Remove { busy: false, .. }
    ));
    wire.0.borrow_mut().clear();
    (plugins, cx, wire)
}

#[gpui::test]
fn b02_a_click_on_the_remove_plugin_cancel_closes_the_card_and_sends_nothing(
    cx: &mut TestAppContext,
) {
    let (plugins, cx, wire) = remove_card(cx);
    click(cx, "plugins-card-cancel");
    assert_eq!(
        plugins.read_with(cx, |view, _| view.page().card().clone()),
        crate::g2::plugin_settings::page::Card::None,
        "Cancel closes the card, as Escape does"
    );
    assert!(
        wire.0.borrow().is_empty(),
        "nothing is removed: {:?}",
        wire.0.borrow()
    );
}

#[gpui::test]
fn b02_a_click_on_remove_plugin_sends_what_enter_sends_once(cx: &mut TestAppContext) {
    let enter = {
        let (_, cx, wire) = remove_card(cx);
        entered(cx, &wire)
    };
    let (plugins, cx, wire) = remove_card(cx);
    click(cx, "plugins-card-action");
    let clicked = wire.0.borrow().clone();
    assert_eq!(clicked.len(), 1, "one request: {clicked:?}");
    assert!(
        clicked[0].contains(r#""op":"remove""#) && clicked[0].contains(r#""plugin":"test.local""#),
        "the click asks to remove the open plugin: {clicked:?}"
    );
    assert_eq!(clicked, enter, "the click sends exactly what Enter sends");
    click(cx, "plugins-card-action");
    assert_eq!(
        wire.0.borrow().len(),
        1,
        "a click on the busy Remove sends nothing more, as Enter would not"
    );
    let busy = plugins.read_with(cx, |view, _| {
        matches!(
            view.page().card(),
            crate::g2::plugin_settings::page::Card::Remove { busy: true, .. }
        )
    });
    assert!(busy, "the card waits on its answer, as after Enter");
}
