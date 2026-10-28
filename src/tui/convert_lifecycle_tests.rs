//! The real action and reply path must retain the captured identity through cancellation.
use super::{
    action_key_tests::drain,
    actions,
    convert_tests::{frame, opened},
    input::Key,
    keymap::Map,
    wire::{self, count, text},
};
use crate::jsondoc::parse;

#[test]
fn tui_convert_rejects_stale_replies_and_cancels_a_delayed_start() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "photo.png");
    actions::key(&mut m, &Key::named("Return", ""), &Map::load(), &mut w).unwrap();
    let sent = drain(&mut w);
    let request = count(
        sent.iter()
            .find(|v| text(v, "c") == "convertbatch")
            .unwrap(),
        "requestId",
    );
    actions::key(&mut m, &Key::named("Escape", ""), &Map::load(), &mut w).unwrap();
    assert!(drain(&mut w).is_empty());
    for line in [
        format!(
            r#"{{"t":"convertstarted","id":9,"requestId":{},"source":"/fx/v/work/photo.png"}}"#,
            request + 1
        ),
        format!(
            r#"{{"t":"convertstarted","id":9,"requestId":{request},"source":"/fx/v/work/other.png"}}"#
        ),
    ] {
        m.receive(parse(&line).unwrap(), &mut w).unwrap();
    }
    assert!(drain(&mut w).is_empty());
    m.receive(parse(&format!(r#"{{"t":"convertstarted","id":9,"requestId":{request},"source":"/fx/v/work/photo.png"}}"#)).unwrap(), &mut w).unwrap();
    let sent = drain(&mut w);
    assert!(sent
        .iter()
        .any(|v| text(v, "c") == "transfercancel" && count(v, "id") == 9));
    m.receive(parse(&format!(r#"{{"t":"convertdone","id":8,"requestId":{request},"source":"/fx/v/work/photo.png","ok":true,"path":"/foreign"}}"#)).unwrap(), &mut w).unwrap();
    assert!(m.converting.run.is_some());
    m.receive(parse(&format!(r#"{{"t":"convertdone","id":9,"requestId":{request},"source":"/fx/v/work/photo.png","ok":false,"err":"cancelled","paths":[]}}"#)).unwrap(), &mut w).unwrap();
    assert!(m.converting.run.is_none());
    assert!(m.menu_action.is_empty());
    wire::finish(w, reader);
}
#[test]
fn tui_convert_range_validation_options_and_menu_are_reachable() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "report.pdf");
    let f = m.converting.card.as_mut().unwrap();
    f.count = Some(3);
    f.pages = "4".into();
    super::convert::submit(&mut m, &mut w).unwrap();
    assert!(drain(&mut w).iter().all(|v| text(v, "c") != "convertbatch"));
    assert!(frame(&m).contains("This document has 3 pages."));
    m.converting.card.as_mut().unwrap().pages = "2-1".into();
    super::convert::submit(&mut m, &mut w).unwrap();
    assert!(frame(&m).contains("Enter All or a page range"));
    super::convert::cancel(&mut m, &mut w).unwrap();
    drain(&mut w);
    m.rows.get_mut(&7).unwrap().name = "photo.png".into();
    super::render::menu_run::open(&mut m, &mut w).unwrap();
    m.menu_ready = true;
    let entries = super::render::menu_layout::entries(&m, &Map::load());
    let row = entries
        .iter()
        .flatten()
        .find(|r| r.id == "convert")
        .unwrap();
    assert_eq!(row.label, "Convert…");
    super::convert::open(&mut m, &mut w, true).unwrap();
    assert_eq!(m.menu_action, "convertValidate");
    assert!(drain(&mut w)
        .iter()
        .any(|v| text(v, "op") == "validate" && text(v, "action") == "convert"));
    wire::finish(w, reader);
}
#[test]
fn tui_convert_metadata_dims_and_default_button_persists() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "photo.png");
    let before = frame(&m);
    for _ in 0..7 {
        actions::key(&mut m, &Key::character('j', ""), &Map::load(), &mut w).unwrap();
    }
    let f = m.converting.card.as_ref().unwrap();
    assert_eq!(f.format(), "pdf");
    assert!(!f.option_enabled());
    let after = frame(&m);
    for label in ["Remove metadata", "[ Cancel ]", "[ Convert ]"] {
        assert_eq!(
            before.lines().position(|l| l.contains(label)),
            after.lines().position(|l| l.contains(label))
        );
    }
    actions::key(&mut m, &Key::named("Tab", ""), &Map::load(), &mut w).unwrap();
    assert_eq!(m.converting.card.as_ref().unwrap().focus, 2);
    wire::finish(w, reader);
}

#[test]
fn tui_convert_busy_refusal_closes_the_pending_submission() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "photo.png");
    super::convert::submit(&mut m, &mut w).unwrap();
    drain(&mut w);
    m.receive(
        parse(
            r#"{"t":"error","where":"convert","path":"","msg":"an operation is already running"}"#,
        )
        .unwrap(),
        &mut w,
    )
    .unwrap();
    assert!(m.converting.run.is_none());
    assert!(m.menu_action.is_empty());
    assert!(m.error.contains("already running"));
    assert!(m.transfer.is_empty());
    assert!(drain(&mut w).iter().any(|v| text(v, "op") == "close"));
    wire::finish(w, reader);
}
