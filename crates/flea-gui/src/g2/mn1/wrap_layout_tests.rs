//! Real-font trailing-column and actual paint bounds, including refresh and flyout builds.
use super::{
    element::MenuElement,
    ipc::JoinedAdapter,
    join::Joined,
    lines::Env,
    menu::{Face, Menu, Outcome},
};
use crate::{
    ipc::Adapter,
    list::{
        bench::{allocations, real_cx},
        Typeface,
    },
};
use flea::{
    i18n::{I18n, Paths},
    jsondoc::{self, Json},
    ui_logic_mn1::{fit::Rect, Entry, Text},
};
use gpui::{div, font, prelude::*, px, Context, Render, TestAppContext, Window, WindowHandle};
use std::{cell::RefCell, rc::Rc};

// Independent Theme.space tokens at the matrix's body sizes, including native EY-380 size20.
pub(super) struct FrozenTokens {
    pub minimum: f32,
    pub padding: f32,
    pub slot: f32,
    pub gap: f32,
    pub chevron: f32,
    pub row: f32,
}

pub(super) fn frozen_tokens(body: f32) -> FrozenTokens {
    let (minimum, padding, slot, gap, chevron, row) = match body as u32 {
        12 => (220., 12., 15., 8., 10., 32.),
        14 => (257., 14., 18., 9., 12., 37.),
        18 => (330., 18., 23., 12., 15., 48.),
        20 => (367., 20., 26., 13., 17., 53.),
        24 => (441., 24., 31., 15., 21., 63.),
        _ => panic!("unlisted frozen body size {body}"),
    };
    FrozenTokens {
        minimum,
        padding,
        slot,
        gap,
        chevron,
        row,
    }
}

struct Host(Rc<RefCell<Menu>>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(MenuElement::new(Rc::clone(&self.0), px(8.)))
    }
}

fn catalog() -> I18n {
    I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

fn refusal(chevron: bool) -> Entry {
    let mut row = Entry::row("extract", Text::Data("Extract".into()), "archive");
    row.hint = Some(Text::Key("menu.extract.no_tool"));
    row.hint_wrap = true;
    row.disabled = true;
    if chevron {
        row.submenu = Some(vec![Entry::leaf("reader", Text::Data("Reader".into()))]);
    }
    row
}

fn draw(cx: &mut TestAppContext, handle: WindowHandle<Host>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn check_width(menu: &Rc<RefCell<Menu>>, catalog: &I18n, work_width: f32) {
    let joined = Joined {
        menu: Rc::clone(menu),
        ..Joined::default()
    };
    let raw = JoinedAdapter {
        joined: &joined,
        identity: "a".into(),
        catalog,
    }
    .call("menuState", &[])
    .unwrap()
    .stdout;
    let state = jsondoc::parse(raw.trim()).unwrap();
    let rows = state.get("rows").and_then(Json::as_array).unwrap();
    let needed = rows
        .iter()
        .map(|row| {
            [
                "markLeft",
                "slot",
                "labelLeft",
                "label",
                "labelRight",
                "hintRight",
                "status",
                "chevron",
                "chevronRight",
            ]
            .iter()
            .map(|key| match row.get(key).unwrap() {
                Json::Num(value) => value.parse::<f64>().unwrap(),
                _ => panic!("numeric part {key}"),
            })
            .sum::<f64>()
            .ceil()
        })
        .fold(0., f64::max);
    let tokens = frozen_tokens(f32::from(
        menu.borrow().card.as_ref().unwrap().built.typeface.body,
    ));
    let expected = f64::from(tokens.minimum)
        .max(needed)
        .min(f64::from(work_width))
        .round();
    let observed = f64::from(menu.borrow().card.as_ref().unwrap().frame.w);
    assert!((observed - expected).abs() <= 1.);
    assert!(
        observed >= f64::from(work_width)
            || !rows
                .iter()
                .any(|row| matches!(row.get("labelTruncated"), Some(Json::Bool(true))))
    );
}

fn check_painted(card: &super::menu::Card, row: usize, vertical: bool) {
    let line = &card.built.lines[row];
    let word = line.word.as_ref().unwrap();
    let caption = line.wrapped.as_ref().unwrap();
    let painted = caption
        .painted_bounds()
        .expect("the actual painter must receive this caption");
    let tokens = frozen_tokens(f32::from(card.built.typeface.body));
    let column = if line.chevron {
        tokens.chevron + tokens.gap
    } else {
        0.
    };
    let right = card.frame.x + card.frame.w - tokens.padding - column;
    assert!((f32::from(painted.origin.x + painted.size.width) - right).abs() < 0.001);
    assert!((f32::from(painted.origin.x) - (right - caption.width)).abs() < 0.001);
    assert!((f32::from(painted.size.width) - caption.width).abs() < 0.001);
    if caption.width > 0. {
        let retained: String = caption
            .lines
            .iter()
            .flat_map(|part| part.text.split_whitespace())
            .collect();
        assert_eq!(retained, word.text.split_whitespace().collect::<String>());
        assert!(caption.lines.iter().all(|part| part.run.resolves() > 0));
    }
    if vertical {
        assert_eq!(
            line.label_room(card.frame.w - 2.).to_bits(),
            line.label.as_ref().unwrap().width().to_bits()
        );
        assert_eq!(caption.lines.len(), 2);
        let top = card.row_screen_top(row);
        assert!(f32::from(painted.origin.y) >= top);
        assert!(f32::from(painted.origin.y + painted.size.height) <= top + tokens.row);
    }
}

#[test]
fn fix3_real_font_columns_with_and_without_submenu_follow_the_frozen_recipe() {
    let mut cx = real_cx();
    cx.update(|cx| cx.set_reduce_motion(true));
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    let handle = cx.add_window(move |_, _| Host(shared));
    let catalog = catalog();
    let mut cases = 0;
    for family in ["JetBrains Mono", "DejaVu Sans"] {
        for body in [12., 14., 18., 20., 24.] {
            let typeface = Typeface {
                font: font(family),
                body: px(body),
                caption: px(body * 12. / 14.),
                ..Typeface::omarchy_default()
            };
            let env = Env {
                catalog: &catalog,
                preset: "default",
                key_hints: true,
                typeface: &typeface,
                inset: 4.,
            };
            for work_width in [1100., 257., 220., 180., 160., 120.] {
                for chevron in [false, true] {
                    handle
                        .update(&mut cx, |_, window, _| {
                            menu.borrow_mut().open(
                                Face::File,
                                vec![refusal(chevron)],
                                (10., 10.),
                                Rect {
                                    x: 0.,
                                    y: 0.,
                                    w: work_width,
                                    h: 700.,
                                },
                                "a",
                                &env,
                                window,
                            );
                            let held = menu.borrow();
                            let card = held.card.as_ref().unwrap();
                            let line = &card.built.lines[0];
                            let word = line.word.as_ref().unwrap();
                            let tokens = frozen_tokens(body);
                            let column = if chevron {
                                tokens.chevron + tokens.gap
                            } else {
                                0.
                            };
                            let frozen = (card.frame.w
                                - 2. * tokens.padding
                                - tokens.slot
                                - 2. * tokens.gap
                                - line.label.as_ref().unwrap().width()
                                - column)
                                .max(0.)
                                .min(word.width());
                            let caption = line.wrapped.as_ref().unwrap();
                            assert!((caption.width - frozen).abs() < 0.001);
                            assert_eq!(card.frame.w, tokens.minimum.min(work_width));
                            assert_eq!(line.has_trailing_column(), chevron);
                            assert!(line.key.is_none());
                            drop(held);
                            let mut changed = refusal(chevron);
                            changed.disabled = false;
                            assert!(menu.borrow_mut().refresh(vec![changed], &env, window));
                            let held = menu.borrow();
                            let refreshed = held.card.as_ref().unwrap();
                            assert_eq!(refreshed.frame.w, tokens.minimum.min(work_width));
                            let caption = refreshed.built.lines[0].wrapped.as_ref().unwrap();
                            assert!((caption.width - frozen).abs() < 0.001);
                        })
                        .unwrap();
                    check_width(&menu, &catalog, work_width);
                    draw(&mut cx, handle);
                    check_painted(menu.borrow().card.as_ref().unwrap(), 0, false);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 120);
    println!("FIX3 real-font column/paint matrix: {cases} cases passed");
}

#[test]
fn fix3_real_font_default_refusals_paint_whole_at_edges_refresh_and_flyout() {
    let mut cx = real_cx();
    cx.update(|cx| cx.set_reduce_motion(true));
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    let handle = cx.add_window(move |_, _| Host(shared));
    let catalog = catalog();
    let typeface = Typeface {
        font: font("JetBrains Mono"),
        ..Typeface::omarchy_default()
    };
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
        w: 1100.,
        h: 700.,
    };
    handle
        .update(&mut cx, |_, window, _| {
            menu.borrow_mut().open(
                Face::File,
                vec![refusal(false), refusal(false), refusal(false)],
                (10., 10.),
                work,
                "a",
                &env,
                window,
            );
        })
        .unwrap();
    draw(&mut cx, handle);
    draw(&mut cx, handle);
    {
        let held = menu.borrow();
        let card = held.card.as_ref().unwrap();
        assert_eq!(card.frame.w, 257.);
        for row in 0..3 {
            check_painted(card, row, true);
        }
    }
    let before = allocations();
    draw(&mut cx, handle);
    let spent = allocations() - before;
    assert!(
        spent < 64,
        "retained real-font paint exceeds the existing allocation floor: {spent}"
    );
    handle
        .update(&mut cx, |_, window, _| {
            let mut changed = refusal(false);
            changed.disabled = false;
            assert!(menu.borrow_mut().refresh(vec![changed], &env, window));
        })
        .unwrap();
    draw(&mut cx, handle);
    check_painted(menu.borrow().card.as_ref().unwrap(), 0, true);
    handle
        .update(&mut cx, |_, window, _| {
            let mut parent = Entry::row("openWith", Text::Data("Open With".into()), "app-window");
            parent.submenu = Some(vec![refusal(false)]);
            menu.borrow_mut().open(
                Face::File,
                vec![parent],
                (10., 10.),
                work,
                "a",
                &env,
                window,
            );
            assert_eq!(
                menu.borrow_mut().act("menuRight", None, "a", &env, window),
                Outcome::Redraw
            );
            assert!(menu.borrow().flyout.is_some());
        })
        .unwrap();
    draw(&mut cx, handle);
    check_painted(&menu.borrow().flyout.as_ref().unwrap().card, 0, true);
    println!("FIX3 real-font default: first/middle/last, refresh and flyout paint complete; retained allocations={spent}");
}
