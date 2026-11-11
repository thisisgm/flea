use crate::jsondoc;
use crate::tui::{
    action_key_tests::{drain, listing},
    actions,
    editor::Editor,
    input::Key,
    keymap::Map,
    model::Model,
    wire::{self, count, text, Wire},
};
use std::path::Path;

fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
fn receive(m: &mut Model, w: &mut Wire, line: &str) {
    m.receive(jsondoc::parse(line).unwrap(), w).unwrap();
}
// The filter line typed to "fold", with the id of the request its last letter sent.
fn typed_filter(m: &mut Model, w: &mut Wire) -> usize {
    m.editor = Some(Editor::new("filter", String::new(), m.path.clone()));
    for c in "fold".chars() {
        press(m, w, Key::character(c, ""));
    }
    let sent = drain(w);
    count(
        sent.iter()
            .rev()
            .find(|v| text(v, "c") == "filter")
            .expect("the filter asks the backend"),
        "id",
    )
}
fn answer(m: &mut Model, w: &mut Wire, id: usize) {
    receive(
        m,
        w,
        &format!(r#"{{"t":"filtered","id":{id},"query":"fold","n":2,"rows":[0],"listing":1}}"#),
    );
}

// Enter on the filter line commits it and opens the highlighted folder, whether the answer came before or after it.
#[test]
fn one_enter_on_the_filter_line_acts_on_the_highlighted_row() {
    for answered_first in [true, false] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "default");
        let id = typed_filter(&mut m, &mut w);
        if answered_first {
            answer(&mut m, &mut w, id);
        }
        press(&mut m, &mut w, Key::named("Return", ""));
        assert!(m.editor.is_none(), "Enter commits the line");
        if !answered_first {
            assert!(
                m.pending.is_none(),
                "an Enter before the answer waits for it"
            );
            answer(&mut m, &mut w, id);
        }
        assert_eq!(
            m.pending.as_deref(),
            Some(Path::new("/listing/folder")),
            "answered first: {answered_first}"
        );
        wire::finish(w, reader);
    }
}

// A key between the Enter and the answer drops the wait; Down commits the line without acting, so the matches can be read.
#[test]
fn another_key_drops_a_waiting_enter_and_down_commits_without_acting() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = listing(&mut w, "default");
    let id = typed_filter(&mut m, &mut w);
    press(&mut m, &mut w, Key::named("Return", ""));
    press(&mut m, &mut w, Key::named("Down", ""));
    answer(&mut m, &mut w, id);
    assert!(m.pending.is_none(), "the key after Enter dropped its wait");
    let mut m = listing(&mut w, "default");
    let id = typed_filter(&mut m, &mut w);
    answer(&mut m, &mut w, id);
    press(&mut m, &mut w, Key::named("Down", ""));
    assert!(
        m.editor.is_none() && m.filter == "fold",
        "Down commits the filter"
    );
    assert!(m.pending.is_none(), "Down acts on no row");
    wire::finish(w, reader);
}

// Enter on the find line searches and opens the highlighted result once its rows land; Down searches and only shows them.
#[test]
fn one_enter_on_the_find_line_acts_on_the_first_result_when_it_lands() {
    for (key, opens) in [("Return", true), ("Down", false)] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "default");
        press(&mut m, &mut w, Key::character('f', ""));
        m.editor
            .as_mut()
            .expect("f opens Find")
            .replace("fold".into());
        press(&mut m, &mut w, Key::named(key, ""));
        let sent = drain(&mut w);
        let gen = count(
            sent.iter()
                .find(|v| text(v, "c") == "searchquery")
                .expect("the names query"),
            "gen",
        );
        assert!(m.pending.is_none());
        receive(
            &mut m,
            &mut w,
            &crate::backend::proto::listed_gen_line(1, 0.0, 0.0, 0, "/listing", true, gen),
        );
        receive(
            &mut m,
            &mut w,
            r#"{"t":"rows","start":0,"rows":[{"n":"folder","d":true}]}"#,
        );
        let opened = m.pending.as_deref() == Some(Path::new("/listing/folder"));
        assert_eq!(opened, opens, "{key} on the find line");
        wire::finish(w, reader);
    }
}

// Shift, Ctrl and Alt with Return leave every line open and unsent; plain Return and keypad Enter commit it.
#[test]
fn only_a_plain_return_commits_a_line() {
    for mods in ["shift", "ctrl", "alt"] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "default");
        let mut editor = Editor::new("path", String::new(), m.path.clone());
        editor.value = "/elsewhere".into();
        m.editor = Some(editor);
        press(&mut m, &mut w, Key::named("Return", mods));
        assert_eq!(
            m.editor.as_ref().map(|e| e.value.as_str()),
            Some("/elsewhere"),
            "{mods}+Return"
        );
        assert!(m.pending.is_none(), "{mods}+Return");
        wire::finish(w, reader);
    }
    let keypad = crate::tui::input::Decoder::default().feed(b"\x1b[57414u", false);
    assert_eq!(
        keypad,
        [Key::named("Return", "")],
        "keypad Enter reads as a plain Return"
    );
}
