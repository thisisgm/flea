//! #214: a label the card cannot hold whole ends in an ellipsis inside its room, and the row keeps its whole text.
use super::{
    lines::Env,
    menu::{Face, Menu},
};
use crate::list::Typeface;
use flea::{
    i18n::{I18n, Paths},
    ui_logic_mn1::{fit::Rect, Entry, Text},
};
use gpui::{div, prelude::*, Context, Render, TestAppContext, Window};

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

const LONG: &str = "Open With Image Viewer for Very Long Application Names Everywhere";

fn open_in(width: f32, cx: &mut TestAppContext) -> Menu {
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
    let work = Rect {
        x: 0.,
        y: 0.,
        w: width,
        h: 700.,
    };
    let handle = cx.add_window(|_, _| Host);
    let mut menu = Menu::default();
    handle
        .update(cx, |_, window, _| {
            menu.open(
                Face::File,
                vec![
                    Entry::row("open", Text::Data(LONG.into()), "open"),
                    Entry::row("copy", Text::Data("Copy".into()), "copy"),
                ],
                (20., 20.),
                work,
                "a",
                &env,
                window,
            );
        })
        .unwrap();
    menu
}

#[gpui::test]
fn a_label_wider_than_a_narrow_card_ends_in_an_ellipsis_inside_its_room(cx: &mut TestAppContext) {
    let menu = open_in(200., cx);
    let card = menu.card.as_ref().unwrap();
    let line = &card.built.lines[0];
    let room = line.label_room(card.frame.w - 2.);
    let whole = line.label.as_ref().unwrap();
    assert!(
        whole.width() > room,
        "the fixture label must not fit a 200px card"
    );
    let painted = line.painted_label().unwrap();
    assert!(
        painted.width() <= room + 0.5,
        "the painted label {} is {} wide in a {room} room",
        painted.text,
        painted.width()
    );
    assert!(painted.text.ends_with('\u{2026}'), "{}", painted.text);
    assert!(LONG.starts_with(painted.text.trim_end_matches('\u{2026}')));
    assert_eq!(
        whole.text, LONG,
        "the row keeps its whole label for its readers"
    );
    let short = &card.built.lines[1];
    assert!(short.elided.is_none(), "a label that fits is painted whole");
    assert_eq!(short.painted_label().unwrap().text, "Copy");
}

#[gpui::test]
fn a_card_wide_enough_paints_every_label_whole(cx: &mut TestAppContext) {
    let menu = open_in(1100., cx);
    let card = menu.card.as_ref().unwrap();
    assert!(card.built.lines.iter().all(|line| line.elided.is_none()));
    assert_eq!(card.built.lines[0].painted_label().unwrap().text, LONG);
}
