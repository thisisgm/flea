use super::*;
use crate::tui::render::frame_tests::{model, plain};
use crate::tui::wire;
use std::time::Duration;

const THEME: &str = "background='#1c120d'\nforeground='#c6b395'\nred='#d65a4f'\ngreen='#5faf5f'\nyellow='#d7af5f'";
const LINES: &str = r#"[[3,"Visual",0," gate notes"],[],[1,"Nope"],[2,"Line four keeps the preview busy."]]"#;

fn reply(m: &mut Model, wire: &mut Wire, token: usize, lines: &str) {
    let text = format!(r#"{{"t":"code","token":{token},"slot":"tui","state":"styled","first":1,"total":4,"lines":{lines}}}"#);
    m.receive(crate::jsondoc::parse(&text).unwrap(), wire).unwrap();
}

// The laid-out row that starts source line `line`.
fn index(m: &Model, line: usize) -> usize {
    m.preview_state.code.parts.iter().position(|part| *part == (line, 0)).unwrap()
}

#[test]
fn a_request_sends_the_held_text_once_for_the_shown_window() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = model();
    request(&mut m, &mut wire).unwrap();
    let sent = wire.events.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!((text(&sent, "c"), text(&sent, "slot"), text(&sent, "name"), text(&sent, "lang")), ("code", "tui", "notes.txt", ""));
    assert_eq!(text(&sent, "text"), m.preview_body);
    assert_eq!((count(&sent, "token"), count(&sent, "first"), count(&sent, "count")), (1, 1, 400));
    request(&mut m, &mut wire).unwrap();
    assert!(wire.events.recv_timeout(Duration::from_millis(200)).is_err(), "a window already asked for was asked for again");
    m.preview_generation += 1;
    request(&mut m, &mut wire).unwrap();
    let sent = wire.events.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(count(&sent, "token"), 2);
    wire::finish(wire, reader);
}

#[test]
fn only_a_current_styled_reply_that_spells_each_line_colours_it() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = model();
    request(&mut m, &mut wire).unwrap();
    wire.events.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let theme = Theme::from_text(THEME);
    reply(&mut m, &mut wire, 9, LINES);
    assert!(!paint(&m, index(&m, 1), "Visual gate notes", 30, &theme).contains(&theme.syntax[2]), "a stale reply coloured the text");
    reply(&mut m, &mut wire, 1, LINES);
    let first = paint(&m, index(&m, 1), "Visual gate notes", 30, &theme);
    assert!(first.starts_with(&format!("{}{}Visual{} gate notes", theme.foreground, theme.syntax[2], theme.foreground)), "{first:?}");
    assert_eq!(text_width_of(&first), 30);
    let third = paint(&m, index(&m, 3), "This file is the fixture's plain text sample.", 60, &theme);
    assert!(!third.contains(&theme.syntax[0]), "runs that do not spell their line were drawn");
    assert!(paint(&m, index(&m, 4), "Line four keeps the preview busy.", 60, &theme).contains(&theme.syntax[1]));
    m.preview_generation += 1;
    assert!(!paint(&m, index(&m, 1), "Visual gate notes", 30, &theme).contains(&theme.syntax[2]), "the last preview's runs coloured the next");
    wire::finish(wire, reader);
}

#[test]
fn a_wrapped_part_keeps_its_offset_into_the_source_line_and_fits_its_width() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = model();
    request(&mut m, &mut wire).unwrap();
    wire.events.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    reply(&mut m, &mut wire, 1, LINES);
    let theme = Theme::from_text(THEME);
    m.preview_state.code.parts = vec![(1, 3)];
    let part = paint(&m, 0, "ual gate", 12, &theme);
    assert!(part.starts_with(&format!("{}{}ual{} gate", theme.foreground, theme.syntax[2], theme.foreground)), "{part:?}");
    assert_eq!(plain(&paint(&m, 0, "ual gate", 4, &theme)), "ual ");
    wire::finish(wire, reader);
}

fn text_width_of(painted: &str) -> usize {
    crate::tui::cells::text_width(&plain(painted))
}
