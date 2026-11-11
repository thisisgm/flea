//! The tags flyout's actual paint uses each tag's surface ink, including on low-chroma palettes, through every phase of the open fade.
use super::MenuElement;
use crate::{
    g2::{
        context_menus::{
            lines::Env,
            menu::{Face, Menu},
        },
        rail_places::{
            content::{tag_mark, theme_tags, Ground},
            seams::ThemeRoles,
        },
    },
    list::Typeface,
    palette::Theme,
};
use flea::{
    i18n::{I18n, Paths},
    ui_logic_context_menus::{
        build::{listing_entries, Context as MenuContext},
        fit::Rect,
        Entry, Text,
    },
    ui_logic_rail_places::tags::NAMES,
};
use gpui::{div, prelude::*, px, Context, Quad, Render, TestAppContext, Window, WindowHandle};
use std::{cell::RefCell, rc::Rc, time::Duration};

struct Host(Rc<RefCell<Menu>>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(MenuElement::new(Rc::clone(&self.0), px(8.)))
    }
}

fn host(cx: &mut TestAppContext) -> (WindowHandle<Host>, Rc<RefCell<Menu>>) {
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    (cx.add_window(move |_, _| Host(shared)), menu)
}

// Opens the file menu with its tags flyout, as a right click and menuRight do.
fn open_tags(cx: &mut TestAppContext, handle: WindowHandle<Host>, menu: &Rc<RefCell<Menu>>) {
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
    handle
        .update(cx, |_, window, _| {
            let tags = NAMES
                .iter()
                .enumerate()
                .map(|(at, name)| Entry {
                    tag: Some(at as u8 + 1),
                    ..Entry::leaf(format!("tag{}", at + 1), Text::Data((*name).into()))
                })
                .collect();
            let rows = listing_entries(&MenuContext {
                row_mode: 0o100644,
                row_is_file: true,
                hidden: Vec::new(),
                tag_submenu: Some(tags),
                ..MenuContext::default()
            });
            let mut menu = menu.borrow_mut();
            menu.open(
                Face::File,
                rows,
                (20., 20.),
                Rect {
                    x: 0.,
                    y: 0.,
                    w: 1100.,
                    h: 700.,
                },
                "a",
                &env,
                window,
            );
            let card = menu.card.as_mut().unwrap();
            card.cursor = card.entries.iter().position(|row| row.action == "tags");
            menu.act("menuRight", None, "a", &env, window);
            assert_eq!(menu.flyout.as_ref().unwrap().card.built.lines.len(), 7);
        })
        .unwrap();
}

// Every quad one real draw put in the scene.
fn draw(cx: &mut TestAppContext, handle: WindowHandle<Host>) -> (Vec<Quad>, f32) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        (window.painted_quads(), window.scale_factor())
    })
    .unwrap()
}

fn squares(cx: &mut TestAppContext, handle: WindowHandle<Host>) -> Vec<Quad> {
    let (mut squares, scale) = draw(cx, handle);
    squares.retain(|quad| {
        (quad.bounds.size.width.0 / scale - 14.).abs() < 0.01
            && (quad.bounds.size.height.0 / scale - 14.).abs() < 0.01
    });
    squares.sort_by(|a, b| a.bounds.origin.y.0.total_cmp(&b.bounds.origin.y.0));
    squares
}

fn alpha(quad: &Quad) -> f32 {
    quad.background.as_solid().unwrap().a
}

#[gpui::test]
fn fx_uish_tag_flyout_model_reports_its_live_tag_states(cx: &mut TestAppContext) {
    use flea::{
        jsondoc::{self, Json},
        ui_logic_rail_places::tags::State,
    };
    let (handle, menu) = host(cx);
    open_tags(cx, handle, &menu);
    let catalog = I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    });
    let mut menu = menu.borrow_mut();
    let card = &mut menu.flyout.as_mut().unwrap().card;
    assert!(
        card.built.lines.len() >= 3,
        "the flyout builds a line per seeded tag"
    );
    for (line, state) in
        card.built
            .lines
            .iter_mut()
            .zip([State::Applied, State::Mixed, State::Absent])
    {
        line.tag_state = Some(state);
    }
    card.entries[0].tag_state = Some(State::Absent);
    let state = jsondoc::parse(&crate::g2::context_menus::ipc::joined::model(
        Some(card),
        &catalog,
    ))
    .unwrap();
    let rows = state.as_array().unwrap();
    assert!(
        rows.len() >= 3,
        "the model reports a row per tag line: {state:?}"
    );
    for (row, expected) in rows.iter().zip(["applied", "mixed", "absent"]) {
        assert_eq!(
            row.get("tagRow")
                .and_then(|tag| tag.get("state"))
                .and_then(Json::as_str),
            Some(expected)
        );
    }
}

// The menu card's own surface: the largest quad the draw holds.
fn card_alpha(cx: &mut TestAppContext, handle: WindowHandle<Host>) -> Option<f32> {
    let (quads, _) = draw(cx, handle);
    quads
        .iter()
        .max_by(|a, b| {
            let area = |q: &Quad| q.bounds.size.width.0 * q.bounds.size.height.0;
            area(a).total_cmp(&area(b))
        })
        .map(alpha)
}

fn out_cubic(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

#[gpui::test]
fn ey154_tag_rows_paint_distinct_shared_swatches_after_theme_changes(cx: &mut TestAppContext) {
    let (handle, menu) = host(cx);
    open_tags(cx, handle, &menu);
    // The swatches are judged once the 180 ms open fade has settled.
    draw(cx, handle);
    cx.executor().advance_clock(Duration::from_millis(200));
    let mut first_inks = Vec::new();
    for (background, surface, foreground, red, orange) in [
        ("#1c120d", "#150d0a", "#c7b59a", "#d65a4f", "#e19763"),
        ("#eff1f5", "#e6e9ef", "#4c4f69", "#d20f39", "#fe640b"),
        ("#181818", "#202020", "#d8d8d8", "#999999", "#bbbbbb"),
    ] {
        let roles = [
            ("red", red),
            ("orange", orange),
            ("yellow", "#dc9c36"),
            ("green", "#79ab55"),
            ("blue", "#509467"),
            ("magenta", "#8f7351"),
            ("dark_foreground", "#907e64"),
        ];
        let text = format!(
            "background='{background}'\ndark_background='{surface}'\nforeground='{foreground}'\n{}",
            roles
                .iter()
                .map(|(role, color)| format!("{role}='{color}'"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        cx.update(|cx| cx.set_global(Theme::from_text(&text)));
        let squares = squares(cx, handle);
        assert_eq!(squares.len(), 7, "every tag paints one solid square");
        let inks: Vec<_> = squares
            .iter()
            .map(|quad| quad.background.as_solid().unwrap())
            .collect();
        assert_ne!(
            inks[0], inks[1],
            "different tag rows must not share the accent ink"
        );
        let shared = theme_tags(&ThemeRoles {
            roles: roles
                .map(|(name, color)| (name.to_owned(), color.to_owned()))
                .to_vec(),
            background: background.into(),
            surface: surface.into(),
        });
        for (at, ink) in inks.iter().enumerate() {
            assert_eq!(
                Some(*ink),
                tag_mark(&shared, NAMES[at], Ground::Surface).fill,
                "the menu and rail must use the same tag ink for {}",
                NAMES[at]
            );
            assert_eq!(ink.a, 1., "tag squares stay solid on every theme");
        }
        first_inks.push(inks[0]);
    }
    assert!(
        first_inks.windows(2).all(|pair| pair[0] != pair[1]),
        "theme changes repaint the swatches"
    );
}

#[gpui::test]
fn ey154_tag_squares_draw_once_through_the_open_fade(cx: &mut TestAppContext) {
    let (handle, menu) = host(cx);
    open_tags(cx, handle, &menu);
    // The first frame stands at opacity 0: nothing is drawn, rather than four invisible strips per quad.
    let (quads, _) = draw(cx, handle);
    assert!(quads.is_empty(), "an unseen frame draws no quad");
    assert!(
        menu.borrow().opened(),
        "the menu owns the keys from its first frame"
    );
    cx.executor().advance_clock(Duration::from_millis(90));
    let during = squares(cx, handle);
    assert_eq!(during.len(), 7, "one square per tag during the fade");
    for quad in &during {
        assert!(
            (alpha(quad) - out_cubic(0.5)).abs() < 1e-3,
            "{}",
            alpha(quad)
        );
    }
    cx.executor().advance_clock(Duration::from_millis(110));
    let after = squares(cx, handle);
    assert_eq!(after.len(), 7, "one square per tag after the fade");
    assert!(after.iter().all(|quad| alpha(quad) == 1.));
}

#[gpui::test]
fn menu_close_fades_for_140ms_and_a_reopen_continues_from_there(cx: &mut TestAppContext) {
    let (handle, menu) = host(cx);
    open_tags(cx, handle, &menu);
    draw(cx, handle);
    cx.executor().advance_clock(Duration::from_millis(200));
    assert_eq!(card_alpha(cx, handle), Some(1.));
    menu.borrow_mut().close();
    assert!(!menu.borrow().opened() && menu.borrow().visible());
    assert_eq!(card_alpha(cx, handle), Some(1.));
    cx.executor().advance_clock(Duration::from_millis(70));
    let stood = 1. - out_cubic(0.5);
    let closing = card_alpha(cx, handle).unwrap();
    assert!((closing - stood).abs() < 1e-3, "{closing}");
    open_tags(cx, handle, &menu);
    let reopened = card_alpha(cx, handle).unwrap();
    assert!(
        (reopened - stood).abs() < 1e-3,
        "a reopen starts where the close stood: {reopened}"
    );
    cx.executor().advance_clock(Duration::from_millis(90));
    let rising = card_alpha(cx, handle).unwrap();
    assert!(
        (rising - (stood + (1. - stood) * out_cubic(0.5))).abs() < 1e-3,
        "{rising}"
    );
    menu.borrow_mut().close();
    card_alpha(cx, handle);
    cx.executor().advance_clock(Duration::from_millis(140));
    assert_eq!(
        card_alpha(cx, handle),
        None,
        "a closed card is gone after 140 ms"
    );
    assert!(!menu.borrow().visible());
}

#[gpui::test]
fn menu_fades_snap_under_reduced_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, menu) = host(cx);
    open_tags(cx, handle, &menu);
    let squares = squares(cx, handle);
    assert_eq!(squares.len(), 7);
    assert!(squares.iter().all(|quad| alpha(quad) == 1.));
    menu.borrow_mut().close();
    assert_eq!(card_alpha(cx, handle), None);
    assert!(!menu.borrow().visible());
}
