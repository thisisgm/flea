use super::{
    action_key_tests::drain,
    actions,
    input::{Key, Pointer},
    keymap::Map,
    model::Model,
    print,
    render::frame_tests::model,
    wire::{self, count, text, Wire},
};
use crate::jsondoc;

fn press(m: &mut Model, w: &mut Wire, key: Key) {
    actions::key(m, &key, &Map::load(), w).unwrap();
}
fn answer(m: &mut Model, w: &mut Wire, line: &str) {
    m.receive(jsondoc::parse(line).unwrap(), w).unwrap();
}
fn ready(w: &mut Wire) -> Model {
    let mut m = model();
    m.rows.get_mut(&7).unwrap().name = "report.pdf".into();
    print::open(&mut m, w, false).unwrap();
    let id = m.action_id;
    for line in [
        format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true}}"#),
        format!(
            r#"{{"t":"menuaction","op":"validate","action":"print","id":{id},"ok":true,"paths":["/fx/v/work/report.pdf"]}}"#
        ),
    ] {
        answer(&mut m, w, &line);
    }
    let id = m.printing.card.as_ref().unwrap().query.unwrap();
    answer(
        &mut m,
        w,
        &format!(
            r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Brother","default":true}}],"printer":"Brother","duplex":true,"twoSided":false,"color":true,"grayscale":false}}"#
        ),
    );
    drain(w);
    m
}
#[test]
fn tui_print_fix_same_folder_relist_keeps_edits_navigation_closes() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = ready(&mut w);
    m.printing.card.as_mut().unwrap().copies = "12".into();
    answer(
        &mut m,
        &mut w,
        r#"{"t":"listed","n":9,"read":0.0,"sort":0.0,"v":1,"w":true,"path":"/fx/v/work","listing":4,"changed":2}"#,
    );
    assert_eq!(
        m.printing.card.as_ref().expect("watcher keeps card").copies,
        "12"
    );
    m.pending = Some("/other".into());
    answer(
        &mut m,
        &mut w,
        r#"{"t":"listed","n":0,"path":"/other","listing":5}"#,
    );
    assert!(m.printing.card.is_none());
    wire::finish(w, reader);
}
#[test]
fn tui_print_fix_busy_both_entry_points_close_menu_and_explain() {
    for from_menu in [false, true] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = ready(&mut w);
        print::submit(&mut m, &mut w).unwrap();
        m.printing.card = None;
        m.menu_action.clear();
        m.menu = from_menu;
        if from_menu {
            print::open(&mut m, &mut w, true).unwrap();
        } else {
            press(&mut m, &mut w, Key::character('P', "ctrl"));
        }
        assert!(!m.menu);
        assert_eq!(m.error, m.i18n.tr("dialogs.output.error.print_busy"));
        wire::finish(w, reader);
    }
}
#[test]
fn tui_print_fix_busy_causes_have_specific_reasons() {
    for (cause, key) in [
        (0, "search.contents.identity"),
        (1, "tui.operation.search_busy"),
        (2, "jobs.target.busy"),
    ] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = model();
        match cause {
            0 => super::print_guard_tests::settle_unsafe(&mut m, &mut w),
            1 => m.searching = true,
            _ => m.pending_clipboard = true,
        }
        press(&mut m, &mut w, Key::character('P', "ctrl"));
        assert!(m.printing.card.is_none());
        assert_eq!(m.error, m.i18n.tr(key), "cause {cause}");
        wire::finish(w, reader);
    }
}
#[test]
fn tui_print_fix_malformed_options_fail_loud_and_stale_is_ignored() {
    for body in [
        r#""destinations":null"#,
        r#""destinations":[],"printer":"","duplex":false"#,
        r#""destinations":[{"name":"-bad","default":true}]"#,
    ] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = ready(&mut w);
        print::query(&mut m, &mut w, "").unwrap();
        let id = m.printing.card.as_ref().unwrap().query.unwrap();
        answer(
            &mut m,
            &mut w,
            &format!(r#"{{"t":"printoptions","id":{},"ok":true,{body}}}"#, id + 1),
        );
        assert_eq!(m.printing.card.as_ref().unwrap().query, Some(id));
        answer(
            &mut m,
            &mut w,
            &format!(r#"{{"t":"printoptions","id":{id},"ok":true,{body}}}"#),
        );
        let f = m.printing.card.as_ref().unwrap();
        assert!(f.query.is_none() && !f.queried && !f.error.is_empty());
        assert!(!m.error.is_empty());
        let out = super::render::frame(
            &m,
            &super::theme::Theme::from_text(""),
            &Map::load(),
            130,
            31,
            std::time::Duration::ZERO,
        );
        assert!(!out.contains(&m.i18n.tr("menu.loading")));
        let query_error = m.printing.card.as_ref().unwrap().error.clone();
        for _ in 0..3 {
            press(&mut m, &mut w, Key::named("Tab", ""));
        }
        press(&mut m, &mut w, Key::character('2', ""));
        assert_eq!(
            m.printing.card.as_ref().unwrap().error,
            query_error,
            "field edit keeps query failure"
        );
        let out = super::render::frame(
            &m,
            &super::theme::Theme::from_text(""),
            &Map::load(),
            130,
            31,
            std::time::Duration::ZERO,
        );
        assert!(
            !out.contains(&m.i18n.tr("menu.loading")),
            "field edit cannot restart loading"
        );
        print::submit(&mut m, &mut w).unwrap();
        assert!(!drain(&mut w).iter().any(|v| text(v, "c") == "print"));
        wire::finish(w, reader);
    }
}
#[test]
fn tui_print_fix_pointer_motion_keeps_keyboard_field_and_input() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = ready(&mut w);
    for _ in 0..3 {
        press(&mut m, &mut w, Key::named("Tab", ""));
    }
    press(&mut m, &mut w, Key::character('1', ""));
    let out = super::render::frame(
        &m,
        &super::theme::Theme::from_text(""),
        &Map::load(),
        130,
        31,
        std::time::Duration::ZERO,
    );
    let rows = super::render::frame_tests::grid(&out, 130, 31);
    let y = rows.iter().position(|row| row.contains("COPIES")).unwrap() + 2;
    let x = rows[y - 2].find("COPIES").unwrap() + 1;
    for (x, digit) in [(x, '2'), (x + 24, '3')] {
        let mut key = Key::named("Pointer", "");
        key.pointer = Some(Pointer {
            x,
            y,
            button: 0,
            motion: true,
            released: false,
        });
        press(&mut m, &mut w, key);
        press(&mut m, &mut w, Key::character(digit, ""));
    }
    let f = m.printing.card.as_ref().unwrap();
    assert_eq!(f.copies, "123");
    assert!(f.pages.is_empty());
    wire::finish(w, reader);
}
#[test]
fn tui_print_fix_cancel_reports_prior_receipts_and_unsent_files() {
    let root = crate::backend::testdir::TestDir::new("print-cancel-catalog");
    let (mut w, reader) = wire::echo_wire();
    let mut m = ready(&mut w);
    cancellation_catalog(&mut m, root.path());
    m.printing
        .card
        .as_mut()
        .unwrap()
        .paths
        .push("/fx/v/work/hold.pdf".into());
    print::submit(&mut m, &mut w).unwrap();
    let request = drain(&mut w)
        .iter()
        .find(|v| text(v, "c") == "print")
        .map(|v| count(v, "requestId"))
        .unwrap();
    answer(
        &mut m,
        &mut w,
        &format!(r#"{{"t":"printstarted","requestId":{request},"id":90}}"#),
    );
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printed","requestId":{request},"id":90,"path":"/fx/v/work/report.pdf","ok":true}}"#
        ),
    );
    press(&mut m, &mut w, Key::named("Escape", ""));
    answer(
        &mut m,
        &mut w,
        &format!(
            r#"{{"t":"printdone","requestId":{request},"id":90,"ok":false,"sent":1,"err":"cancelled"}}"#
        ),
    );
    assert_eq!(
        m.message,
        "Printing cancelled. Sent: report.pdf. Not sent: hold.pdf."
    );
    wire::finish(w, reader);
}
pub(super) fn cancellation_catalog(m: &mut Model, root: &std::path::Path) {
    let catalog = root.join("catalog/en");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(
        catalog.join("flea.json"),
        r#"{"dialogs.output.cancelled":"Printing cancelled. Sent: %1. Not sent: %2."}"#,
    )
    .unwrap();
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![
            root.join("catalog"),
            crate::i18n::test_catalogs::english_only(),
        ],
    });
}

#[test]
fn tui_print_fix_mime_is_lazy_and_first_print_initializes_once() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = model();
    assert!(
        m.printing.types.get().is_none(),
        "TUI startup must not load Print MIME tables"
    );
    print::open(&mut m, &mut w, false).unwrap();
    let held = m.printing.types.get().unwrap() as *const _;
    print::cancel(&mut m, &mut w).unwrap();
    print::open(&mut m, &mut w, false).unwrap();
    assert_eq!(held, m.printing.types.get().unwrap() as *const _);
    wire::finish(w, reader);
}
#[test]
fn tui_print_fix_copies_respect_shared_limits() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = ready(&mut w);
    let max = flea::ui_logic_op4::printing::MAX_COPIES;
    let f = m.printing.card.as_mut().unwrap();
    f.focus = 1;
    f.copies = max.to_string();
    f.edit_fresh = false;
    press(&mut m, &mut w, Key::named("Up", ""));
    press(&mut m, &mut w, Key::character('1', ""));
    assert_eq!(m.printing.card.as_ref().unwrap().copies, max.to_string());
    m.printing.card.as_mut().unwrap().copies = "1".into();
    press(&mut m, &mut w, Key::named("Down", ""));
    assert_eq!(m.printing.card.as_ref().unwrap().copies, "1");
    wire::finish(w, reader);
}
