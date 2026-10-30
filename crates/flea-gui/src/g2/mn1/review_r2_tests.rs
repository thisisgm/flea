//! Cases pinned to v0.3.12 across work areas: flyouts stay in the work area, and a hovered flyout lands on its first leaf with the keys.
use super::lines::Env;
use super::menu::{Face, Menu, Outcome};
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_mn1::{
    build::{listing_entries, App, Context},
    fit::Rect,
    Entry, Text,
};
use gpui::{div, prelude::*, Context as GpuiContext, Render, TestAppContext, Window, WindowHandle};
use std::{cell::RefCell, rc::Rc};

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
const INSET: f32 = 4.;

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
        inset: INSET,
    };
    let handle: WindowHandle<Host> = cx.add_window(|_, _| Host);
    let menu = Rc::new(RefCell::new(Menu::default()));
    handle
        .update(cx, |_, window, _| f(&mut menu.borrow_mut(), &env, window))
        .unwrap()
}

fn rows(hidden: &[&str]) -> Vec<Entry> {
    let tags = (1..=7u8)
        .map(|slot| Entry {
            tag: Some(slot),
            ..Entry::leaf(format!("tag{slot}"), Text::Data(format!("Tag {slot}")))
        })
        .collect();
    listing_entries(&Context {
        row_mode: 0o100644,
        row_is_file: true,
        printable: true,
        hidden: hidden.iter().map(|s| (*s).to_owned()).collect(),
        archive_formats: ["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "7z"]
            .map(String::from)
            .to_vec(),
        tag_submenu: Some(tags),
        open_with_apps: vec![App {
            id: "nvim.desktop".into(),
            label: "Neovim".into(),
            icon: "nvim".into(),
            default: true,
        }],
        open_with_loaded: true,
        ..Context::default()
    })
}

fn inside(f: Rect, work: Rect) -> bool {
    let e = 0.01;
    f.x >= work.x - e
        && f.x + f.w <= work.x + work.w + e
        && f.y >= work.y + INSET - e
        && f.y + f.h <= work.y + work.h - INSET + e
}

fn check(menu: &Menu, work: Rect, what: &str) {
    if let Some(card) = &menu.card {
        assert!(
            inside(card.frame, work),
            "{what}: card {:?} outside {work:?}",
            card.frame
        );
    }
    if let Some(fly) = &menu.flyout {
        assert!(
            inside(fly.card.frame, work),
            "{what}: flyout {:?} outside {work:?}",
            fly.card.frame
        );
    }
}

fn parents(menu: &Menu) -> Vec<usize> {
    let card = menu.card.as_ref().unwrap();
    card.entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.has_submenu() && !e.disabled)
        .map(|(i, _)| i)
        .collect()
}

fn cursor_to(menu: &mut Menu, row: usize, env: &Env, window: &Window) {
    for _ in 0..80 {
        if menu.card.as_ref().unwrap().cursor == Some(row) {
            return;
        }
        menu.act("cursorDown", None, "a", env, window);
    }
    panic!("cursor never reached row {row}");
}

#[gpui::test]
fn r2_every_card_and_flyout_stays_in_the_work_area_across_keys_wheel_hover_and_refresh(
    cx: &mut TestAppContext,
) {
    with(cx, |menu, env, window| {
        let mut runs = 0;
        for &(w, h) in &[
            (480., 240.),
            (480., 260.),
            (700., 300.),
            (700., 333.),
            (1100., 400.),
            (1100., 480.),
            (1100., 561.),
            (1100., 700.),
        ] {
            let work = Rect { x: 0., y: 0., w, h };
            for &(px_, py) in &[
                (4., 4.),
                (w / 2., h / 3.),
                (w / 2., h / 2.),
                (w - 10., h - 10.),
            ] {
                for hidden in [
                    &[][..],
                    &[
                        "delete",
                        "openTerminal",
                        "moveto",
                        "copyto",
                        "properties",
                        "permissions",
                    ][..],
                ] {
                    let all = rows(hidden);
                    menu.open(Face::File, all.clone(), (px_, py), work, "a", env, window);
                    let parents = parents(menu);
                    for &parent in &parents {
                        // Keyboard: the parent, Right, every leaf down and back up, then the wheel over the flyout.
                        menu.open(Face::File, all.clone(), (px_, py), work, "a", env, window);
                        cursor_to(menu, parent, env, window);
                        menu.act("menuRight", None, "a", env, window);
                        let what = format!("{w}x{h} at ({px_},{py}) parent {parent}");
                        check(menu, work, &format!("{what} key open"));
                        for _ in 0..12 {
                            menu.act("cursorDown", None, "a", env, window);
                            check(menu, work, &format!("{what} down"));
                        }
                        for _ in 0..12 {
                            menu.act("cursorUp", None, "a", env, window);
                            check(menu, work, &format!("{what} up"));
                        }
                        if let Some(f) = menu.flyout.as_ref().map(|f| f.card.frame) {
                            for units in [
                                -120, -120, -120, -120, -120, -120, -120, 120, 120, 120, 120, 120,
                                120, 120,
                            ] {
                                menu.wheel(f.x + f.w / 2., f.y + f.h / 2., units);
                                check(menu, work, &format!("{what} wheel"));
                            }
                        }
                        // A provider answer that adds a row and widens the menu while the flyout is open.
                        menu.open(Face::File, all.clone(), (px_, py), work, "a", env, window);
                        cursor_to(menu, parent, env, window);
                        menu.act("menuRight", None, "a", env, window);
                        for _ in 0..5 {
                            menu.act("cursorDown", None, "a", env, window);
                        }
                        let mut more = all.clone();
                        let at = more
                            .iter()
                            .position(|e| e.action == "trash")
                            .unwrap_or(more.len());
                        more.insert(
                            at,
                            Entry::row(
                                "localsend",
                                Text::Data(
                                    "Send with LocalSend to a machine with a long name".into(),
                                ),
                                "send",
                            ),
                        );
                        menu.refresh(more, env, window);
                        check(menu, work, &format!("{what} refresh"));
                        for _ in 0..12 {
                            menu.act("cursorDown", None, "a", env, window);
                            check(menu, work, &format!("{what} refresh down"));
                        }
                        // Pointer: the parent row in view, hovered, then every flyout leaf hovered.
                        menu.open(Face::File, all.clone(), (px_, py), work, "a", env, window);
                        cursor_to(menu, parent, env, window);
                        // Hovering the row that already holds the cursor opens nothing, so the cursor steps off it first.
                        menu.act("cursorUp", None, "a", env, window);
                        let card = menu.card.as_ref().unwrap();
                        let (x, y) = (card.frame.x + 20., card.row_screen_top(parent) + 6.);
                        if y >= card.frame.y + (6 + card.view_h) as f32 {
                            continue;
                        }
                        menu.hover(x, y, env, window);
                        assert!(menu.flyout.is_some(), "{what}: hover opens the flyout");
                        check(menu, work, &format!("{what} hover"));
                        if let Some(f) = menu.flyout.as_ref().map(|f| f.card.frame) {
                            let mut yy = f.y + 8.;
                            while yy < f.y + f.h {
                                menu.hover(f.x + f.w / 2., yy, env, window);
                                check(menu, work, &format!("{what} hover leaf"));
                                yy += 9.;
                            }
                        }
                        runs += 1;
                    }
                }
            }
        }
        assert!(runs > 100, "the sweep ran {runs} parent walks");
    });
}

#[gpui::test]
fn r2_hover_opened_flyout_lands_on_its_first_leaf_and_takes_the_keys(cx: &mut TestAppContext) {
    with(cx, |menu, env, window| {
        let all = rows(&[
            "delete",
            "openTerminal",
            "moveto",
            "copyto",
            "properties",
            "permissions",
        ]);
        let hover_copy_as = |menu: &mut Menu| {
            menu.open(Face::File, all.clone(), (40., 40.), WIDE, "a", env, window);
            let card = menu.card.as_ref().unwrap();
            let copy_as = card
                .entries
                .iter()
                .position(|e| e.action == "copyAs")
                .unwrap();
            assert!(
                card.row_screen_top(copy_as) + 37. <= card.frame.y + card.frame.h,
                "Copy As is in view"
            );
            menu.hover(
                card.frame.x + 20.,
                card.row_screen_top(copy_as) + 18.,
                env,
                window,
            );
            assert_eq!(
                menu.flyout.as_ref().map(|f| f.action.as_str()),
                Some("copyAs"),
                "hovering Copy As opens its flyout"
            );
        };
        hover_copy_as(menu);
        let opened = menu.flyout.as_ref().map(|f| f.card.cursor);
        menu.act("cursorDown", None, "a", env, window);
        let down = menu.flyout.as_ref().map(|f| f.card.cursor);
        hover_copy_as(menu);
        let enter = menu.act("open", None, "a", env, window);
        let want = Outcome::Fire {
            action: "copyAs".into(),
            leaf: "copyPath".into(),
        };
        assert!(
            opened == Some(Some(0)) && down == Some(Some(1)) && enter == want,
            "v0.3.12 openSubmenu() sets submenuCursor 0 on a hover open and keyCatcher sends Down and Enter to an open flyout; \
             every flyout board draws a leaf lit. Got: flyout cursor after hover {opened:?}, flyout cursor after Down {down:?} \
             (None = flyout closed), Enter {enter:?}"
        );
    });
}
