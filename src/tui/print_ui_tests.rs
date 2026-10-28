use super::{
    action_key_tests::drain,
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    print,
    render::{
        self,
        frame_tests::{entry, grid, model},
    },
    theme::Theme,
    wire::{self, count, text, Wire},
};
use crate::jsondoc::{self, Json};
use std::time::Duration;
fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
fn answer(m: &mut Model, w: &mut Wire, line: &str) {
    m.receive(jsondoc::parse(line).unwrap(), w).unwrap();
}
fn ready(m: &mut Model, w: &mut Wire) {
    let id = m.action_id;
    answer(
        m,
        w,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true}}"#),
    );
    answer(
        m,
        w,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"print","id":{id},"ok":true,"paths":["{}"]}}"#,
            m.current_path().unwrap().display()
        ),
    );
    let id = m.printing.card.as_ref().unwrap().query.unwrap();
    answer(
        m,
        w,
        &format!(
            r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Brother","default":true}},{{"name":"Other","default":false}}],"printer":"Brother","duplex":true,"twoSided":false,"color":true,"grayscale":false,"setupArgv":[]}}"#
        ),
    );
    drain(w);
}

#[test]
fn tui_print_all_printable_kinds_dispatch_and_other_kinds_hide() {
    let (mut w, reader) = wire::echo_wire();
    for (name, paged) in [
        ("report.pdf", true),
        ("garden.docx", true),
        ("sheet.ods", true),
        ("talk.pptx", true),
        ("photo.png", false),
        ("notes.txt", false),
        ("code.rs", false),
        ("config.json", false),
    ] {
        let mut m = model();
        m.rows.insert(7, entry(name, false, 20));
        press(&mut m, &mut w, Key::character('P', "ctrl"));
        ready(&mut m, &mut w);
        assert_eq!(m.printing.card.as_ref().unwrap().paged, paged, "{name}");
        press(&mut m, &mut w, Key::named("Return", ""));
        let sent = drain(&mut w);
        let request = sent.iter().find(|v| text(v, "c") == "print").unwrap();
        assert_eq!(count(request, "menuId"), m.action_id);
        assert_eq!(count(request, "copies"), 1);
        assert_eq!(
            request.get("paths").and_then(Json::as_array).unwrap().len(),
            1
        );
    }
    for name in ["archive.zip", "movie.mp4", "model.stl"] {
        let mut m = model();
        m.rows.insert(7, entry(name, false, 20));
        assert!(!print::offered(&m), "{name}");
        press(&mut m, &mut w, Key::character('P', "ctrl"));
        assert!(m.printing.card.is_none() && !m.error.is_empty());
    }
    wire::finish(w, reader);
}

#[test]
fn tui_print_file_menu_reuses_exact_multi_selection_snapshot() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    m.selected = [7, 8].into_iter().collect();
    m.cursor = 0;
    m.remember_selection();
    m.menu_view.hidden.push("openwith".into());
    press(&mut m, &mut w, Key::character('m', ""));
    let sent = drain(&mut w);
    let snapshot = sent.iter().find(|v| text(v, "op") == "snapshot").unwrap();
    assert_eq!(
        snapshot.get("rows").and_then(Json::as_array).unwrap().len(),
        2,
        "unselected cursor cannot enter Print's batch"
    );
    let id = m.action_id;
    answer(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true}}"#),
    );
    let rows = render::menu_layout::entries(&m, &Map::load());
    m.menu_cursor = rows
        .iter()
        .position(|r| r.as_ref().is_some_and(|r| r.id == "print"))
        .unwrap();
    let row = rows[m.menu_cursor].as_ref().unwrap();
    assert_eq!(row.label, "Print 2 Items…");
    press(&mut m, &mut w, Key::named("Return", ""));
    let sent = drain(&mut w);
    assert!(!m.menu);
    assert!(sent.iter().any(|v| text(v, "op") == "validate"
        && text(v, "action") == "print"
        && count(v, "id") == id));
    assert!(
        !sent.iter().any(|v| text(v, "op") == "snapshot"),
        "reuse held identities"
    );
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"menuaction","op":"validate","action":"print","id":{id},"ok":true,"paths":["/fx/v/work/notes.txt","/fx/v/work/todo.md"]}}"#
        ),
    );
    assert_eq!(m.printing.card.as_ref().unwrap().paths.len(), 2);
    wire::finish(w, reader);
}

#[test]
fn tui_print_focused_controls_and_pending_queries_never_submit() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    press(&mut m, &mut w, Key::character('P', "ctrl"));
    ready(&mut m, &mut w);
    press(&mut m, &mut w, Key::named("Tab", ""));
    press(&mut m, &mut w, Key::named("Tab", ""));
    press(&mut m, &mut w, Key::named("Return", ""));
    assert!(m.printing.card.as_ref().unwrap().dropdown);
    let displayed = grid(
        &render::frame(
            &m,
            &Theme::from_text(""),
            &Map::load(),
            130,
            31,
            Duration::ZERO,
        ),
        130,
        31,
    );
    let row = displayed
        .iter()
        .position(|line| line.contains("Other"))
        .unwrap();
    let column = displayed[row].chars().position(|c| c == 'O').unwrap();
    let mut wheel = Key::named("Pointer", "");
    wheel.pointer = Some(super::input::Pointer {
        x: column + 1,
        y: row + 1,
        button: super::input::WHEEL_DOWN,
        motion: false,
        released: false,
    });
    press(&mut m, &mut w, wheel);
    assert_eq!(m.printing.card.as_ref().unwrap().choice, 1);
    assert_eq!(m.printing.card.as_ref().unwrap().scroll, 0);
    assert!(!drain(&mut w).iter().any(|v| text(v, "c") == "print"));
    press(&mut m, &mut w, Key::named("Escape", ""));
    let old = m.printing.card.as_ref().unwrap().printer.clone();
    press(&mut m, &mut w, Key::named("Right", ""));
    let current = m.printing.card.as_ref().unwrap().query.unwrap();
    print::submit(&mut m, &mut w).unwrap();
    assert!(!drain(&mut w).iter().any(|v| text(v, "c") == "print"));
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printoptions","id":{},"ok":false,"err":"stale failure"}}"#,
            current - 1
        ),
    );
    assert_eq!(m.printing.card.as_ref().unwrap().printer, old);
    assert!(m.error.is_empty());
    press(&mut m, &mut w, Key::named("Escape", ""));
    assert!(m.printing.card.is_none());
    wire::finish(w, reader);
}

#[test]
fn tui_print_overlay_clears_picture_and_keeps_listing_in_scrim() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    press(&mut m, &mut w, Key::character('P', "ctrl"));
    ready(&mut m, &mut w);
    let theme = Theme::from_text(render::cards::tests::PALETTE);
    let out = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    let rows = grid(&out, 130, 31);
    let inks = render::cards::tests::inks(&out, 130, 31);
    assert!(
        rows[9].contains("notes.txt"),
        "held listing remains drawn: {}",
        rows[9]
    );
    assert!(!rows[9].contains('▎'));
    assert_eq!(
        inks[9][32].0,
        render::cards::tests::role(&theme.dim.as_ref().unwrap().foreground)
    );
    let mut graphics = super::graphics::Graphics::new();
    graphics.bytes = b"held picture".to_vec();
    m.image_file = Some("/old.png".into());
    super::stage::step(
        &mut m,
        &mut graphics,
        &mut super::stage::Shown::default(),
        &mut w,
        (130, 31),
        true,
        &mut String::new(),
    )
    .unwrap();
    assert!(graphics.bytes.is_empty() && m.image_file.is_none());
    wire::finish(w, reader);
}
