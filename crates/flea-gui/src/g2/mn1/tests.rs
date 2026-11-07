//! The menu primitive driven in real GPUI windows: keys, pointer, flyouts, refusals, the width fit, the scroll cue and paint.
use super::element::MenuElement;
use super::ipc::MenuAdapter;
use super::lines::{self, Env};
use super::menu::{Face, Menu, Outcome};
use crate::ipc::Adapter;
use crate::list::{bench::allocations, Typeface};
use flea::i18n::{I18n, Paths};
use flea::ui_logic_mn1::{
    build::{listing_entries, Context, Peer},
    fit::{self, Rect, MENU_W, PAD},
    places, Entry, Text,
};
use gpui::{
    div, prelude::*, px, Context as GpuiContext, Render, TestAppContext, Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

pub(super) struct Host {
    menu: Rc<RefCell<Menu>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut GpuiContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(MenuElement::new(Rc::clone(&self.menu), px(8.)))
    }
}

struct Fixture {
    catalog: I18n,
    typeface: Typeface,
    hints: bool,
}

impl Fixture {
    fn new() -> Self {
        let catalogs = vec![crate::test_catalogs::english_only()];
        let paths = Paths {
            omarchy: "/nonexistent".into(),
            language_file: "/nonexistent".into(),
            catalogs,
        };
        Self {
            catalog: I18n::load(paths),
            typeface: Typeface::omarchy_default(),
            hints: true,
        }
    }

    fn env(&self) -> Env<'_> {
        Env {
            catalog: &self.catalog,
            preset: "default",
            key_hints: self.hints,
            typeface: &self.typeface,
            inset: 4.,
        }
    }
}

pub(super) const WIDE: Rect = Rect {
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

pub(super) fn window(cx: &mut TestAppContext) -> (WindowHandle<Host>, Rc<RefCell<Menu>>) {
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    (cx.add_window(move |_, _| Host { menu: shared }), menu)
}

pub(super) fn with<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Host>,
    f: impl FnOnce(&mut Menu, &Env, &Window) -> R,
) -> R {
    let fixture = Fixture::new();
    handle
        .update(cx, |host, window, _| {
            f(&mut host.menu.borrow_mut(), &fixture.env(), window)
        })
        .unwrap()
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<Host>) {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.simulate_next_frame(cx)
    })
    .unwrap();
}

pub(super) fn file_rows() -> Vec<Entry> {
    listing_entries(&Context {
        row_mode: 0o100644,
        row_is_file: true,
        row_is_image: true,
        can_convert: true,
        archive_formats: vec!["zip".into()],
        taildrop_installed: true,
        taildrop_peers: vec![Peer {
            id: "box".into(),
            label: "Box".into(),
        }],
        hidden: vec![
            "delete".into(),
            "openTerminal".into(),
            "moveto".into(),
            "copyto".into(),
            "properties".into(),
            "permissions".into(),
            "pasteAs".into(),
        ],
        ..Context::default()
    })
}

fn read(menu: &Menu, function: &str, args: &[&str]) -> String {
    let adapter = MenuAdapter {
        menu,
        origin: (0., 0.),
    };
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    adapter
        .call(function, &args)
        .map(|reply| reply.stdout.trim_end().to_owned())
        .unwrap_or_default()
}

#[gpui::test]
fn keyboard_open_steps_and_escape_returns_focus(cx: &mut TestAppContext) {
    let (handle, menu) = window(cx);
    with(cx, handle, |menu, env, window| {
        assert_eq!(
            menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window),
            Outcome::Redraw
        );
        assert_eq!(
            menu.card.as_ref().unwrap().cursor,
            Some(0),
            "the first row by key"
        );
        menu.act("cursorUp", None, "a", env, window);
        assert_eq!(
            menu.card.as_ref().unwrap().cursor,
            Some(0),
            "an end keeps the cursor"
        );
        menu.act("cursorDown", None, "a", env, window);
        menu.act("cursorDown", None, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        assert!(
            card.entries[card.cursor.unwrap()].steppable(),
            "a step never lands on a separator or a disabled row"
        );
        assert_eq!(
            menu.act("escape", None, "a", env, window),
            Outcome::Closed,
            "Escape closes and hands focus back"
        );
        assert!(!menu.opened());
    });
    assert_eq!(read(&menu.borrow(), "contextMenuVisible", &[]), "false");
}

#[gpui::test]
fn sol_menu_order_pointer_hover_lights_a_flyout_parent(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.card.as_ref().unwrap().cursor,
            Some(0),
            "a pointer open lands on the first row, as v0.3.12 place() does"
        );
        let card = menu.card.as_ref().unwrap();
        let open_with = card
            .entries
            .iter()
            .position(|e| e.action == "openWith")
            .unwrap();
        let y = card.row_screen_top(open_with) + 10.;
        let x = card.frame.x + 30.;
        assert_eq!(menu.hover(x, y, env, window), Outcome::Redraw);
        let flyout = menu
            .flyout
            .as_ref()
            .expect("hovering a flyout parent opens its flyout");
        assert_eq!(
            (flyout.parent, flyout.card.cursor),
            (Some(open_with), Some(0)),
            "the parent is lit and the flyout lands on its first leaf"
        );
        assert!(
            flyout.card.frame.w >= menu.card.as_ref().unwrap().frame.w,
            "a flyout is at least its parent's width"
        );
        assert_eq!(
            flyout.card.frame.x,
            menu.card.as_ref().unwrap().frame.x + menu.card.as_ref().unwrap().frame.w,
            "beside its parent"
        );
        menu.act("cursorDown", None, "a", env, window);
        assert_eq!(
            menu.click(x, y, None, "a", env, window),
            Outcome::Redraw,
            "a press on the parent opens its flyout afresh"
        );
        assert_eq!(
            menu.flyout.as_ref().map(|f| (f.parent, f.card.cursor)),
            Some((Some(open_with), Some(0))),
            "back on its first leaf"
        );
        let cut = menu
            .card
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .position(|e| e.action == "cut")
            .unwrap();
        menu.hover(
            x,
            menu.card.as_ref().unwrap().row_screen_top(cut) + 10.,
            env,
            window,
        );
        assert!(menu.flyout.is_none(), "another row closes the flyout");
        let card = menu.card.as_ref().unwrap();
        let below = card.frame.y + card.frame.h + 40.;
        assert_eq!(
            menu.click(x, below, None, "a", env, window),
            Outcome::Closed,
            "a press outside closes"
        );
    });
}

#[gpui::test]
fn flyout_keys_letters_and_tags(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        let mut rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            hidden: Vec::new(),
            clipboard_available: true,
            ..Context::default()
        });
        let tags: Vec<Entry> = (1..=7)
            .map(|n| Entry {
                tag: Some(n),
                ..Entry::leaf(format!("tag{n}"), Text::Data(format!("Tag {n}")))
            })
            .collect();
        if let Some(row) = rows.iter_mut().find(|e| e.action == "copyAs") {
            row.disabled = false;
        }
        rows.push(Entry {
            submenu: Some(tags),
            ..Entry::row("tags", Text::Key("menu.row.tags"), "tag")
        });
        menu.open(Face::File, rows.clone(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.open_submenu_for("copyAs", true, env, window),
            Outcome::Redraw
        );
        assert_eq!(
            menu.flyout.as_ref().unwrap().card.cursor,
            Some(0),
            "the flyout's first leaf by key"
        );
        assert_eq!(
            menu.act("parent", None, "a", env, window),
            Outcome::Redraw,
            "Left closes the flyout, the menu stays"
        );
        assert!(menu.opened() && menu.flyout.is_none());
        menu.open_submenu_for("copyAs", true, env, window);
        assert_eq!(
            menu.act("copyName", Some(&rows), "a", env, window),
            Outcome::Fire {
                action: "copyAs".into(),
                leaf: "copyName".into()
            }
        );
        menu.open(Face::File, rows.clone(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.act("tag3", Some(&rows), "a", env, window),
            Outcome::Fire {
                action: "tags".into(),
                leaf: "tag3".into()
            }
        );
        menu.open(Face::File, rows.clone(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.act("open", Some(&rows), "b", env, window),
            Outcome::Refused("menu.refusal.moved"),
            "a stale selection refuses"
        );
        assert!(!menu.opened(), "and closes");
    });
}

#[gpui::test]
fn lone_flyouts_and_locked_refusals(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        let rows = file_rows();
        menu.open(Face::File, rows.clone(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.open_submenu_for("pasteAs", false, env, window),
            Outcome::Refused("menu.refusal.empty_clipboard")
        );
        menu.open(Face::File, rows.clone(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(
            menu.open_submenu_for("pasteAs", true, env, window),
            Outcome::Redraw,
            "a hidden Paste As opens lone"
        );
        assert_eq!(menu.flyout.as_ref().unwrap().parent, None);
        assert_eq!(
            menu.act("pasteLink", None, "b", env, window),
            Outcome::Refused("menu.refusal.moved"),
            "a moved selection outranks the leaf"
        );
        let shipped: Vec<String> = [
            "delete",
            "openTerminal",
            "placeMenu",
            "runScript",
            "moveto",
            "copyto",
            "properties",
            "permissions",
            "copyAs",
            "extThumbs",
        ]
        .map(String::from)
        .to_vec();
        menu.open(
            Face::Rail,
            vec![Entry::row(
                "open",
                Text::Key("menu.row.open"),
                "folder-open",
            )],
            (10., 10.),
            WIDE,
            "",
            env,
            window,
        );
        assert_eq!(
            menu.open_locked("/locked", 0o040000, &shipped, (60., 60.), WIDE, env, window),
            Outcome::Refused(places::LOCKED_REFUSAL)
        );
        assert!(
            !menu.opened() && menu.handle.is_empty(),
            "a stale refusal closes the open rail menu and leaves no target"
        );
        let one: Vec<String> = vec!["openTerminal".into(), "permissions".into()];
        assert_eq!(
            menu.open_locked("/locked", 0o040000, &one, (60., 60.), WIDE, env, window),
            Outcome::Redraw
        );
        assert_eq!(
            (
                menu.handle.as_str(),
                menu.card.as_ref().unwrap().entries.len()
            ),
            ("/locked", 1),
            "one row draws copypath alone"
        );
        assert_eq!(
            menu.act("open", None, "", env, window),
            Outcome::Fire {
                action: "copypath".into(),
                leaf: String::new()
            }
        );
    });
}

#[gpui::test]
fn one_width_computation_per_build_and_none_for_an_unchanged_refresh(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        let before = lines::builds();
        menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window);
        assert_eq!(lines::builds() - before, 1);
        assert!(
            !menu.refresh(file_rows(), env, window),
            "the same answer rebuilds nothing"
        );
        assert_eq!(lines::builds() - before, 1);
        let mut more = file_rows();
        let at = more.iter().position(|e| e.action == "trash").unwrap();
        more.insert(
            at,
            Entry::row("localsend", Text::Key("menu.row.localsend"), "send"),
        );
        menu.act("cursorDown", None, "a", env, window);
        let action = {
            let c = menu.card.as_ref().unwrap();
            c.entries[c.cursor.unwrap()].action.to_string()
        };
        assert!(menu.refresh(more, env, window));
        let card = menu.card.as_ref().unwrap();
        assert_eq!(
            card.entries[card.cursor.unwrap()].action,
            action,
            "a provider change keeps the cursor's action"
        );
        assert_eq!(lines::builds() - before, 2);
    });
}

#[gpui::test]
fn the_card_fits_its_widest_row_inside_the_work_area(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        assert_eq!(
            card.frame.w,
            fit::fit_width(card.built.widest, WIDE.w),
            "the frame is the widest row, never under the token"
        );
        assert!(card.frame.w >= MENU_W);
        assert!(
            card.built.lines.iter().all(|l| l.wanted <= card.frame.w),
            "no label clipped in English"
        );
        let german: Vec<Entry> = [
            "Mit anderer Anwendung \u{f6}ffnen",
            "In neuem Tab \u{f6}ffnen",
            "Dauerhaft l\u{f6}schen\u{2026}",
        ]
        .iter()
        .map(|label| Entry::row("open", Text::Data((*label).into()), "file"))
        .collect();
        menu.open(Face::File, german, (40., 40.), WIDE, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        assert!(
            card.frame.w >= card.built.widest.ceil()
                && card.built.lines.iter().all(|l| l.wanted <= card.frame.w),
            "long German labels grow the card whole"
        );
        menu.open(
            Face::File,
            file_rows(),
            (470., 230.),
            SMALL,
            "a",
            env,
            window,
        );
        let f = menu.card.as_ref().unwrap().frame;
        assert!(
            f.x >= 0. && f.x + f.w <= SMALL.w && f.y >= 4. && f.y + f.h <= SMALL.h - 4. + 0.5,
            "inside 480x240: {f:?}"
        );
    });
}

#[gpui::test]
fn sol_menu_order_a_long_menu_scrolls_with_a_half_row_cue(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (10., 10.), SMALL, "a", env, window);
        let card = menu.card.as_ref().unwrap();
        assert!(
            fit::content_height(&card.entries) > card.view_h,
            "past the fold"
        );
        assert!(
            fit::cue_ok(&card.entries, card.content_y, card.view_h),
            "at rest the bottom edge cuts a row at half"
        );
        for _ in 0..40 {
            menu.act("cursorDown", None, "a", env, window);
        }
        let card = menu.card.as_ref().unwrap();
        let top = fit::row_top(&card.entries, card.cursor.unwrap()) - card.content_y;
        assert!(
            top >= 0 && top + fit::ROW <= card.view_h,
            "the cursor row is inside the view"
        );
        assert_eq!(
            card.entries[card.cursor.unwrap()].action,
            "trash",
            "Move to Trash is last, as the HIG file menu draws it"
        );
        let before = card.content_y;
        let (x, y) = (card.frame.x + 10., card.frame.y + card.frame.h / 2.);
        for _ in 0..40 {
            menu.wheel(x, y, 120);
        }
        let card = menu.card.as_ref().unwrap();
        assert!(
            card.content_y < before && card.cursor == Some(0),
            "the wheel steps the cursor back up a row a notch"
        );
        assert_eq!(card.frame.h, (card.view_h + 2 * PAD) as f32);
    });
}

#[gpui::test]
fn hints_follow_the_switch(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    let fixture = Fixture {
        hints: false,
        ..Fixture::new()
    };
    handle
        .update(cx, |host, window, _| {
            let mut menu = host.menu.borrow_mut();
            menu.open(
                Face::File,
                file_rows(),
                (40., 40.),
                WIDE,
                "a",
                &fixture.env(),
                window,
            );
            assert!(
                read(&menu, "contextMenuHints", &[])
                    .chars()
                    .all(|c| c == '|'),
                "hints off draw no key"
            );
            let on = Fixture::new();
            menu.open(
                Face::File,
                file_rows(),
                (40., 40.),
                WIDE,
                "a",
                &on.env(),
                window,
            );
            let hints = read(&menu, "contextMenuHints", &[]);
            let entries = read(&menu, "contextMenuEntries", &[]);
            let cut = entries.split('|').position(|l| l == "Cut").unwrap();
            assert_eq!(
                hints.split('|').nth(cut),
                Some("x"),
                "Cut wears its key: {hints}"
            );
        })
        .unwrap();
}

#[gpui::test]
fn sol_menu_order_ipc_reads_the_drawn_rows(cx: &mut TestAppContext) {
    let (handle, menu) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (40., 40.), WIDE, "a", env, window);
    });
    let menu = menu.borrow();
    let entries = read(&menu, "contextMenuEntries", &[]);
    assert!(
        entries.starts_with("Open|Open With|Run|Open as Text|-|Cut|Copy|Paste")
            && entries.ends_with("|-|Move to Trash"),
        "The HIG file menu closes with Move to Trash alone: {entries}"
    );
    assert!(
        read(&menu, "contextMenuGlyphs", &[]).contains("tailscale"),
        "a brand row names its own mark"
    );
    assert_eq!(read(&menu, "contextMenuCursor", &[]), "0");
    let rect = read(&menu, "contextMenuRect", &[]);
    assert_eq!(rect.split(' ').count(), 4, "{rect}");
    assert!(!read(&menu, "contextMenuRowCentre", &["0"]).is_empty());
    assert!(read(&menu, "menuState", &[]).starts_with("{\"opened\":true,\"cursor\":0"));
}

#[gpui::test]
fn paint_allocates_nothing_per_frame(cx: &mut TestAppContext) {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        menu.open(Face::File, file_rows(), (10., 10.), SMALL, "a", env, window);
        menu.open_submenu_for("openWith", false, env, window);
    });
    draw(cx, handle);
    draw(cx, handle);
    let before = allocations();
    draw(cx, handle);
    let spent = allocations() - before;
    assert!(
        spent < 64,
        "a retained frame stays near GPUI's own floor: {spent}"
    );
}

#[gpui::test]
fn marks_and_a_trimmed_flyout_follow_the_boards(cx: &mut TestAppContext) {
    let (handle, menu) = window(cx);
    assert_eq!(
        super::glyphs::mark("no-such-mark").0.as_ref(),
        "flea-menu:file",
        "an unknown mark draws the plain file mark"
    );
    with(cx, handle, |menu, env, window| {
        let rows = listing_entries(&Context {
            has_row: false,
            selection_count: 0,
            sort_by: "name".into(),
            ..Context::default()
        });
        menu.open(Face::Background, rows, (900., 5.), WIDE, "", env, window);
        let sort = menu
            .card
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .position(|e| e.action == "sort")
            .unwrap();
        while menu.card.as_ref().unwrap().cursor != Some(sort) {
            menu.act("cursorDown", None, "", env, window);
        }
        menu.act("menuRight", None, "", env, window);
    });
    assert_eq!(
        read(&menu.borrow(), "contextMenuSubmenuGlyphs", &[]),
        "checkmark|||",
        "Sort By checks the order in effect with the menus' checkmark"
    );
    with(cx, handle, |menu, env, window| {
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
        let parent = menu.card.as_ref().unwrap().frame;
        let fly = &menu.flyout.as_ref().expect("Copy As opens its flyout").card;
        assert!(
            fly.view_h < fit::content_height(&fly.entries),
            "the six rows overflow 480x240 and trim to a half row"
        );
        assert_eq!(
            fly.frame.y + fly.frame.h,
            parent.y + parent.h,
            "the trimmed flyout's foot stays level with the file menu's"
        );
    });
}

#[gpui::test]
fn copy_as_in_a_small_window_shows_a_half_row_under_each_parent_fade(cx: &mut TestAppContext) {
    let (handle, menu) = window(cx);
    with(cx, handle, |menu, env, window| {
        // Text size 14 insets the menu 5 px, so the file menu's frame is 230 tall at 480x240.
        let env = Env { inset: 5., ..*env };
        let rows = listing_entries(&Context {
            row_mode: 0o100644,
            row_is_file: true,
            has_row: true,
            selection_count: 1,
            hidden: Vec::new(),
            ..Context::default()
        });
        menu.open(Face::File, rows, (4., 4.), SMALL, "a", &env, window);
        menu.open_submenu_for("copyAs", true, &env, window);
        let card = menu.card.as_ref().unwrap();
        assert_eq!(card.frame.h, 230., "the accepted frame keeps its height");
        let (top, bottom) = fit::edge_cuts(&card.entries, card.content_y, card.view_h);
        let label = |cut: Option<(usize, i32)>| {
            cut.map(|(index, shown)| (card.entries[index].action.to_string(), shown))
        };
        assert_eq!(
            (label(top), label(bottom)),
            (
                Some(("openTerminal".to_owned(), 16)),
                Some(("tags".to_owned(), 17))
            ),
            "half of Open in Terminal over Move To and half of Tags under Copy As"
        );
        assert!(fit::keeps_clear(
            &card.entries,
            card.cursor,
            card.content_y,
            card.view_h
        ));
    });
    draw(cx, handle);
    let state = read(&menu.borrow(), "menuState", &[]);
    assert!(
        state.contains("\"view\":218"),
        "the view is not trimmed: {state}"
    );
}

#[path = "text_size_tests.rs"]
mod text_size;
