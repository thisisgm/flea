//! Cases pinned to the boards and v0.3.12: every mark parses, cards stay in the work area, an open lands on the first row, the wheel follows the pointer.
use super::glyphs;
use super::lines::Env;
use super::menu::{Face, Menu};
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_context_menus::{
    build::{listing_entries, Context},
    fit::{self, Rect},
    rows::header_entries,
    Brand, Entry, Text,
};
use gpui::{div, prelude::*, Context as GpuiContext, Render, TestAppContext, Window, WindowHandle};
use std::{cell::RefCell, rc::Rc};

#[path = "uish_misc_review_tests.rs"]
mod uish_misc;

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut GpuiContext<Self>) -> impl IntoElement {
        div()
    }
}

const WIDE: Rect = Rect {
    x: 0.,
    y: 0.,
    w: 1100.,
    h: 700.,
};
const SMALL: Rect = Rect {
    x: 0.,
    y: 0.,
    w: 480.,
    h: 240.,
};

fn with<R>(cx: &mut TestAppContext, f: impl FnOnce(&mut Menu, &Env, &Window) -> R) -> R {
    let catalogs = vec![crate::test_catalogs::english_only()];
    let catalog = I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs,
    });
    let typeface = Typeface::omarchy_default();
    let env = Env {
        catalog: &catalog,
        preset: "default",
        key_hints: true,
        typeface: &typeface,
        inset: 4.,
    };
    let handle: WindowHandle<Host> = cx.add_window(|_, _| Host);
    let menu = Rc::new(RefCell::new(Menu::default()));
    handle
        .update(cx, |_, window, _| f(&mut menu.borrow_mut(), &env, window))
        .unwrap()
}

#[gpui::test]
fn r1_tailscale_mark_parses(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for brand in [Brand::Tailscale, Brand::Dropbox, Brand::LocalSend] {
            let (key, data, _) = glyphs::brand(brand);
            assert!(
                cx.svg_renderer().parse_svg(data).is_ok(),
                "{key} does not parse, so paint_svg draws nothing"
            );
        }
    });
}

#[gpui::test]
fn r1_trimmed_flyout_stays_in_the_work_area_at_its_foot_row(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(Face::File, rows, (4., 4.), SMALL, "a", env, window);
        let copy_as = menu
            .card
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .position(|e| e.action == "copyAs")
            .unwrap();
        while menu.card.as_ref().unwrap().cursor != Some(copy_as) {
            menu.act("cursorDown", None, "a", env, window);
        }
        menu.act("menuRight", None, "a", env, window);
        let open = menu.flyout.as_ref().unwrap().card.frame;
        for _ in 0..5 {
            menu.act("cursorDown", None, "a", env, window);
        }
        let fly = &menu.flyout.as_ref().unwrap().card;
        assert_eq!(fly.cursor, Some(5), "the cursor walked to Shell-Quoted");
        let f = fly.frame;
        assert!(
            f.y + f.h <= SMALL.h - 4.,
            "flyout {f:?} (opened as {open:?}) runs past the 240 tall work area's inset"
        );
    });
}

#[gpui::test]
fn r1_refresh_keeps_the_card_in_the_work_area(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(
            Face::File,
            rows.clone(),
            (1090., 690.),
            WIDE,
            "a",
            env,
            window,
        );
        let before = menu.card.as_ref().unwrap().frame;
        let mut more = rows;
        let at = more.iter().position(|e| e.action == "trash").unwrap();
        more.insert(
            at,
            Entry::row(
                "localsend",
                Text::Data("Send with LocalSend to a peer with a long name".into()),
                "send",
            ),
        );
        assert!(menu.refresh(more, env, window));
        let f = menu.card.as_ref().unwrap().frame;
        assert!(
            f.y + f.h <= WIDE.h - 4. && f.x + f.w <= WIDE.w,
            "card {f:?} (was {before:?}) left the work area after a provider refresh"
        );
    });
}

#[gpui::test]
fn r1_pointer_open_lands_on_the_first_row(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let rows = header_entries(&["mode".to_owned(), "kind".to_owned()], false);
        menu.open(Face::Header, rows, (228., 40.), WIDE, "", env, window);
        assert_eq!(menu.card.as_ref().unwrap().cursor, Some(0), "header-menu board and v0.3.12 place(): the first row holds the cursor after a right-click open");
    });
}

#[gpui::test]
fn r1_wheel_over_the_card_steps_the_card_not_the_flyout(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(Face::File, rows, (40., 40.), WIDE, "a", env, window);
        menu.open_submenu_for("copyAs", false, env, window);
        assert!(menu.flyout.is_some());
        let card = menu.card.as_ref().unwrap();
        // Copy As sits past the fold, so the card has scrolled and the pointer rests on its first drawn row.
        assert!(card.content_y > 0, "Copy As scrolled the card");
        let (x, y) = (card.frame.x + 20., card.frame.y + 16.);
        let f = menu.flyout.as_ref().unwrap().card.frame;
        assert!(
            !(x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h),
            "the pointer rests over the file menu, not the flyout"
        );
        menu.wheel(x, y, -120);
        assert!(menu.flyout.is_none(), "v0.3.12 stepMain(): a wheel over the main frame closes the flyout first, then steps the menu");
        let at = menu.card.as_ref().unwrap().cursor;
        assert_eq!(
            menu.wheel(WIDE.w - 2., WIDE.h - 2., -120),
            super::menu::Outcome::Nothing,
            "the ground outside both cards swallows the wheel"
        );
        assert_eq!(menu.card.as_ref().unwrap().cursor, at);
    });
}

#[gpui::test]
fn r1_every_cut_mark_parses(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for (name, key, data) in glyphs::CUT {
            assert!(
                cx.svg_renderer().parse_svg(data).is_ok(),
                "{name} ({key}) does not parse"
            );
        }
    });
}

#[test]
fn int14_the_cut_table_draws_exactly_the_core_mark_names() {
    let drawn: Vec<&str> = glyphs::CUT.iter().map(|(name, _, _)| *name).collect();
    assert_eq!(
        drawn,
        flea::ui_logic_context_menus::entry::MARK_NAMES,
        "one mark-name list, in one order"
    );
    for name in flea::ui_logic_context_menus::entry::MARK_NAMES {
        assert_eq!(
            flea::ui_logic_custom_actions::rows::static_glyph(name),
            Some(*name),
            "a plugin row may name {name}"
        );
    }
}

#[gpui::test]
fn r1_touchpad_travel_never_carries_from_the_card_into_its_flyout(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(Face::File, rows, (40., 40.), WIDE, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        menu.touchpad(card.frame.x + 20., card.row_screen_top(0) + 10., -30., true);
        menu.open_submenu_for("copyAs", false, env, window);
        let f = menu.flyout.as_ref().unwrap().card.frame;
        let (x, y) = (f.x + f.w - 10., f.y + 20.);
        let start = menu.flyout.as_ref().unwrap().card.cursor;
        menu.touchpad(x, y, -10., false);
        assert_eq!(menu.flyout.as_ref().unwrap().card.cursor, start, "v0.3.12 keeps one remainder per card: 30 px on the menu and 10 px on its flyout step neither");
        menu.touchpad(x, y, -30., true);
        assert_eq!(
            menu.flyout.as_ref().unwrap().card.cursor,
            start,
            "a new swipe starts from no leftover, as v0.3.12 resets it on ScrollBegin"
        );
        menu.touchpad(x, y, -10., false);
        assert_ne!(
            menu.flyout.as_ref().unwrap().card.cursor,
            start,
            "within one swipe the leftover adds up to a row"
        );
    });
}

#[gpui::test]
fn r1_refresh_keeps_a_hover_opened_flyout_open_on_its_leaf(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(Face::File, rows.clone(), (40., 40.), WIDE, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        let copy_as = card
            .entries
            .iter()
            .position(|e| e.action == "copyAs")
            .unwrap();
        // Copy As opens past the fold, so the wheel brings it into view first, as a pointer user would.
        let (x, mid) = (card.frame.x + 20., card.frame.y + card.frame.h / 2.);
        for _ in 0..20 {
            let card = menu.card.as_ref().unwrap();
            if card.row_screen_top(copy_as) + fit::ROW as f32
                <= card.frame.y + card.frame.h - fit::PAD as f32
            {
                break;
            }
            menu.wheel(x, mid, -120);
        }
        let y = menu.card.as_ref().unwrap().row_screen_top(copy_as) + 10.;
        menu.hover(x, y, env, window);
        assert_eq!(
            menu.flyout.as_ref().map(|f| (f.parent, f.card.cursor)),
            Some((Some(copy_as), Some(0))),
            "hover opens Copy As on its first leaf"
        );
        menu.act("cursorDown", None, "a", env, window);
        assert_eq!(
            menu.flyout.as_ref().map(|f| f.card.cursor),
            Some(Some(1)),
            "Down steps the hovered flyout"
        );
        let mut more = rows;
        // LocalSend joins the send group, the separator before Copy As closing it.
        let at = more.iter().position(|e| e.action == "copyAs").unwrap() - 1;
        more.insert(
            at,
            Entry::row(
                "localsend",
                Text::Data("Send with LocalSend".into()),
                "send",
            ),
        );
        assert!(menu.refresh(more, env, window));
        let fly = menu
            .flyout
            .as_ref()
            .map(|f| (f.action.as_str(), f.card.cursor));
        assert_eq!(
            fly,
            Some(("copyAs", Some(1))),
            "a provider answer keeps the flyout open on its leaf"
        );
    });
}
