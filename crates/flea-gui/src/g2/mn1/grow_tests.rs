//! #214: the file menu and its Copy As flyout grow to their widest row (copyas-menu, v0.3.12 ContextMenu.qml:502-503), so no label is cut.
use super::{
    lines::Env,
    menu::{Face, Menu},
};
use crate::list::Typeface;
use flea::{
    i18n::{I18n, Paths},
    ui_logic_mn1::{
        build::{listing_entries, Context},
        fit::{Rect, MENU_W},
    },
};
use gpui::{div, prelude::*, Context as GpuiContext, Render, TestAppContext, Window};

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut GpuiContext<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn whole(card: &super::menu::Card, what: &str) {
    for line in &card.built.lines {
        assert!(
            line.elided.is_none(),
            "{what}: {:?} is cut in a {} wide card",
            line.label.as_ref().map(|label| label.text.clone()),
            card.frame.w
        );
        assert!(
            line.wanted <= card.frame.w,
            "{what}: a row wider than its card"
        );
    }
}

#[gpui::test]
fn the_file_menu_and_copy_as_grow_to_their_widest_row_in_every_window(cx: &mut TestAppContext) {
    let catalog = I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    });
    let typeface = Typeface::omarchy_default();
    let env = Env {
        catalog: &catalog,
        preset: "default",
        key_hints: true,
        typeface: &typeface,
        inset: 4.,
    };
    // Every row shown, Delete Permanently with its key hint among them, as copyas-menu draws the menu.
    let rows = listing_entries(&Context {
        row_mode: 0o100644,
        row_is_file: true,
        can_convert: true,
        archive_formats: vec!["zip".into()],
        ..Context::default()
    });
    let handle = cx.add_window(|_, _| Host);
    for (w, h) in [(1100., 700.), (480., 240.)] {
        let work = Rect { x: 0., y: 0., w, h };
        handle
            .update(cx, |_, window, _| {
                let mut menu = Menu::default();
                menu.open(
                    Face::File,
                    rows.clone(),
                    (40., 40.),
                    work,
                    "a",
                    &env,
                    window,
                );
                let card = menu.card.as_ref().unwrap();
                assert!(
                    card.built.widest > MENU_W,
                    "the fixture's widest row must pass the menu token"
                );
                // The widest row fits both windows, so the card is exactly that row, in whole pixels.
                assert!(
                    card.built.widest <= w,
                    "the fixture's widest row fits the window"
                );
                assert_eq!(
                    card.frame.w,
                    card.built.widest.ceil(),
                    "the menu grows to its widest row and no wider"
                );
                whole(card, "menu");
                let parent = card.frame.w;
                menu.open_submenu_for("copyAs", false, &env, window);
                let flyout = &menu.flyout.as_ref().expect("Copy As opens").card;
                assert!(
                    flyout.frame.w >= parent,
                    "a flyout is at least its parent's width"
                );
                whole(flyout, "Copy As");
            })
            .unwrap();
    }
}
