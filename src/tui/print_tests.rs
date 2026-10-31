use super::{
    action_key_tests::drain,
    actions,
    input::Key,
    keymap::Map,
    model::Model,
    render::{
        self,
        frame_tests::{caps_language, grid, model},
    },
    theme::Theme,
    wire::{self, count, text, Wire},
};
use crate::jsondoc;
use std::time::Duration;

fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
fn answer(m: &mut Model, w: &mut Wire, line: &str) {
    m.receive(
        jsondoc::parse(&line.replace("ID", &m.action_id.to_string())).unwrap(),
        w,
    )
    .unwrap();
}
fn frame(m: &Model) -> String {
    grid(
        &render::frame(
            m,
            &Theme::from_text(""),
            &Map::load(),
            130,
            31,
            Duration::ZERO,
        ),
        130,
        31,
    )
    .join("\n")
}
fn opened(w: &mut Wire) -> (Model, usize) {
    let mut m = model();
    m.rows.get_mut(&7).unwrap().name = "report.pdf".into();
    press(&mut m, w, Key::character('P', "ctrl"));
    drain(w);
    answer(
        &mut m,
        w,
        r#"{"t":"menuaction","op":"snapshot","id":ID,"ok":true,"count":1}"#,
    );
    drain(w);
    answer(
        &mut m,
        w,
        r#"{"t":"menuaction","op":"validate","action":"print","id":ID,"ok":true,"paths":["/fx/v/work/report.pdf"]}"#,
    );
    let sent = drain(w);
    let query = sent
        .iter()
        .find(|v| text(v, "c") == "printoptions")
        .map(|v| count(v, "id"))
        .expect("Print must send its capability query");
    (m, query)
}
fn options(m: &mut Model, w: &mut Wire, id: usize, capable: bool) {
    answer(
        m,
        w,
        &format!(
            r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Other","default":false}},{{"name":"Brother","default":true}}],"printer":"Brother","duplex":{capable},"twoSided":false,"color":{capable},"grayscale":false,"setupArgv":["system-config-printer"]}}"#
        ),
    );
}
fn submit(m: &mut Model, w: &mut Wire) -> usize {
    press(m, w, Key::named("Return", ""));
    drain(w)
        .iter()
        .find(|v| text(v, "c") == "print")
        .map_or(0, |v| count(v, "requestId"))
}

#[test]
fn tui_print_action_opens_real_card() {
    let (mut w, reader) = wire::echo_wire();
    let (mut m, query) = opened(&mut w);
    options(&mut m, &mut w, query, true);
    let shown = frame(&m);
    for expected in [
        "Print report.pdf",
        "PRINTER",
        "COPIES",
        "PAGES",
        "Brother",
        "default",
        "All",
        "Two-sided",
        "Grayscale",
        "[ Cancel ]",
        "[ Print ]",
    ] {
        assert!(shown.contains(expected), "missing {expected}: {shown}");
    }
    assert!(!super::layout::focused(&m));
    let requests = drain(&mut w);
    assert!(!requests.iter().any(|v| text(v, "c") == "print"));
    wire::finish(w, reader);
}

#[test]
fn tui_print_headings_drop_the_greek_accents() {
    let (mut w, reader) = wire::echo_wire();
    let (mut m, query) = opened(&mut w);
    options(&mut m, &mut w, query, true);
    m.i18n = caps_language("el");
    let shown = frame(&m);
    // Plain to_uppercase draws ΕΚΤΥΠΩΤΉΣ, ΑΝΤΊΓΡΑΦΑ and ΣΕΛΊΔΕΣ.
    for expected in ["ΕΚΤΥΠΩΤΗΣ", "ΑΝΤΙΓΡΑΦΑ", "ΣΕΛΙΔΕΣ"] {
        assert!(shown.contains(expected), "missing {expected}: {shown}");
    }
    wire::finish(w, reader);
}

#[test]
fn tui_print_capabilities_dim_without_layout_shift() {
    let (mut w, reader) = wire::echo_wire();
    let (mut m, query) = opened(&mut w);
    options(&mut m, &mut w, query, false);
    let disabled = frame(&m);
    assert!(disabled.contains("[ ] Two-sided") && disabled.contains("[ ] Grayscale"));
    let theme = Theme::from_text(super::render::cards::tests::PALETTE);
    let out = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    let inks = super::render::cards::tests::inks(&out, 130, 31);
    let rows = grid(&out, 130, 31);
    let y = rows
        .iter()
        .position(|row| row.contains("Two-sided"))
        .unwrap();
    let x = rows[y].chars().position(|c| c == '[').unwrap();
    assert_eq!(
        inks[y][x + 4].0,
        super::render::cards::tests::role(&theme.muted)
    );
    press(&mut m, &mut w, Key::named("Tab", ""));
    press(&mut m, &mut w, Key::named("Tab", ""));
    press(&mut m, &mut w, Key::named("Right", ""));
    let requests = drain(&mut w);
    let query = requests
        .iter()
        .find(|v| text(v, "c") == "printoptions")
        .unwrap();
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printoptions","id":{},"ok":true,"destinations":[{{"name":"Brother","default":true}},{{"name":"Other","default":false}}],"printer":"Other","duplex":true,"twoSided":false,"color":true,"grayscale":false,"setupArgv":[]}}"#,
            count(query, "id")
        ),
    );
    let enabled = frame(&m);
    let out = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    let enabled_inks = super::render::cards::tests::inks(&out, 130, 31);
    for label in ["Two-sided", "Grayscale"] {
        let column = rows[y].find(label).unwrap();
        assert_eq!(
            inks[y][column].0,
            super::render::cards::tests::role(&theme.muted)
        );
        assert_eq!(
            enabled_inks[y][column].0,
            super::render::cards::tests::role(&theme.foreground)
        );
    }
    for label in ["Two-sided", "Grayscale", "[ Cancel ]", "[ Print ]"] {
        assert_eq!(
            disabled
                .lines()
                .enumerate()
                .find_map(|(y, row)| row.find(label).map(|x| (y, x))),
            enabled
                .lines()
                .enumerate()
                .find_map(|(y, row)| row.find(label).map(|x| (y, x)))
        );
    }
    wire::finish(w, reader);
}

#[test]
fn tui_print_no_printer_differs_from_query_failure() {
    let (mut w, reader) = wire::echo_wire();
    let (mut m, query) = opened(&mut w);
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printoptions","id":{query},"ok":true,"destinations":[],"printer":"","duplex":false,"twoSided":false,"color":false,"grayscale":false,"setupArgv":["system-config-printer"]}}"#
        ),
    );
    assert!(frame(&m).contains("No printer is set up."));
    assert!(frame(&m).contains("Set Up a Printer…"));
    press(&mut m, &mut w, Key::named("Escape", ""));
    let (mut m, query) = opened(&mut w);
    answer(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"printoptions","id":{query},"ok":false,"err":"CUPS unavailable"}}"#),
    );
    assert!(!frame(&m).contains("No printer is set up."));
    assert!(m.error.contains("CUPS unavailable"));
    assert_eq!(submit(&mut m, &mut w), 0);
    wire::finish(w, reader);
}

#[test]
fn tui_print_receipt_requires_real_acceptance() {
    let (mut w, reader) = wire::echo_wire();
    let (mut m, query) = opened(&mut w);
    options(&mut m, &mut w, query, true);
    let request = submit(&mut m, &mut w);
    assert_ne!(request, 0);
    assert!(!m.message.contains("Sent"));
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printed","requestId":{request},"id":70,"path":"/fx/v/work/report.pdf","ok":true}}"#
        ),
    );
    assert!(!m.message.contains("Sent"), "unstarted receipt refused");
    answer(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"printstarted","requestId":{request},"id":70}}"#),
    );
    assert!(!frame(&m).contains("[ Print ]"));
    for (id, path) in [(71, "/fx/v/work/report.pdf"), (70, "/other.pdf")] {
        answer(
            &mut m,
            &mut w,
            &format!(
                r#"{{"t":"printed","requestId":{request},"id":{id},"path":"{path}","ok":true}}"#
            ),
        );
        assert!(!m.message.contains("Sent"));
    }
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printed","requestId":{request},"id":70,"path":"/fx/v/work/report.pdf","ok":false,"message":"report.pdf was not printed: queue refused."}}"#
        ),
    );
    assert!(m.error.contains("queue refused"));
    assert!(!m.message.contains("Sent"));
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printdone","requestId":{request},"id":70,"ok":false,"err":"queue refused"}}"#
        ),
    );
    let (mut m, query) = opened(&mut w);
    options(&mut m, &mut w, query, true);
    let request = submit(&mut m, &mut w);
    answer(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"printstarted","requestId":{request},"id":80}}"#),
    );
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printed","requestId":{request},"id":80,"path":"/fx/v/work/report.pdf","ok":true}}"#
        ),
    );
    assert_eq!(m.message, "Sent report.pdf to Brother");
    wire::finish(w, reader);
}

#[test]
fn tui_print_revalidates_selection_and_cancel() {
    let root = crate::backend::testdir::TestDir::new("print-shipped-cancellation");
    let (mut w, reader) = wire::echo_wire();
    let (mut m, query) = opened(&mut w);
    super::print_fix_tests::cancellation_catalog(&mut m, root.path());
    options(&mut m, &mut w, query, true);
    let request = submit(&mut m, &mut w);
    assert_ne!(request, 0);
    press(&mut m, &mut w, Key::named("Escape", ""));
    answer(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"printstarted","requestId":{request},"id":90}}"#),
    );
    assert!(drain(&mut w)
        .iter()
        .any(|v| text(v, "c") == "transfercancel" && count(v, "id") == 90));
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printdone","requestId":{request},"id":90,"ok":false,"sent":0,"err":"cancelled"}}"#
        ),
    );
    assert_eq!(
        m.message,
        "Printing cancelled. Sent: None. Not sent: report.pdf."
    );
    let (mut m, query) = opened(&mut w);
    options(&mut m, &mut w, query, true);
    press(&mut m, &mut w, Key::named("Escape", ""));
    drain(&mut w);
    options(&mut m, &mut w, query, true);
    assert!(
        !frame(&m).contains("[ Print ]"),
        "late query cannot reopen a cancelled card"
    );
    wire::finish(w, reader);
}
