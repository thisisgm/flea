//! The wrapped hint's fitted column is the retained text that a real menu element paints.
use super::{
    element::MenuElement,
    lines::Env,
    menu::{Face, Menu},
};
use crate::list::{bench::allocations, Typeface};
use flea::{
    i18n::{I18n, Paths},
    ui_logic_context_menus::{
        fit::{Rect, MENU_W},
        Entry, Text,
    },
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

fn refusal(wrap: bool) -> Entry {
    let mut row = Entry::row("extract", Text::Data("Extract".into()), "archive");
    row.hint = Some(Text::Data("No tool can read this archive".into()));
    row.hint_wrap = wrap;
    row.disabled = true;
    row
}

#[gpui::test]
fn fix2_wrapped_caption_is_fitted_then_painted_without_rebuild_or_allocation(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| cx.set_reduce_motion(true));
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
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    let handle = cx.add_window(move |_, _| Host(shared));
    let work = Rect {
        x: 0.,
        y: 0.,
        w: 1100.,
        h: 700.,
    };
    handle
        .update(cx, |_, window, _| {
            menu.borrow_mut().open(
                Face::File,
                vec![refusal(false)],
                (20., 20.),
                work,
                "a",
                &env,
                window,
            );
            assert!(menu.borrow().card.as_ref().unwrap().frame.w > MENU_W);
            menu.borrow_mut().open(
                Face::File,
                vec![refusal(true)],
                (20., 20.),
                work,
                "a",
                &env,
                window,
            );
        })
        .unwrap();
    let check = || {
        let held = menu.borrow();
        let card = held.card.as_ref().unwrap();
        let line = &card.built.lines[0];
        let caption = line
            .wrapped
            .as_ref()
            .expect("wrapping is prepared before publication");
        assert_eq!(card.frame.w, MENU_W);
        assert!(
            caption.lines.len() >= 2,
            "the full refusal is split across actual glyph runs"
        );
        assert!(caption.width < line.word.as_ref().unwrap().width());
        assert!(caption
            .lines
            .iter()
            .all(|line| line.width() <= caption.width + 1.));
        assert!(line.label_room(card.frame.w - 2.) >= line.label.as_ref().unwrap().width());
        assert!(
            line.key.is_none(),
            "the explicit wrapped reason occupies the hint slot"
        );
    };
    check();
    for _ in 0..2 {
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }
    {
        let held = menu.borrow();
        let caption = held.card.as_ref().unwrap().built.lines[0]
            .wrapped
            .as_ref()
            .unwrap();
        assert!(
            caption.lines.iter().all(|line| line.run.resolves() > 0),
            "every retained line reached the real painter"
        );
    }
    let builds = super::lines::builds();
    let before = allocations();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let spent = allocations() - before;
    assert!(
        spent < 64,
        "a retained wrapped frame stays at the existing paint floor: {spent}"
    );
    assert_eq!(super::lines::builds(), builds);
    check();
    handle
        .update(cx, |_, window, _| {
            let mut row = refusal(true);
            row.hint = Some(Text::Data("No reader is installed for this archive".into()));
            assert!(menu.borrow_mut().refresh(vec![row], &env, window));
            let held = menu.borrow();
            assert!(held.card.as_ref().unwrap().built.lines[0].wrapped.is_some());
            assert_eq!(held.card.as_ref().unwrap().frame.w, MENU_W);
        })
        .unwrap();
}
