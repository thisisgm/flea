use super::*;

fn line(text: &str) -> Line {
    parse(text).unwrap_or_else(|| panic!("{text} parses"))
}

fn failed(id: u64, err: &str, unavailable: bool) -> Line {
    Line {
        id,
        wire: Wire::Failed {
            err: err.into(),
            unavailable,
            locked: false,
        },
    }
}

#[test]
fn office_groups_by_extension() {
    for (name, group) in [
        ("a.docx", Group::Document),
        ("a.ODT", Group::Document),
        ("a.doc", Group::Document),
        ("a.rtf", Group::Document),
        ("b.xlsx", Group::Spreadsheet),
        ("b.ods", Group::Spreadsheet),
        ("b.xls", Group::Spreadsheet),
        ("c.pptx", Group::Presentation),
        ("c.odp", Group::Presentation),
        ("c.PPT", Group::Presentation),
    ] {
        assert_eq!(Group::of(&format!("/home/u/{name}")), Some(group), "{name}");
    }
    assert_eq!(Group::of("/d/data.csv"), None, "CSV stays the CSV preview");
    assert_eq!(
        Group::of("/d.docx/readme"),
        None,
        "a folder's extension is not the file's"
    );
    assert_eq!(
        Group::Presentation.glyph(),
        "presentation",
        "a presentation's bar leads with the screen"
    );
    assert_eq!(Group::Spreadsheet.count_key(), "document.office_sheets");
}

// The backend's lines, as src/backend/officepdf.rs writes them, and the sheets line it is asked to join.
#[test]
fn office_wire_lines_parse() {
    assert_eq!(
        line(r#"{"t":"officepdf","id":4,"state":"converting"}"#),
        Line {
            id: 4,
            wire: Wire::Converting
        }
    );
    assert!(matches!(
        line(r#"{"t":"officepdf","id":4,"state":"ready","path":"/c/x.pdf"}"#).wire,
        Wire::Pdf { path, first: None } if path == "/c/x.pdf"
    ));
    assert_eq!(
        line(r#"{"t":"officepdf","id":4,"state":"failed","err":"no","unavailable":true}"#).wire,
        Wire::Failed {
            err: "no".into(),
            unavailable: true,
            locked: false
        }
    );
    assert_eq!(
        line(r#"{"t":"officesheets","id":5,"state":"ready","names":["Monthly","Savings"],"paths":["/c/a.csv","/c/b.csv"]}"#).wire,
        Wire::Sheets { names: vec!["Monthly".into(), "Savings".into()], paths: vec!["/c/a.csv".into(), "/c/b.csv".into()] }
    );
    assert_eq!(
        parse(r#"{"t":"code","id":4,"state":"ready"}"#),
        None,
        "another reply type"
    );
    assert_eq!(
        parse(r#"{"t":"officepdf","id":4,"state":"odd"}"#),
        None,
        "an unknown state"
    );
    assert_eq!(
        parse(r#"{"t":"officepdf","state":"ready","path":"/x"}"#),
        None,
        "no id"
    );
    assert_eq!(
        request_line(3, "/d/a \"b\".docx", false),
        r#"{"c":"officepdf","id":3,"path":"/d/a \"b\".docx"}"#
    );
    assert!(request_line(3, "/d/b.xlsx", true).starts_with(r#"{"c":"officesheets""#));
}

// The error inventory: each backend string is a reason, and only the approved sentences are ever drawn.
#[test]
fn office_error_inventory_projects_the_approved_sentences() {
    assert_eq!(
        Reason::of("LibreOffice is not installed on this box."),
        Some(Reason::NotInstalled)
    );
    assert_eq!(
        Reason::of("the conversion took longer than 60 seconds"),
        Some(Reason::TimedOut(60))
    );
    assert_eq!(
        Reason::of("the converter wrote no PDF"),
        Some(Reason::NoPdf)
    );
    assert_eq!(
        Reason::of("that file is not an office document"),
        Some(Reason::NotOffice)
    );
    assert_eq!(
        Reason::of("soffice exited with 139"),
        Some(Reason::Other),
        "converter stderr is diagnostic only"
    );
    assert_eq!(
        Reason::of("the conversion took longer than many seconds"),
        Some(Reason::Other)
    );
    assert_eq!(Reason::of("cancelled"), None, "cancelled is a state");
    assert_eq!(
        Reason::of("replaced by a newer request"),
        None,
        "a replaced request is a state"
    );
    let i18n = crate::i18n::test_catalogs::english();
    let mut office = Office::default();
    for (err, unavailable, key, glyph) in [
        (
            "the conversion took longer than 60 seconds",
            false,
            "document.office_failed",
            "alert",
        ),
        (
            "the converter wrote no PDF",
            false,
            "document.office_failed",
            "alert",
        ),
        (
            "soffice: general input/output error",
            false,
            "document.office_failed",
            "alert",
        ),
        (
            "LibreOffice is not installed on this box.",
            true,
            "document.office_unsupported",
            "file",
        ),
        (
            "that file is not an office document",
            true,
            "document.office_unsupported",
            "file",
        ),
    ] {
        office.open("/d/garden-report.docx", 7);
        office.reply(&failed(7, err, unavailable));
        office.sniffed(false);
        let p = office
            .projection()
            .unwrap_or_else(|| panic!("{err} projects"));
        assert_eq!((p.key, p.glyph, p.open_with), (key, glyph, true), "{err}");
        assert_eq!(
            office.diagnostic, err,
            "the backend's text is kept apart as the diagnostic"
        );
        assert_ne!(
            i18n.tr(p.key),
            err,
            "the sentence is never the backend's text"
        );
    }
    assert_eq!(
        i18n.tr("document.office_failed"),
        "This document could not be previewed."
    );
    assert_eq!(
        i18n.tr("document.office_locked"),
        "This document is password protected."
    );
    assert_eq!(
        i18n.tr("document.office_unsupported"),
        "This file cannot be previewed."
    );
    assert_eq!(i18n.tr("document.office_open_with"), "Open With\u{2026}");
}

#[test]
fn office_states_from_open_to_ready_cancelled_and_locked() {
    let mut office = Office::default();
    office.open("/d/talk.odp", 3);
    assert_eq!(
        office.state,
        State::Converting,
        "a fresh subject converts with no sentence"
    );
    assert_eq!(
        office.projection(),
        None,
        "converting draws the thumbnail or the kind mark and nothing else"
    );
    assert_eq!(
        office.reply(&line(
            r#"{"t":"officepdf","id":2,"state":"ready","path":"/c/old.pdf"}"#
        )),
        Applied::Stale,
        "another id is stale"
    );
    assert_eq!(
        office.reply(&line(r#"{"t":"officepdf","id":3,"state":"converting"}"#)),
        Applied::Converting
    );
    assert_eq!(
        office.reply(&line(
            r#"{"t":"officepdf","id":3,"state":"ready","path":"/c/t.pdf"}"#
        )),
        Applied::Pdf("/c/t.pdf".into())
    );
    assert_eq!((office.state.clone(), office.id), (State::Ready, 0));
    assert_eq!(
        office.count(4),
        Some(("document.office_slides", 4)),
        "a presentation counts slides"
    );
    assert_eq!(
        office.reply(&line(
            r#"{"t":"officepdf","id":3,"state":"ready","path":"/c/t.pdf"}"#
        )),
        Applied::Stale,
        "a repeat is stale"
    );

    office.open("/d/talk.odp", 4);
    office.leave();
    assert_eq!(
        office.state,
        State::Cancelled,
        "leaving before the answer cancels"
    );
    assert_eq!(
        office.projection(),
        None,
        "cancelled draws no sentence and no toast"
    );
    assert_eq!(
        office.reply(&line(
            r#"{"t":"officepdf","id":4,"state":"ready","path":"/c/t.pdf"}"#
        )),
        Applied::Stale,
        "a late line after leaving is dropped"
    );

    office.open("/d/talk.odp", 5);
    assert_eq!(
        office.reply(&failed(5, "cancelled", false)),
        Applied::Settled
    );
    assert_eq!(
        (office.state.clone(), office.projection()),
        (State::Cancelled, None),
        "the backend's cancel is a state"
    );
    office.open("/d/talk.odp", 6);
    assert_eq!(
        office.reply(&failed(6, "replaced by a newer request", false)),
        Applied::Ask,
        "a replaced request asks once more"
    );
    assert_eq!((office.state.clone(), office.id), (State::Converting, 0));
    office.asked(7);
    assert_eq!(
        office.reply(&failed(6, "replaced by a newer request", false)),
        Applied::Stale
    );
    office.reply(&failed(7, "replaced by a newer request", false));
    assert_eq!(office.state, State::Cancelled, "a second replace ends it");

    office.open("/d/report.docx", 10);
    office.reply(&line(
        r#"{"t":"officepdf","id":10,"state":"ready","path":"/c/r.pdf"}"#,
    ));
    office.undrawable(PDF_UNOPENED);
    assert_eq!(
        (office.state.clone(), office.diagnostic.as_str()),
        (State::Failed(Reason::Other), PDF_UNOPENED),
        "a ready PDF that does not open is a failure"
    );
    assert_eq!(office.projection().unwrap().key, "document.office_failed");
    office.undrawable(SHEET_REFUSED);
    assert_eq!(
        office.diagnostic, PDF_UNOPENED,
        "only a ready subject fails"
    );

    office.open("/d/lease.docx", 8);
    assert_eq!(
        office.reply(&failed(8, "soffice: general input/output error", false)),
        Applied::Sniff,
        "a converter failure asks the header"
    );
    office.sniffed(true);
    assert_eq!(office.state, State::Locked);
    let p = office.projection().unwrap();
    assert_eq!(
        (p.glyph, p.error_tone, p.key),
        ("lock", true, "document.office_locked")
    );
    office.open("/d/lease.docx", 9);
    office.reply(&line(
        r#"{"t":"officepdf","id":9,"state":"failed","err":"x","locked":true}"#,
    ));
    assert_eq!(
        office.state,
        State::Locked,
        "the backend's own locked flag needs no sniff"
    );
    office.open("/d/report.docx", 10);
    office.reply(&failed(10, "the converter wrote no PDF", false));
    assert_eq!(
        (office.state.clone(), office.projection()),
        (State::Converting, None),
        "a converter error draws no sentence until the header says whether it is a lock"
    );
    office.sniffed(false);
    assert_eq!(
        office.state,
        State::Failed(Reason::NoPdf),
        "an open header leaves the failure"
    );
    office.open("/d/report.docx", 11);
    office.reply(&failed(
        11,
        "LibreOffice is not installed on this box.",
        false,
    ));
    office.sniffed(true);
    assert_eq!(
        office.state,
        State::Unsupported(Reason::NotInstalled),
        "no converter is never read as a password"
    );
}

// budget.xlsx: the sheets arrive as cached CSVs, [ and ] switch them and wrap at the ends.
#[test]
fn office_sheets_switch_with_the_brackets() {
    let mut office = Office::default();
    office.open("/d/budget.xlsx", 2);
    assert!(office.wants_sheets());
    let ready = line(
        r#"{"t":"officesheets","id":2,"state":"ready","names":["Monthly","Savings"],"paths":["/c/m.csv","/c/s.csv"]}"#,
    );
    assert_eq!(
        office.reply(&ready),
        Applied::Sheet("/c/m.csv".into()),
        "the first sheet shows first"
    );
    assert_eq!(
        office.sheet_words(),
        vec![("Monthly".into(), true), ("Savings".into(), false)]
    );
    assert_eq!(
        office.count(0),
        Some(("document.office_sheets", 2)),
        "a workbook counts sheets"
    );
    assert_eq!(
        office.turn_sheet(-1),
        Some("/c/s.csv".into()),
        "[ on the first sheet wraps to the last, as the 3MF plate steps wrap"
    );
    assert_eq!(
        office.turn_sheet(1),
        Some("/c/m.csv".into()),
        "] on the last sheet wraps to the first"
    );
    assert_eq!(
        office.turn_sheet(1),
        Some("/c/s.csv".into()),
        "] shows the next sheet"
    );
    assert_eq!(office.sheet_words()[1], ("Savings".into(), true));
    office.open("/d/budget.xlsx", 3);
    let broken = line(r#"{"t":"officesheets","id":3,"state":"ready","names":["A"],"paths":[]}"#);
    assert_eq!(office.reply(&broken), Applied::Settled);
    assert_eq!(
        office.projection().map(|p| p.key),
        Some("document.office_failed"),
        "a workbook with no sheet fails"
    );
}
