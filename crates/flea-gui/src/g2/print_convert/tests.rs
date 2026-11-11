use super::{
    model::{Action, Card, Focus, Model},
    seam::{Effect, OutputError, Ticket},
};
use flea::{
    i18n::I18n,
    ui_logic_print_convert::{
        output::Kind,
        selection::{Selection, Source},
    },
};

pub fn selected(kind: Kind, names: &[&str]) -> Selection {
    Selection {
        generation: 11,
        menu: 4,
        files: names
            .iter()
            .map(|name| Source {
                path: format!("/fixture/{name}"),
                kind,
            })
            .collect(),
    }
}
pub(super) fn draft(action: Action, kind: Kind, names: &[&str]) -> Model {
    let mut m = Model::default();
    assert!(m.open(
        action,
        selected(kind, names),
        &crate::test_catalogs::english()
    ));
    m
}
pub(super) fn checked(m: &mut Model) {
    let ticket = match m.card.as_ref().unwrap() {
        Card::Convert(c) => c.pending.unwrap(),
        Card::Combine(c) => c.pending.unwrap(),
        _ => panic!(),
    };
    m.checked(ticket, Ok(()), &crate::test_catalogs::english());
}

#[test]
fn op4_convert_draft_focus_collision_and_late_check_keep_identity() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Convert, Kind::Image, &["photo.png"]);
    assert!(!m.open(Action::Convert, selected(Kind::Image, &["other.png"]), &c));
    assert_eq!(
        m.selection.as_ref().unwrap().files[0].path,
        "/fixture/photo.png"
    );
    let old = match m.card.as_ref().unwrap() {
        Card::Convert(c) => c.pending.unwrap(),
        _ => panic!(),
    };
    assert!(!m.can_submit());
    m.choose_format(2, &c);
    m.checked(old, Err(OutputError::Exists("old.jpg".into())), &c);
    assert!(m.error.is_empty());
    checked(&mut m);
    m.tab(false);
    assert_eq!(m.focus, Some(Focus::Check));
    m.toggle_check(&c);
    assert!(m.can_submit());
    for expected in [Focus::Cancel, Focus::Submit, Focus::Formats] {
        m.tab(false);
        assert_eq!(m.focus, Some(expected));
    }
    m.submit(&c);
    let request = m.run.as_ref().unwrap().ticket.request;
    assert!(!m.close());
    assert!(!m.line(&format!(r#"{{"t":"convertdone","requestId":{request},"id":8,"source":"/fixture/photo.png","ok":true,"paths":[],"path":"/fixture/x.webp"}}"#), &c));
    assert!(m.line(
        &format!(
            r#"{{"t":"convertstarted","requestId":{request},"id":8,"source":"/fixture/photo.png"}}"#
        ),
        &c
    ));
    assert!(!m.line(&format!(r#"{{"t":"convertdone","requestId":{request},"id":9,"source":"/fixture/photo.png","ok":true,"paths":[],"path":"/fixture/x.webp"}}"#), &c));
    assert!(m.line(&format!(r#"{{"t":"convertdone","requestId":{request},"id":8,"source":"/fixture/photo.png","ok":false,"collision":true,"paths":[],"path":"","err":"late collision"}}"#), &c));
    assert!(!m.busy);
    assert!(m.card.is_some());
    assert!(!m.error.is_empty());
    assert!(matches!(&m.card, Some(Card::Convert(c)) if c.strip && c.format == "webp"));
}

#[test]
fn op4_prestart_error_and_backend_death_retire_pending_activation() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Convert, Kind::Image, &["a.png"]);
    checked(&mut m);
    m.submit(&c);
    assert!(m.line(r#"{"t":"error","where":"convert","msg":"busy mount"}"#, &c));
    assert!(!m.busy);
    assert!(m.run.is_none());
    assert!(m.card.is_some());
    m.choose_format(2, &c);
    checked(&mut m);
    m.submit(&c);
    m.backend_died(&c);
    assert!(!m.busy);
    assert!(m.unavailable);
    assert_eq!(m.focus_order(), [Focus::Cancel]);
    let error = m.error.clone();
    m.choose_format(3, &c);
    assert_eq!(m.error, error);
    assert!(m.close());
}

#[test]
fn op4_partial_failure_refreshes_outputs_and_cancellation_has_no_refusal() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Convert, Kind::Image, &["a.png", "b.jpg"]);
    checked(&mut m);
    m.submit(&c);
    let r = m.run.as_ref().unwrap().ticket.request;
    m.line(
        &format!(r#"{{"t":"convertstarted","id":8,"requestId":{r},"source":"/fixture/a.png"}}"#),
        &c,
    );
    m.effects.clear();
    for fields in [
        r#""ok":true,"paths":[],"path":"","err":"""#,
        r#""ok":true,"paths":["/fixture/a.jpg"],"path":"/fixture/other.jpg","err":"""#,
        r#""ok":false,"paths":["/fixture/a.jpg"],"path":"/fixture/a.jpg""#,
    ] {
        assert!(!m.line(
            &format!(
                r#"{{"t":"convertdone","id":8,"requestId":{r},"source":"/fixture/a.png",{fields}}}"#
            ),
            &c
        ));
        assert!(m.busy);
        assert!(m.effects.is_empty());
    }

    assert!(m.line(&format!(r#"{{"t":"convertdone","id":8,"requestId":{r},"source":"/fixture/a.png","ok":false,"paths":["/fixture/a.jpg"],"path":"/fixture/a.jpg","err":"collision"}}"#), &c));
    assert!(m.effects.contains(&Effect::Refresh {
        paths: vec!["/fixture/a.jpg".into()],
        select: false,
    }));
    m.choose_format(2, &c);
    checked(&mut m);
    m.submit(&c);
    let r = m.run.as_ref().unwrap().ticket.request;
    m.effects.clear();
    assert!(m.line(&format!(r#"{{"t":"convertdone","id":9,"requestId":{r},"source":"/fixture/a.png","ok":false,"paths":[],"path":"","err":"cancelled"}}"#), &c));
    assert!(!m
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Status { error: true, .. })));
    assert!(m.card.is_none());
}

fn options(id: u64, printer: &str, duplex: bool, color: bool) -> String {
    format!(
        r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Simple","default":false}},{{"name":"Brother","default":true}}],"printer":"{printer}","duplex":{duplex},"twoSided":false,"color":{color},"grayscale":true,"setupArgv":["system-config-printer"]}}"#
    )
}
fn printer_ticket(m: &Model) -> Ticket {
    match m.card.as_ref().unwrap() {
        Card::Print(p) => p.pending.unwrap(),
        _ => panic!(),
    }
}

#[test]
fn op4_printer_query_failure_is_not_no_printer_and_options_drop_stale() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Print, Kind::Pdf, &["report.pdf"]);
    let id = printer_ticket(&m).request;
    assert!(!m.line(r#"{"t":"printoptions","ok":true,"id":1}"#, &c));
    assert!(m.line(&options(id, "Brother", true, true), &c));
    assert!(matches!(&m.card, Some(Card::Print(p)) if p.destinations[0].name == "Brother"));
    m.query_printer(Some(1));
    assert!(
        matches!(&m.card, Some(Card::Print(p)) if p.printer == "Brother" && p.capabilities.duplex && p.pending_printer == "Simple")
    );
    let next = printer_ticket(&m).request;
    assert!(!m.line(&options(id, "Brother", true, true), &c));
    assert!(m.line(&options(next, "Simple", false, false), &c));
    assert!(matches!(&m.card, Some(Card::Print(p)) if !p.capabilities.color && p.grayscale));
    assert!(!m.focus_order().contains(&Focus::Duplex));
    m.query_printer(Some(0));
    let id = printer_ticket(&m).request;
    assert!(m.line(
        &format!(r#"{{"t":"printoptions","id":{id},"ok":false,"err":"cups unavailable"}}"#),
        &c
    ));
    assert!(
        matches!(&m.card, Some(Card::Print(p)) if !p.destinations.is_empty() && p.pending.is_none())
    );
    assert!(!m.can_submit());
}

#[test]
fn op4_print_sent_is_one_receipt_without_progress_card() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Print, Kind::Pdf, &["report.pdf"]);
    let id = printer_ticket(&m).request;
    m.line(&options(id, "Brother", true, true), &c);
    if let Some(Card::Print(p)) = &mut m.card {
        p.printer_name = "Brother HL-L3270CDW".into();
    }
    m.submit(&c);
    let r = m.run.as_ref().unwrap().ticket.request;
    m.effects.clear();
    assert!(m.line(
        &format!(r#"{{"t":"printstarted","id":7,"requestId":{r}}}"#),
        &c
    ));
    assert!(m.card.is_none());
    assert!(m.line(&format!(r#"{{"t":"printed","id":7,"requestId":{r},"path":"/fixture/report.pdf","ok":true,"message":"ignored English"}}"#), &c));
    assert!(m.line(
        &format!(
            r#"{{"t":"printdone","id":7,"requestId":{r},"ok":true,"sent":1,"message":"duplicate","err":""}}"#
        ),
        &c
    ));
    assert_eq!(
        m.effects
            .iter()
            .filter(|e| matches!(e, Effect::Status { .. }))
            .count(),
        1
    );
    assert!(m.effects.contains(&Effect::Status {
        error: false,
        text: "Sent report.pdf to Brother HL-L3270CDW".into()
    }));
    assert!(!m
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Progress { .. })));
}

#[test]
fn op4_combining_reorders_paths_and_name_and_keeps_jpeg_transparency_valid() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Combine, Kind::Image, &["z.png", "a.jpg", "b.png"]);
    m.move_image(0, 2, &c);
    checked(&mut m);
    assert_eq!(
        m.selection
            .as_ref()
            .unwrap()
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["/fixture/a.jpg", "/fixture/b.png", "/fixture/z.png"]
    );
    m.setting(Focus::Format, 1, &c);
    checked(&mut m);
    assert!(
        matches!(&m.card, Some(Card::Combine(c)) if c.name == "a (combined).jpg" && c.settings.background == flea::ui_logic_print_convert::composition::Background::White)
    );
    m.setting(Focus::Layout, 2, &c);
    checked(&mut m);
    assert!(matches!(&m.card, Some(Card::Combine(c)) if c.background_enabled(3)));
    if let Some(Card::Combine(c)) = &mut m.card {
        c.columns_text.clear();
    }
    m.probe(&c);
    assert!(!m.can_submit());
    assert!(matches!(&m.card, Some(Card::Combine(c)) if c.settings.columns == 2));
    if let Some(Card::Combine(c)) = &mut m.card {
        c.columns_text = "2".into();
    }
    m.probe(&c);
    checked(&mut m);
    m.submit(&c);
    let line = m
        .effects
        .iter()
        .rev()
        .find_map(|e| match e {
            Effect::Send(s) => Some(s),
            _ => None,
        })
        .unwrap();
    let json = flea::jsondoc::parse(line).unwrap();
    assert_eq!(
        super::wire::strings(&json, "paths").unwrap(),
        ["/fixture/a.jpg", "/fixture/b.png", "/fixture/z.png"]
    );
}

#[test]
fn op4_combining_offers_pdf_last_and_keeps_a_transparent_ground() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Combine, Kind::Image, &["a.png", "b.png"]);
    m.setting(Focus::Format, 3, &c);
    checked(&mut m);
    assert!(
        matches!(&m.card, Some(Card::Combine(c)) if c.format == "pdf" && c.name == "a (combined).pdf" && c.settings.background == flea::ui_logic_print_convert::composition::Background::Transparent)
    );
    assert!(m.can_submit());
    m.submit(&c);
    let line = m
        .effects
        .iter()
        .rev()
        .find_map(|e| match e {
            Effect::Send(s) => Some(s),
            _ => None,
        })
        .unwrap();
    let json = flea::jsondoc::parse(line).unwrap();
    assert_eq!(json.get("format").and_then(|v| v.as_str()), Some("pdf"));
}

#[test]
fn op4_locale_switch_preserves_diagnostic_and_per_string_english_fallback() {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/g2/print_convert/fixtures");
    let de = I18n::load(flea::i18n::Paths {
        omarchy: base.clone(),
        language_file: base.join("language"),
        catalogs: vec![base],
    });
    let en = crate::test_catalogs::english();
    let mut m = draft(Action::Convert, Kind::Image, &["photo.png"]);
    assert_eq!(m.title(&de), "photo.png umwandeln");
    assert_eq!(de.tr("dialogs.output.metadata"), "Remove metadata");
    m.set_error(&en, "dialogs.output.error.operation", &["opaque CUPS 0x7f"]);
    m.localize(&de);
    assert_eq!(m.error, "Vorgang fehlgeschlagen. opaque CUPS 0x7f");
    m.localize(&en);
    assert_eq!(
        m.error,
        "The operation could not be completed. opaque CUPS 0x7f"
    );
    assert_eq!(
        de.trn("dialogs.output.title.combine", 3),
        "Combine 3 Images"
    );
    m.set_io_error(
        &en,
        Some("feedback.os.permission_denied"),
        "permission denied".into(),
    );
    m.localize(&de);
    assert!(m.error.ends_with("Zugriff verweigert."));
    m.set_io_error(&de, None, "opaque OS 0xff".into());
    assert!(m.error.ends_with("opaque OS 0xff"));
}

#[test]
fn op4_no_printer_setup_uses_returned_argv_and_closed_checks_stay_closed() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Print, Kind::Pdf, &["report.pdf"]);
    let id = printer_ticket(&m).request;
    assert!(m.line(&format!(r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[],"printer":"","duplex":false,"twoSided":false,"color":false,"grayscale":true,"setupArgv":["system-config-printer"]}}"#), &c));
    assert!(m.can_submit());
    m.submit(&c);
    assert!(m
        .effects
        .contains(&Effect::Launch(vec!["system-config-printer".into()])));
    assert!(m.card.is_none());
    assert!(m.run.is_none());
    let mut m = draft(Action::Convert, Kind::Image, &["photo.png"]);
    let ticket = match m.card.as_ref().unwrap() {
        Card::Convert(c) => c.pending.unwrap(),
        _ => panic!(),
    };
    m.close();
    m.checked(ticket, Ok(()), &c);
    assert!(m.card.is_none());
    assert!(!m.line("{broken", &c));
}

#[test]
fn op4_pdf_ranges_limits_and_count_diagnostics_keep_their_meaning() {
    let c = crate::test_catalogs::english();
    let mut m = draft(Action::Convert, Kind::Pdf, &["report.pdf"]);
    let ticket = match &m.card {
        Some(Card::Convert(s)) => s.count_pending.unwrap(),
        _ => panic!(),
    };
    m.document_pages(ticket, Ok(3), &c);
    checked(&mut m);
    if let Some(Card::Convert(s)) = &mut m.card {
        s.pages = "4".into();
    }
    m.probe(&c);
    assert_eq!(m.error, "This document has 3 pages.");
    assert!(!m.can_submit());
    let mut m = draft(Action::Convert, Kind::Pdf, &["report.pdf"]);
    let ticket = match &m.card {
        Some(Card::Convert(s)) => s.count_pending.unwrap(),
        _ => panic!(),
    };
    m.document_pages(ticket, Ok(10_001), &c);
    assert_eq!(
        m.error,
        "PDF conversion supports between 1 and 10000 pages."
    );
    let mut m = draft(Action::Convert, Kind::Pdf, &["report.pdf"]);
    let ticket = match &m.card {
        Some(Card::Convert(s)) => s.count_pending.unwrap(),
        _ => panic!(),
    };
    m.document_pages(ticket, Err("opaque Poppler 0x81".into()), &c);
    assert!(m.error.ends_with("opaque Poppler 0x81"));
}

#[path = "vf4_print_tests.rs"]
mod vf4_print;
