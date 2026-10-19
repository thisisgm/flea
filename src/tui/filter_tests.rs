use super::super::{actions, input::{Key, Pointer}, keymap::Map, model::Model, render::BODY_ROW, theme::Theme, wire::{self, count, text}};
use crate::jsondoc::{self, Json};
use crate::tui::render::frame_tests::plain;

const LARGE: usize = 100_000;

fn drain(wire: &mut wire::Wire) -> Vec<Json> {
    wire.send(vec![("c", wire::word("fence"))]).unwrap();
    let mut values = Vec::new();
    loop {
        let value = wire.events.recv_timeout(std::time::Duration::from_secs(10)).unwrap().unwrap();
        if text(&value, "c") == "fence" { return values; }
        values.push(value);
    }
}
// A folder of 100,000 names holding its first two rows, the way a large folder sits before any scroll.
fn large(wire: &mut wire::Wire) -> Model {
    let mut m = Model::new("/large".into(), &Json::Null);
    m.open(m.path.clone(), wire).unwrap();
    m.receive(jsondoc::parse(&format!(r#"{{"t":"listed","n":{LARGE}}}"#)).unwrap(), wire).unwrap();
    m.receive(jsondoc::parse(r#"{"t":"rows","start":0,"rows":[{"n":"alpha"},{"n":"one"}]}"#).unwrap(), wire).unwrap();
    drain(wire);
    m
}
fn press(m: &mut Model, wire: &mut wire::Wire, key: Key) { actions::key(m, &key, &Map::load(), wire).unwrap(); }
fn typed(m: &mut Model, wire: &mut wire::Wire, keys: &str) {
    for c in keys.chars() { press(m, wire, Key::character(c, "")); }
}
fn pointer(m: &mut Model, wire: &mut wire::Wire, button: u32, y: usize) {
    let mut key = Key::named("", "");
    key.pointer = Some(Pointer { button, x: 30, y, released: false, motion: false });
    press(m, wire, key);
}
fn held(m: &Model) -> Vec<usize> { m.rows.keys().copied().collect() }
fn rows(m: &mut Model, wire: &mut wire::Wire, start: usize, names: &[&str]) {
    let rows = names.iter().map(|n| format!(r#"{{"n":"{n}"}}"#)).collect::<Vec<_>>().join(",");
    m.receive(jsondoc::parse(&format!(r#"{{"t":"rows","start":{start},"rows":[{rows}]}}"#)).unwrap(), wire).unwrap();
}
// A filter on the large folder, typed and closed with Return, its answer drawn three rows at a time.
fn answered(w: &mut wire::Wire, query: &str, matches: &str) -> Model {
    let mut m = large(w);
    m.height = 3;
    typed(&mut m, w, &format!("/{query}"));
    press(&mut m, w, Key::named("Return", ""));
    let ask = last_ask(&drain(w)).clone();
    answer(&mut m, w, &ask, matches);
    m
}
fn last_ask(sent: &[Json]) -> &Json {
    sent.iter().rev().find(|v| text(v, "c") == "filter").expect("the filter asks the backend about the whole folder")
}
fn answer(m: &mut Model, wire: &mut wire::Wire, ask: &Json, rows: &str) {
    let line = format!(r#"{{"t":"filtered","id":{},"query":"{}","n":{LARGE},"rows":{rows},"ms":0.010,"listing":1}}"#, count(ask, "id"), text(ask, "query"));
    m.receive(jsondoc::parse(&line).unwrap(), wire).unwrap();
}
fn windows(sent: &[Json]) -> Vec<(usize, usize)> {
    sent.iter().filter(|v| text(v, "c") == "window").map(|v| (count(v, "start"), count(v, "count"))).collect()
}

#[test]
fn a_match_outside_the_loaded_rows_is_shown_and_counted() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = large(&mut w);
    typed(&mut m, &mut w, "/needle");
    let sent = drain(&mut w);
    let ask = last_ask(&sent).clone();
    assert_eq!(text(&ask, "query"), "needle");
    answer(&mut m, &mut w, &ask, "[99999]");
    assert_eq!(windows(&drain(&mut w)), [(99999, 1)], "only the drawn match is fetched");
    m.receive(jsondoc::parse(r#"{"t":"rows","start":99999,"rows":[{"n":"Needle.txt"}],"kinds":[],"listing":1}"#).unwrap(), &mut w).unwrap();
    assert_eq!(m.visible(), [99999]);
    assert_eq!(m.current_path(), Some(m.path.join("Needle.txt")));
    let theme = Theme::from_text("");
    assert!(plain(&crate::tui::layout::footer(&m, &theme, 128, std::time::Duration::ZERO)).starts_with("Filter needle  1 of 100,000 shown"));
    let frame = plain(&crate::tui::render::frame(&m, &theme, &Map::load(), 130, 31, std::time::Duration::ZERO));
    assert!(frame.contains("Needle.txt") && frame.contains("1 of 100,000 shown"), "{frame}");
    wire::finish(w, reader);
}
#[test]
fn superseded_answers_drop_and_scrolling_fetches_only_the_missing_matches() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = large(&mut w);
    m.height = 3;
    typed(&mut m, &mut w, "/n");
    let first = last_ask(&drain(&mut w)).clone();
    typed(&mut m, &mut w, "e");
    let second = last_ask(&drain(&mut w)).clone();
    answer(&mut m, &mut w, &first, "[1,5]");
    assert!(drain(&mut w).is_empty() && m.filter_count() == 1, "an answer to an older query changes nothing");
    answer(&mut m, &mut w, &second, "[1,40,41,42,900]");
    assert_eq!((m.cursor, m.filter_count()), (1, 5));
    assert_eq!(windows(&drain(&mut w)), [(40, 2)], "one window per run of adjacent drawn matches");
    // Rows 0 and 2 carry the query in their names, so only the answer, never a name match, keeps them out.
    rows(&mut m, &mut w, 0, &["ne0", "one", "ne2"]);
    assert_eq!(held(&m), [1], "a reply to an older window cannot bring back a row the filter does not draw");
    rows(&mut m, &mut w, 40, &["ne40", "new41"]);
    press(&mut m, &mut w, Key::named("Return", ""));
    for _ in 0..3 { press(&mut m, &mut w, Key::named("Down", "")); }
    assert_eq!((m.cursor, m.visible()), (42, vec![40, 41, 42]));
    assert_eq!(windows(&drain(&mut w)), [(42, 1)]);
    rows(&mut m, &mut w, 42, &["new42"]);
    assert_eq!(held(&m), [40, 41, 42], "rows the filter no longer draws are not held");
    typed(&mut m, &mut w, "/w");
    press(&mut m, &mut w, Key::named("Return", ""));
    let narrower = last_ask(&drain(&mut w)).clone();
    assert_eq!(text(&narrower, "query"), "new");
    assert_eq!((m.visible(), m.filter_count()), (vec![41, 42], 2), "until its answer a query draws and counts only the held rows it matches");
    press(&mut m, &mut w, Key::named("Up", ""));
    pointer(&mut m, &mut w, 64, BODY_ROW);
    assert_eq!(m.cursor, 42, "a cursor key and the wheel wait for the answer, as each waits for a listing");
    answer(&mut m, &mut w, &narrower, "[41,42]");
    assert_eq!((m.cursor, m.filter_count()), (42, 2));
    drain(&mut w);
    m.receive(jsondoc::parse(&format!(r#"{{"t":"listed","n":{LARGE}}}"#)).unwrap(), &mut w).unwrap();
    let third = last_ask(&drain(&mut w)).clone();
    assert_eq!(text(&third, "query"), "new", "a renumbered listing asks again");
    answer(&mut m, &mut w, &narrower, "[1]");
    assert!(m.name_filter.waiting && m.filter_count() == 0, "an answer to a request sent before the re-list is dropped, though its query matches");
    answer(&mut m, &mut w, &second, "[1]");
    answer(&mut m, &mut w, &third, "[7,42]");
    assert_eq!((m.cursor, m.filter_count()), (42, 2));
    drain(&mut w);
    press(&mut m, &mut w, Key::named("Escape", ""));
    assert!(m.filter.is_empty());
    assert_eq!(windows(&drain(&mut w)), [(40, 3)], "clearing refills the viewport around the cursor");
    wire::finish(w, reader);
}
#[test]
fn first_last_and_a_click_name_drawn_matches() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = answered(&mut w, "ne", "[1,40,41,42,900]");
    drain(&mut w);
    press(&mut m, &mut w, Key::named("End", ""));
    assert_eq!((m.cursor, m.visible()), (900, vec![41, 42, 900]), "the last key lands on the last match");
    assert_eq!(windows(&drain(&mut w)), [(41, 2), (900, 1)]);
    rows(&mut m, &mut w, 41, &["ne41", "ne42"]);
    rows(&mut m, &mut w, 900, &["ne900"]);
    press(&mut m, &mut w, Key::named("Up", ""));
    assert_eq!((m.cursor, m.visible()), (42, vec![41, 42, 900]));
    // The third drawn row is match 900, where the listing's own top plus two names another row.
    assert_ne!(m.top + 2, 900);
    pointer(&mut m, &mut w, 0, BODY_ROW + 2);
    assert_eq!(m.cursor, 900, "a click names the match drawn under the pointer");
    press(&mut m, &mut w, Key::named("Home", ""));
    assert_eq!((m.cursor, m.visible()), (1, vec![1, 40, 41]), "the first key lands on the first match");
    wire::finish(w, reader);
}
// The rows a filter answer draws, then each command one key sends with the rows it names, on a model of its own.
fn sent_after(query: &str, matches: &str, key: Key) -> (Vec<usize>, Vec<(String, Vec<usize>)>) {
    let (mut w, reader) = wire::echo_wire();
    let mut m = answered(&mut w, query, matches);
    // The yazi keymap binds y to copy, which the default keymap leaves unbound.
    m.preset = "yazi".into();
    drain(&mut w);
    let drawn = m.visible();
    press(&mut m, &mut w, key);
    let sent = drain(&mut w).iter().map(|v| (text(v, "c").to_string(),
        v.get("rows").and_then(Json::as_array).map_or_else(Vec::new, |rows| rows.iter().filter_map(Json::as_f64).map(|i| i as usize).collect()))).collect();
    wire::finish(w, reader);
    (drawn, sent)
}
#[test]
fn a_filter_with_no_match_leaves_no_row_to_act_on() {
    let (mut w, reader) = wire::echo_wire();
    let m = answered(&mut w, "zzz", "[]");
    assert!(m.visible().is_empty() && held(&m).is_empty() && m.current_path().is_none());
    wire::finish(w, reader);
    for (key, command) in [(Key::named("Delete", ""), "trash"), (Key::named("Delete", "shift"), "menuaction"), (Key::character('y', ""), "paths")] {
        assert_eq!(sent_after("one", "[1]", key.clone()), (vec![1], vec![(command.to_string(), vec![1])]), "{key:?} on the one drawn match");
        assert_eq!(sent_after("zzz", "[]", key.clone()), (vec![], vec![]), "{key:?} reached a row the filter does not draw");
    }
}
// The preview's no-match gate looks the cursor up once per frame, by binary search, however many rows match.
#[test]
fn the_preview_gate_costs_one_search_per_frame_at_a_million_matches() {
    let theme = Theme::from_text("");
    let map = crate::tui::keymap::Map::load();
    let n = 1_000_000;
    let (mut w, reader) = wire::echo_wire();
    let mut m = crate::tui::render::frame_tests::model();
    (m.preview_visible, m.total) = (true, n);
    m.apply_filter("e".into(), &mut w).unwrap();
    let rows = (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    m.receive(jsondoc::parse(&format!(r#"{{"t":"filtered","id":1,"query":"e","n":{n},"rows":[{rows}]}}"#)).unwrap(), &mut w).unwrap();
    assert_eq!(m.current_path(), Some(m.preview_path.clone()), "the cursor's preview is shown, so the frame asks the gate");
    crate::tui::prompts::GATE.with(|gate| gate.set((0, 0)));
    let frame = crate::tui::render::frame(&m, &theme, &map, 130, 31, std::time::Duration::ZERO);
    let (calls, steps) = crate::tui::prompts::GATE.with(std::cell::Cell::get);
    assert!(!frame.is_empty());
    assert_eq!(calls, 1, "the gate ran once per preview row");
    assert!(steps <= 2 * 20, "{steps} comparisons for one cursor among {n} matches");
    drain(&mut w);
    wire::finish(w, reader);
}
