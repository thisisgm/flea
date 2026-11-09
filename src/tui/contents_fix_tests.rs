use super::{
    action_key_tests::{drain, listing},
    contents,
    contents_process_tests::{find, until, Root},
    contents_tests::{enter, land, press, receive},
    graphics::{Graphics, Protocol},
    input::{Key, Pointer, WHEEL_DOWN},
    model::Model,
    render,
    stage::{self, Shown},
    theme::Theme,
    wire::{self, count, text},
};
use crate::jsondoc::Json;
use flea::ui_logic_sr1::{scope_label, ContentsMatch, ContentsMatches, Kind, Scope, Session};
use std::{io::Write, process::Stdio, time::Duration};

pub(super) fn names(m: &mut Model, wire: &mut wire::Wire) {
    let keys = super::input::Decoder::default().feed(b"\x07\x07", false);
    for key in keys {
        press(m, wire, key);
    }
    m.editor.as_mut().unwrap().replace("needle-".into());
    press(m, wire, Key::named("Enter", ""));
}
fn settled(m: &mut Model, wire: &mut wire::Wire) {
    loop {
        let line = until(m, wire, "searched");
        if !line.get("walking").and_then(Json::as_bool).unwrap_or(false) {
            break;
        }
    }
    assert!(!m.find.waiting, "the real names envelope must land");
}
#[test]
fn tui_contents_fix1_names_real_backend_contract() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Root::new();
    for i in 0..7 {
        std::fs::write(root.0.join(format!("needle-{i}.txt")), "needle body\n").unwrap();
    }
    let mut wire = root.backend(false);
    let mut m = Model::new(root.0.clone(), &Json::Null);
    m.resize_body(4);
    names(&mut m, &mut wire);
    settled(&mut m, &mut wire);
    assert_eq!(m.total, 7);
    assert_eq!(contents::row_height(&m), 1);
    assert_eq!(m.rows.len(), 4);
    m.cursor = 6;
    m.window(&mut wire).unwrap();
    loop {
        let page = until(&mut m, &mut wire, "rows");
        if count(&page, "start") == 3 {
            assert!(page.get("gen").is_none(), "real names windows are untagged");
            break;
        }
    }
    assert_eq!(m.current_path(), Some(root.0.join("needle-6.txt")));
    find(&mut m, &mut wire, "needle").unwrap();
    settled(&mut m, &mut wire);
    assert_eq!(contents::row_height(&m), 2);
    assert!(!m.find.matches.is_empty());
    names(&mut m, &mut wire);
    settled(&mut m, &mut wire);
    assert_eq!(contents::row_height(&m), 1);
    assert!(m.find.matches.is_empty());
    assert_eq!(m.rows.len(), 4);
    assert!(m.current_path().is_some());
}
fn mouse(m: &mut Model, wire: &mut wire::Wire, y: usize, button: u32) {
    m.last_click = None;
    let (left, _, _) = render::panes(120, m.preview_visible);
    press(
        m,
        wire,
        Key {
            name: "Mouse".into(),
            text: String::new(),
            mods: String::new(),
            pointer: Some(Pointer {
                x: left + 4,
                y,
                button,
                released: false,
                motion: false,
            }),
        },
    );
}
#[test]
fn tui_contents_fix1_physical_geometry_and_file_capacity() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    m.columns = 120;
    m.resize_body(render::body_height(16));
    let preview_rows = super::pane::rows(&m);
    let gen = enter(&mut m, &mut wire, "needle");
    land(&mut m, &mut wire, gen, 51, &["a", "b", "c", "d", "e", "f"]);
    for y in [render::BODY_ROW + 6, render::BODY_ROW + 7] {
        m.cursor = 0;
        mouse(&mut m, &mut wire, y, 0);
        assert_eq!(m.cursor, 3, "both lower-half rows select the fourth file");
        mouse(&mut m, &mut wire, y, WHEEL_DOWN);
        assert_eq!(m.cursor, 4, "wheel works over the lower half");
    }
    assert_eq!(m.height, 12, "physical body height stays physical");
    assert_eq!(super::pane::rows(&m), preview_rows);
    m.quicklook = true;
    assert_eq!(super::pane::rows(&m), super::look::text_rows(12));
    let card = super::look::card(&m, &Theme::from_text(""), 120);
    assert!(card.contains(&format!("\x1b[{};", render::BODY_ROW + 11)));
    m.quicklook = false;
    m.cursor = 5;
    m.window(&mut wire).unwrap();
    let sent = drain(&mut wire);
    let page = sent
        .iter()
        .rev()
        .find(|v| text(v, "c") == "window")
        .unwrap();
    assert_eq!(count(page, "count"), 6);
    wire::finish(wire, reader);
}
#[test]
fn tui_contents_fix1_failed_navigation_keeps_refusal_identity() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    let gen = enter(&mut m, &mut wire, "needle");
    receive(
        &mut m,
        &mut wire,
        &ContentsMatches {
            gen: gen as u64,
            listing: 51,
            start: 0,
            matches: vec![ContentsMatch {
                row: 0,
                path: "bad\u{fffd}.txt\u{0}626164ff2e747874".into(),
                path_bytes: b"bad\xff.txt".to_vec(),
                line_no: 1,
                line: "needle".into(),
                operable: false,
            }],
        }
        .line(),
    );
    super::contents_tests::listed(&mut m, &mut wire, gen, 51, 1);
    super::contents_tests::rows(
        &mut m,
        &mut wire,
        gen,
        51,
        0,
        &["bad\u{fffd}.txt\u{0}626164ff2e747874"],
    );
    assert!(m.current_path().is_none() && contents::guarded(&m));
    m.open("/absent-tui-08-fixture".into(), &mut wire).unwrap();
    receive(
        &mut m,
        &mut wire,
        r#"{"t":"error","where":"scan","path":"/absent-tui-08-fixture","msg":"no such folder"}"#,
    );
    assert!(m.pending.is_none());
    assert!(
        m.current_path().is_none(),
        "failed navigation must retain the displayed refusal"
    );
    assert!(contents::guarded(&m));
    assert_eq!(m.find.matches[&0].path_bytes, b"bad\xff.txt");
    drain(&mut wire);
    for action in ["open", "reveal", "copy", "preview", "trash", "editFile"] {
        super::actions::act(&mut m, action, &mut wire).unwrap();
        assert!(drain(&mut wire).is_empty(), "{action} must remain refused");
    }
    wire::finish(wire, reader);
}
#[test]
fn tui_contents_fix1_scope_matches_shared_cycle_and_words() {
    for everywhere in [false, true] {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = listing(&mut wire, "default");
        let catalog = Root::new();
        m.prompts.i18n = super::contents_tests::catalog(&catalog);
        press(&mut m, &mut wire, Key::character('g', "ctrl"));
        m.find.session.scope = Scope::Folder;
        m.find.session.wire.everywhere = everywhere;
        let mut shared = Session::default();
        shared.scope = Scope::Folder;
        shared.wire.everywhere = everywhere;
        for _ in 0..4 {
            shared.scope = shared.next_scope();
            press(&mut m, &mut wire, Key::named("Tab", ""));
            assert_eq!(
                m.find.session.scope, shared.scope,
                "TUI must use the shared cycle"
            );
            let home = std::env::var("HOME").unwrap();
            let root = Session::scope_root("/listing", &home, shared.scope);
            let words = scope_label(
                &m.prompts.i18n,
                root.as_deref(),
                &home,
                shared.scope,
                Kind::Contents,
            );
            let status = super::prompts::status(&m, &Theme::from_text(""), 120, Duration::ZERO).0;
            assert!(
                render::frame_tests::plain(&status).contains(&words),
                "shared scope words: {words}"
            );
        }
        wire::finish(wire, reader);
    }
}
#[test]
fn tui_contents_fix1_held_kitty_graphics_through_stage() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let root = Root::new();
    let output = root
        .command(
            false,
            "tui::contents_fix_tests::tui_contents_fix1_kitty_child",
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let waiting = stdout
        .split("WAIT_BEGIN")
        .nth(1)
        .unwrap()
        .split("WAIT_END")
        .next()
        .unwrap();
    assert!(
        !waiting.contains("a=d,d=I"),
        "held Kitty image was deleted: {waiting:?}"
    );
    let adopted = stdout
        .split("ADOPT_BEGIN")
        .nth(1)
        .unwrap()
        .split("ADOPT_END")
        .next()
        .unwrap();
    assert!(
        adopted.contains("a=d,d=I"),
        "adoption must clear the previous picture"
    );
}
#[test]
#[ignore = "private captured-stdout stage entry point"]
fn tui_contents_fix1_kitty_child() {
    if std::env::var_os("FLEA_TUI_CONTENTS_CHILD").is_none() {
        return;
    }
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "default");
    m.cursor = 1;
    m.rows.get_mut(&1).unwrap().name = "picture.png".into();
    m.rows.get_mut(&1).unwrap().icon = "image-png".into();
    m.preview_visible = true;
    m.preview_loaded = true;
    m.preview_path = m.current_path().unwrap();
    let mut graphics = Graphics::new();
    graphics.protocol = Protocol::Kitty;
    let mut shown = Shown::default();
    let mut frame = String::new();
    stage::step(
        &mut m,
        &mut graphics,
        &mut shown,
        &mut wire,
        (120, 16),
        false,
        &mut frame,
    )
    .unwrap();
    graphics.bytes = super::graphics::PLACE.to_vec();
    let gen = enter(&mut m, &mut wire, "needle");
    assert!(contents::guarded(&m));
    drain(&mut wire);
    for action in [
        "open",
        "reveal",
        "copy",
        "preview",
        "editFile",
        "cursorDown",
    ] {
        super::actions::act(&mut m, action, &mut wire).unwrap();
        assert!(drain(&mut wire).is_empty(), "held operations stay refused");
        assert_eq!(m.cursor, 1, "the displayed target remains held");
    }
    println!("WAIT_BEGIN");
    std::io::stdout().flush().unwrap();
    stage::step(
        &mut m,
        &mut graphics,
        &mut shown,
        &mut wire,
        (120, 16),
        false,
        &mut frame,
    )
    .unwrap();
    println!("WAIT_END");
    land(&mut m, &mut wire, gen, 51, &["new.txt"]);
    println!("ADOPT_BEGIN");
    std::io::stdout().flush().unwrap();
    stage::step(
        &mut m,
        &mut graphics,
        &mut shown,
        &mut wire,
        (120, 16),
        false,
        &mut frame,
    )
    .unwrap();
    println!("ADOPT_END");
    wire::finish(wire, reader);
}
