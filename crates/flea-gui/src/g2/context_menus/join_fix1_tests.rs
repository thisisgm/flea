//! FIX1 proofs for the round 1 review: the menu readers answer the shapes ui.sh parses, read off the painted rows.
use super::{
    ipc::{JoinedAdapter, MenuAdapter},
    join::Joined,
    join_tests::{open, read, ready},
    lines::Env,
    menu::{Face, Menu},
    shebang::ShebangReply,
};
use crate::{ipc::Adapter, list::Typeface};
use flea::{
    i18n::{I18n, Paths},
    jsondoc::{self, Json},
    ui_logic_context_menus::{fit::Rect, Entry, Text},
};
use gpui::{div, prelude::*, Context, Render, TestAppContext, VisualTestContext, Window};
use std::{cell::RefCell, rc::Rc};

const WIDE: Rect = Rect {
    x: 0.,
    y: 0.,
    w: 1100.,
    h: 700.,
};

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

fn catalog() -> I18n {
    I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

fn num(value: Option<&Json>) -> f64 {
    match value {
        Some(Json::Num(text)) => text.parse().unwrap(),
        Some(Json::Bool(_)) | None => 0.,
        other => panic!("not a number: {other:?}"),
    }
}

fn parts_sum(row: &Json) -> f64 {
    let wrap = row.get("hintWrap").and_then(Json::as_bool) == Some(true);
    (num(row.get("markLeft"))
        + num(row.get("slot"))
        + num(row.get("labelLeft"))
        + num(row.get("label"))
        + num(row.get("labelRight"))
        + if wrap { 0. } else { num(row.get("hint")) }
        + num(row.get("hintRight"))
        + num(row.get("status"))
        + num(row.get("chevron"))
        + num(row.get("chevronRight")))
    .ceil()
}

// A menu of rows with every trailing-part mix, opened in a real window and read through the joined adapter.
fn rows_with_every_trailing_mix(cx: &mut TestAppContext) -> (Rc<RefCell<Menu>>, String, String) {
    let (_, visual) = cx.add_window_view(|_, _| Blank);
    let menu = Rc::new(RefCell::new(Menu::default()));
    let catalog = catalog();
    let typeface = Typeface::omarchy_default();
    let mut word = Entry::row("word", Text::Data("Mount".into()), "copy");
    word.hint = Some(Text::Data("offline".into()));
    let mut wrapped = Entry::row("wrapped", Text::Data("Extract".into()), "copy");
    wrapped.hint = Some(Text::Data("No tool can read this archive".into()));
    wrapped.hint_wrap = true;
    wrapped.disabled = true;
    let mut chevron = Entry::row("chevron", Text::Data("Open with".into()), "copy");
    chevron.submenu = Some(vec![Entry::leaf(
        "text.desktop",
        Text::Data("Text editor".into()),
    )
    .with_key_hint("t")]);
    let mut key = Entry::row("key", Text::Data("Rename".into()), "copy");
    key.action = "rename".into();
    let rows = vec![
        Entry::row("plain", Text::Data("Open".into()), "copy"),
        key,
        word,
        wrapped,
        Entry::separator(),
        chevron,
    ];
    let env = Env {
        catalog: &catalog,
        preset: "default",
        key_hints: true,
        typeface: &typeface,
        inset: 4.,
    };
    visual.update(|window, _| {
        menu.borrow_mut()
            .open(Face::File, rows, (40., 40.), WIDE, "a", &env, window)
    });
    let hints = MenuAdapter {
        menu: &menu.borrow(),
        origin: (0., 0.),
    }
    .call("contextMenuHints", &[])
    .unwrap()
    .stdout;
    let joined = Joined {
        menu: Rc::clone(&menu),
        ..Joined::default()
    };
    let state = JoinedAdapter {
        joined: &joined,
        identity: String::new(),
        catalog: &catalog,
        field: None,
    }
    .call("menuState", &[])
    .unwrap()
    .stdout;
    (menu, hints, state)
}

#[gpui::test]
fn fix1_menu_state_entries_carry_submenu_leaves(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    ready(cx, &mut fixture);
    let state = jsondoc::parse(read(cx, handle, "menuState").stdout.trim()).unwrap();
    let entries = state.get("entries").and_then(Json::as_array).unwrap();
    let open_with = entries
        .iter()
        .find(|entry| entry.get("action").and_then(Json::as_str) == Some("openWith"))
        .expect("an Open with row");
    let leaves = open_with
        .get("submenu")
        .and_then(Json::as_array)
        .expect("ui-openwith-design.sh:18 and ui-menus.sh:187 iterate .submenu[]");
    assert!(!leaves.is_empty(), "the flyout leaves are listed");
    let first = leaves[0].get("label").and_then(Json::as_str).unwrap();
    assert_eq!(first, "Text editor", "a leaf's label is its resolved word");
    let model = jsondoc::parse(read(cx, handle, "contextMenuModel").stdout.trim()).unwrap();
    assert!(
        model
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row.get("action").and_then(Json::as_str) == Some("openWith"))
            .all(|row| row.get("submenu").and_then(Json::as_array).is_some()),
        "contextMenuModel carries the same leaves"
    );
    assert!(
        entries
            .iter()
            .filter(|entry| entry.get("action").and_then(Json::as_str) == Some("rename"))
            .all(|entry| entry.get("submenu").is_none()),
        "a row without leaves has no submenu member"
    );
}

#[gpui::test]
fn fix1_row_parts_sum_to_each_painted_row_for_every_trailing_mix(cx: &mut TestAppContext) {
    let (menu, _, state) = rows_with_every_trailing_mix(cx);
    let state = jsondoc::parse(state.trim()).unwrap();
    let rows = state.get("rows").and_then(Json::as_array).unwrap();
    let menu = menu.borrow();
    let lines = &menu.card.as_ref().unwrap().built.lines;
    assert_eq!(rows.len(), lines.len());
    for (index, (row, line)) in rows.iter().zip(lines).enumerate() {
        if line.separator {
            assert_eq!(row.get("separator").and_then(Json::as_bool), Some(true));
            continue;
        }
        let wrap = line.hint_wrap;
        let sum = parts_sum(row);
        assert!(
            (sum - f64::from(line.wanted)).abs() <= 1.,
            "row {index} parts sum {sum} against painted {} (wrap {wrap})",
            line.wanted
        );
        assert_eq!(row.get("hintWrap").and_then(Json::as_bool), Some(wrap));
        assert_eq!(row.get("hintRight"), Some(&Json::Num("0".into())));
        let mark = num(row.get("markLeft")) + num(row.get("slot")) + num(row.get("labelLeft"));
        assert_eq!(
            mark,
            if wrap { 14. + 18. + 9. } else { 15. + 18. + 9. },
            "the painter's label starts after its mark"
        );
    }
    let width = menu.card.as_ref().unwrap().frame.w as f64;
    let need = rows
        .iter()
        .filter(|row| row.get("separator").and_then(Json::as_bool) != Some(true))
        .map(parts_sum)
        .fold(0., f64::max);
    let expected = f64::from(flea::ui_logic_context_menus::fit::MENU_W)
        .max(need)
        .min(f64::from(WIDE.w))
        .round();
    assert!(
        (width - expected).abs() <= 1.,
        "need {need}, expected {expected}, frame {width}"
    );
}

#[gpui::test]
fn fix1_hints_read_the_drawn_word_before_the_key(cx: &mut TestAppContext) {
    let (_, hints, _) = rows_with_every_trailing_mix(cx);
    let hints: Vec<&str> = hints.trim_end().split('|').collect();
    assert_eq!(hints[2], "offline", "a state word is the drawn hint");
    assert!(!hints[1].is_empty(), "a row with no word shows its key");
    assert_eq!(hints[0], "", "a row with neither shows nothing");
}

#[gpui::test]
fn fix1_menu_state_answers_the_shebang_seam_null_until_filled(cx: &mut TestAppContext) {
    let fixture = open(cx);
    let state = jsondoc::parse(read(cx, fixture.handle, "menuState").stdout.trim()).unwrap();
    for key in ["pane", "shebangAsked", "shebangId", "shebangReply"] {
        assert_eq!(
            state.get(key),
            Some(&Json::Null),
            "{key} is null until a producer fills it"
        );
    }
    assert_eq!(state.get("shebangHas").and_then(Json::as_bool), Some(false));
    let catalog = catalog();
    let mut joined = Joined::default();
    joined.shebang.pane = Some("pane-B".into());
    joined.shebang.asked = Some("/fixture/notes.txt".into());
    joined.shebang.id = Some(3);
    joined.shebang.reply = Some(ShebangReply {
        pane: "pane-B".into(),
        path: "/fixture/notes.txt".into(),
        id: 3,
        has_shebang: false,
    });
    let state = JoinedAdapter {
        joined: &joined,
        identity: String::new(),
        catalog: &catalog,
        field: None,
    }
    .call("menuState", &[])
    .unwrap()
    .stdout;
    let state = jsondoc::parse(state.trim()).unwrap();
    assert_eq!(state.get("pane").and_then(Json::as_str), Some("pane-B"));
    assert_eq!(
        state.get("shebangAsked").and_then(Json::as_str),
        Some("/fixture/notes.txt")
    );
    assert_eq!(num(state.get("shebangId")), 3.);
    let reply = state.get("shebangReply").unwrap();
    assert_eq!(
        reply.get("path").and_then(Json::as_str),
        Some("/fixture/notes.txt")
    );
    assert_eq!(reply.get("hasShebang").and_then(Json::as_bool), Some(false));
    assert_eq!(num(reply.get("id")), 3.);
}

include!("fix2_width_proof.rs");
