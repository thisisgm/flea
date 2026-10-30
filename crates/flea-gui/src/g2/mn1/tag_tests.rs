//! The tags flyout's actual paint uses each tag's surface ink, including on low-chroma palettes.
use super::MenuElement;
use crate::{
    g2::{
        mn1::{
            lines::Env,
            menu::{Face, Menu},
        },
        pl4::{
            content::{tag_mark, theme_tags, Ground},
            seams::ThemeRoles,
        },
    },
    list::Typeface,
    palette::Theme,
};
use flea::{
    i18n::{I18n, Paths},
    ui_logic_mn1::{
        build::{listing_entries, Context as MenuContext},
        fit::Rect,
        Entry, Text,
    },
    ui_logic_pl4::tags::NAMES,
};
use gpui::{div, prelude::*, px, Context, Render, TestAppContext, Window};
use std::{cell::RefCell, rc::Rc};

struct Host(Rc<RefCell<Menu>>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(MenuElement::new(Rc::clone(&self.0), px(8.)))
    }
}

#[gpui::test]
fn ey154_tag_rows_paint_distinct_shared_swatches_after_theme_changes(cx: &mut TestAppContext) {
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    let handle = cx.add_window(move |_, _| Host(shared));
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
        let (mut squares, scale) = cx
            .update_window(handle.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                (window.painted_quads(), window.scale_factor())
            })
            .unwrap();
        squares.retain(|quad| {
            (quad.bounds.size.width.0 / scale - 14.).abs() < 0.01
                && (quad.bounds.size.height.0 / scale - 14.).abs() < 0.01
        });
        squares.sort_by(|a, b| a.bounds.origin.y.0.total_cmp(&b.bounds.origin.y.0));
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
