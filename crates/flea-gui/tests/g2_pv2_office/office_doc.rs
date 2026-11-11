//! Office document proofs: a document converts then opens in the PDF viewer, and each failure projects its approved sentence.
use super::*;

// ql-office-converting then ql-office-doc: no pager and dimmed zoom, the cached PDF opens, a stale line drops.
#[gpui::test]
fn pv2_office_document_converts_then_opens_in_the_pdf_viewer(cx: &mut TestAppContext) {
    let (view, asked, _) = office(cx, true);
    open(cx, view, "/d/talk.odp");
    assert_eq!(
        asked.borrow()[0],
        r#"{"c":"officepdf","id":1,"path":"/d/talk.odp"}"#,
        "a presentation asks for its PDF"
    );
    let i18n = crate::test_catalogs::english();
    let bar = view.update(cx, |v, _, cx| v.bar(&i18n, cx)).unwrap();
    assert_eq!(
        bar.glyph, "presentation",
        "the bar leads with the presentation mark"
    );
    let strip = bar.strip.unwrap();
    assert_eq!(
        strip.counter, None,
        "the pager waits until there are pages to count"
    );
    assert!(
        strip
            .items
            .iter()
            .filter(|(i, _, _)| matches!(
                i.control,
                flea::ui_logic_doc_preview::viewer::Control::ZoomIn
                    | flea::ui_logic_doc_preview::viewer::Control::ZoomOut
            ))
            .all(|(i, _, _)| !i.enabled),
        "the zoom marks rest dimmed"
    );
    assert!(
        strip
            .items
            .iter()
            .filter(|(i, _, _)| matches!(
                i.control,
                flea::ui_logic_doc_preview::viewer::Control::Previous
                    | flea::ui_logic_doc_preview::viewer::Control::Next
            ))
            .all(|(i, _, _)| !i.visible),
        "no pager"
    );
    assert_eq!(
        view.update(cx, |v, _, _| v.failure(&i18n)).unwrap(),
        None,
        "converting says nothing"
    );
    assert!(
        view.update(cx, |v, _, cx| v.shown(cx)).unwrap(),
        "the swap may show the converting card"
    );
    let thumb = {
        let buffer = image::RgbaImage::from_raw(16, 9, vec![255; 16 * 9 * 4]).unwrap();
        std::sync::Arc::new(gpui::RenderImage::new(vec![image::Frame::new(buffer)]))
    };
    view.update(cx, |v, _, cx| v.set_thumbnail("/d/talk.odp", thumb, cx))
        .unwrap();
    let rect = view
        .update(cx, |v, _, _| v.thumb_rect((902., 625.)))
        .unwrap()
        .unwrap();
    assert_eq!(
        rect.2,
        902. - 56.,
        "a 16 by 9 slide takes the card's width inside the page margin"
    );
    assert!(
        (rect.1 - (625. - rect.3) / 2.).abs() < 1e-6,
        "and sits centred in the height left over"
    );
    assert!(reply(
        cx,
        view,
        r#"{"t":"officepdf","id":1,"state":"converting"}"#
    ));
    open(cx, view, "/d/garden-report.docx");
    assert!(
        !reply(
            cx,
            view,
            r#"{"t":"officepdf","id":1,"state":"ready","path":"/c/talk.pdf"}"#
        ),
        "the left subject's late line is dropped"
    );
    assert!(
        view.update(cx, |v, _, _| v.thumb.is_none()).unwrap(),
        "the old thumbnail went with its subject"
    );
    assert!(reply(
        cx,
        view,
        r#"{"t":"officepdf","id":2,"state":"ready","path":"/c/garden.pdf"}"#
    ));
    assert_eq!(state(cx, view), State::Ready);
    assert!(
        !view.update(cx, |v, _, cx| v.shown(cx)).unwrap(),
        "the swap waits for the first page"
    );
    assert_eq!(
        view.update(cx, |v, _, cx| v.bar(&i18n, cx).glyph).unwrap(),
        "file-text",
        "a document leads with the lined page"
    );
}

// ql-office-locked: a failed conversion of an encrypted package reads its header and says it is password protected.
#[gpui::test]
fn pv2_office_failures_project_the_approved_sentences(cx: &mut TestAppContext) {
    let root = root();
    let lease = root.0.join("lease.docx");
    let mut cfb = vec![0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    cfb.resize(4096, 0);
    std::fs::write(&lease, cfb).unwrap();
    let report = root.0.join("report.docx");
    // A real container, since a fake one falls to the unsupported card before any conversion runs.
    std::fs::write(&report, container::REAL_DOCX).unwrap();
    let i18n = crate::test_catalogs::english();
    let (view, _, _) = office(cx, true);
    let cases = [
        (
            &lease,
            "soffice: general input/output error",
            false,
            "This document is password protected.",
            "lock",
        ),
        (
            &report,
            "the converter wrote no PDF",
            false,
            "This document could not be previewed.",
            "alert",
        ),
        (
            &report,
            "the conversion took longer than 60 seconds",
            false,
            "This document could not be previewed.",
            "alert",
        ),
        (
            &report,
            "LibreOffice is not installed on this box.",
            true,
            "This file cannot be previewed.",
            "file",
        ),
    ];
    for (n, (path, err, unavailable, sentence, glyph)) in cases.into_iter().enumerate() {
        open(cx, view, path.to_str().unwrap());
        let flag = if unavailable {
            r#","unavailable":true"#
        } else {
            ""
        };
        reply(
            cx,
            view,
            &format!(
                r#"{{"t":"officepdf","id":{},"state":"failed","err":"{}"{flag}}}"#,
                n + 1,
                escape(err)
            ),
        );
        for _ in 0..200 {
            if view
                .update(cx, |v, _, _| {
                    v.failure(&i18n).map(|f| f.glyph) == Some(glyph)
                })
                .unwrap()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            draw(cx, view);
        }
        let failure = view
            .update(cx, |v, _, _| v.failure(&i18n))
            .unwrap()
            .unwrap();
        assert_eq!(
            (failure.sentence.as_str(), failure.glyph),
            (sentence, glyph),
            "{err}"
        );
        assert_eq!(
            failure.open_with.as_deref(),
            Some("Open With\u{2026}"),
            "{err}: Open With is the way in"
        );
        assert_eq!(
            view.update(cx, |v, _, _| v.office.diagnostic.clone())
                .unwrap(),
            err,
            "the backend text stays a diagnostic"
        );
    }
    let read = view.update(cx, |v, _, _| v.sniffed_bytes).unwrap();
    assert!(
        read <= 8 + 65_557 + 302,
        "the sniff read a bounded head and tail: {read}"
    );
    let bar = view.update(cx, |v, _, cx| v.bar(&i18n, cx)).unwrap();
    assert_eq!(
        bar.strip.unwrap().counter,
        None,
        "the failure keeps the viewer's bar with no pager"
    );
    open(cx, view, "/d/talk.odp");
    view.update(cx, |v, window, cx| v.leave(window, cx))
        .unwrap();
    assert_eq!(
        state(cx, view),
        State::Cancelled,
        "leaving mid conversion is a state"
    );
    assert_eq!(
        view.update(cx, |v, _, _| v.failure(&i18n)).unwrap(),
        None,
        "and never a toast or a sentence"
    );
}
