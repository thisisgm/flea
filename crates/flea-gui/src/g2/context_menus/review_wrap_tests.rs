//! Independent FIX2 geometry probes using the platform text system.
use super::{
    element::MenuElement,
    ipc::JoinedAdapter,
    join::Joined,
    lines::Env,
    menu::{Face, Menu},
};
use crate::{
    ipc::Adapter,
    list::{bench::real_cx, Typeface},
};
use flea::{
    i18n::{I18n, Paths},
    jsondoc::{self, Json},
    ui_logic_context_menus::{
        fit::{self, Rect},
        Entry, Text,
    },
};
use gpui::{div, font, prelude::*, px, Context, Render, Window};
use std::{cell::RefCell, rc::Rc};

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
fn number(row: &Json, key: &str) -> f64 {
    match row.get(key).unwrap() {
        Json::Num(value) => value.parse().unwrap(),
        _ => panic!("numeric {key}"),
    }
}
#[test]
fn rev_u06_r3_real_font_wrapped_geometry_matrix() {
    let mut cx = real_cx();
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = Rc::clone(&menu);
    let handle = cx.add_window(move |_, _| Host(shared));
    let catalog = catalog();
    let mut overflows = Vec::new();
    for body in [14., 18., 20., 24.] {
        for family in ["JetBrains Mono", "DejaVu Sans"] {
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
                handle.update(&mut cx, |_, window, _| {
                    let mut row = Entry::row("extract", Text::Data("Extract".into()), "archive");
                    row.hint = Some(Text::Key("menu.extract.no_tool"));
                    row.hint_wrap = true;
                    row.disabled = true;
                    menu.borrow_mut().open(Face::File, vec![row], (10., 10.), Rect { x: 0., y: 0., w: work_width, h: 700. }, "a", &env, window);
                    let held = menu.borrow();
                    let card = held.card.as_ref().unwrap();
                    let line = &card.built.lines[0];
                    let caption = line.wrapped.as_ref().unwrap();
                    let natural = line.word.as_ref().unwrap().width();
                    let metrics = line.word.as_ref().unwrap().run.line();
                    let height = f32::from(metrics.ascent + metrics.descent) * caption.lines.len() as f32;
                    let rendered: Vec<_> = caption.lines.iter().map(|part| (part.text.as_str(), part.width())).collect();
                    println!("R3_WRAP font={family:?} body={body} area={work_width} frame={} label={} caption={} natural={natural} rows={} height={height} glyphs={rendered:?}", card.frame.w, line.label.as_ref().unwrap().width(), caption.width, caption.lines.len());
                    assert!(caption.width >= 0. && caption.width <= natural);
                    assert!(card.frame.w <= work_width);
                    assert!(line.key.is_none());
                    if caption.width >= 20. && caption.lines.iter().any(|part| part.width() > caption.width + 1.) {
                        overflows.push((family, body, work_width, caption.width, rendered.iter().map(|(s,w)| (s.to_string(),*w)).collect::<Vec<_>>()));
                    }
                    if caption.width > 0. {
                        assert_eq!(caption.lines.iter().map(|part| part.text.split_whitespace().collect::<String>()).collect::<String>(), "Noarchivetoolisinstalled");
                    }
                    drop(held);
                    let joined = Joined { menu: Rc::clone(&menu), ..Joined::default() };
                    let state = JoinedAdapter { joined: &joined, identity: "a".into(), catalog: &catalog, field: None }.call("menuState", &[]).unwrap().stdout;
                    let doc = jsondoc::parse(state.trim()).unwrap();
                    let row = &doc.get("rows").and_then(Json::as_array).unwrap()[0];
                    let need = ["markLeft","slot","labelLeft","label","labelRight","hintRight","status","chevron","chevronRight"].iter().map(|key| number(row,key)).sum::<f64>().ceil();
                    let expected = f64::from(super::wrap_layout_tests::frozen_tokens(body).minimum).max(need).min(f64::from(work_width)).round();
                    assert!((f64::from(menu.borrow().card.as_ref().unwrap().frame.w) - expected).abs() <= 1.);
                }).unwrap();
                cx.update_window(handle.into(), |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .unwrap();
            }
        }
    }
    assert!(
        overflows.is_empty(),
        "shaped caption exceeds its allocated width: {overflows:?}"
    );
}

#[test]
fn rev_u06_r3_default_wrapped_refusal_matches_frozen_room_and_fits_row() {
    let mut cx = real_cx();
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
    handle.update(&mut cx, |_, window, _| {
        let mut row = Entry::row("extract", Text::Data("Extract".into()), "archive");
        row.hint = Some(Text::Key("menu.extract.no_tool"));
        row.hint_wrap = true;
        row.disabled = true;
        menu.borrow_mut().open(Face::File, vec![row], (10.,10.), Rect { x:0., y:0., w:1100., h:700. }, "a", &env, window);
        let held = menu.borrow();
        let card = held.card.as_ref().unwrap();
        let line = &card.built.lines[0];
        let word = line.word.as_ref().unwrap();
        let actual = line.wrapped.as_ref().unwrap();
        let label = line.label.as_ref().unwrap().width();
        // ui/MenuRow.qml:257-260 takes the row width less padding, slot, two gaps and the label.
        let frozen_room = card.frame.w - 2. * fit::PAD_X - fit::SLOT - 2. * fit::GAP - label;
        let frozen = super::wrap::Caption::new(word, frozen_room, &env, window);
        let metrics = word.run.line();
        let height = f32::from(metrics.ascent + metrics.descent);
        let actual_height = height * actual.lines.len() as f32;
        let frozen_height = height * frozen.lines.len() as f32;
        println!("R3_DEFAULT actual_width={} actual_lines={} actual_height={actual_height} frozen_width={} frozen_lines={} frozen_height={frozen_height} fixed_row={}", actual.width, actual.lines.len(), frozen.width, frozen.lines.len(), fit::ROW);
        println!("R3_DEFAULT_ACTUAL {:?}", actual.lines.iter().map(|s| &s.text).collect::<Vec<_>>());
        println!("R3_DEFAULT_FROZEN {:?}", frozen.lines.iter().map(|s| &s.text).collect::<Vec<_>>());
        assert!(frozen_height <= fit::ROW as f32);
        assert!(actual_height <= fit::ROW as f32, "default refusal exceeds its fixed row because FIX2 reserves a chevron and extra gap that the frozen wrapped-caption width does not subtract");
    }).unwrap();
}

#[test]
fn rev_u06_r4_default_wrapped_refusal_is_not_reported_as_elided() {
    let mut cx = real_cx();
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
    handle
        .update(&mut cx, |_, window, _| {
            let mut row = Entry::row("extract", Text::Data("Extract".into()), "archive");
            row.hint = Some(Text::Key("menu.extract.no_tool"));
            row.hint_wrap = true;
            row.disabled = true;
            menu.borrow_mut().open(
                Face::File,
                vec![row],
                (10., 10.),
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
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let held = menu.borrow();
    let card = held.card.as_ref().unwrap();
    assert_eq!(card.frame.w, 257.);
    let line = &card.built.lines[0];
    let label = line.label.as_ref().unwrap().width();
    let room = line.label_room(card.frame.w - 2.);
    println!(
        "R4_LABEL label={label:?} room={room:?} label_bits={:08x} room_bits={:08x} caption={}",
        label.to_bits(),
        room.to_bits(),
        line.word_width()
    );
    drop(held);
    let joined = Joined {
        menu: Rc::clone(&menu),
        ..Joined::default()
    };
    let state = JoinedAdapter {
        joined: &joined,
        identity: "a".into(),
        catalog: &catalog,
        field: None,
    }
    .call("menuState", &[])
    .unwrap()
    .stdout;
    println!("R4_STATE {state}");
    let doc = jsondoc::parse(state.trim()).unwrap();
    let rows = doc.get("rows").and_then(Json::as_array).unwrap();
    assert!(!rows.iter().any(|row| matches!(row.get("labelTruncated"), Some(Json::Bool(true)))), "tests/ui-card-layout.sh:234-236 rejects labelTruncated=true when frame 257 is below work area 1100");
}
