use super::tests::*;
use crate::tui::{graphics::Graphics, icons, pane, stage, wire};

#[test]
fn tui_office_failure_and_missing_metadata_are_truthful() {
    let i18n = crate::i18n::I18n::default();
    let cases: [(&str, Vec<u8>, &str, &str); 4] = [
        (
            "note.odt",
            vec![b'x'; 33],
            r#""err":"LibreOffice is not installed on this box.","unavailable":true"#,
            KEYS[0],
        ),
        (
            "lease.docx",
            b"docx bytes".to_vec(),
            r#""err":"encrypted","locked":true"#,
            KEYS[1],
        ),
        (
            "broken.odt",
            vec![b'x'; 33],
            r#""err":"the converter wrote no PDF""#,
            KEYS[2],
        ),
        (
            "sealed.docx",
            [
                &[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1][..],
                &[0; 25][..],
            ]
            .concat(),
            r#""err":"the converter wrote no PDF""#,
            KEYS[1],
        ),
    ];
    for (name, bytes, failure, key) in cases {
        let root = Fixture::new();
        std::fs::write(root.path(name), &bytes).unwrap();
        let mut m = model(&root, name, "Document");
        let (mut wire, reader) = wire::echo_wire();
        let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        let (id, _) = ask(&wire);
        receive(
            &mut m,
            &mut wire,
            &format!(r#"{{"t":"officepdf","id":{id},"state":"failed",{failure}}}"#),
        );
        settle(|| {
            step(&mut m, &mut graphics, &mut shown, &mut wire);
            m.preview_state.unshown.is_some()
        });
        let text = screen(&m);
        assert!(
            text.contains(&i18n.tr(key)),
            "{name} shows its own sentence: {text}"
        );
        for other in KEYS.iter().filter(|other| **other != key) {
            assert!(
                !text.contains(&i18n.tr(other)),
                "{name} does not borrow {other}"
            );
        }
        assert!(
            text.contains(&i18n.tr("preview.openWith")),
            "{name} keeps Open With"
        );
        assert!(
            m.pdf.is_none() && m.preview_state.office.doc.pdf.is_none(),
            "{name} draws no page"
        );
        let mark = m
            .preview_state
            .unshown_mark
            .expect("a can't-show sentence names its mark");
        let glyph = match key {
            KEYS_LOCKED => icons::LOCK,
            KEYS_CALM => icons::FILE,
            _ => pane::ALERT,
        };
        assert_eq!(
            (mark.glyph, mark.error),
            (glyph, key != KEYS_CALM),
            "{name} keeps its own mark and tone"
        );
        drop(graphics);
        wire::finish(wire, reader);
    }
    let root = Fixture::new();
    let converted = root.path("bare.pdf");
    std::fs::write(root.path(DOCX), b"docx bytes").unwrap();
    std::fs::write(&converted, pdf(3, "", "")).unwrap();
    let mut m = model(&root, DOCX, "Word 2007 document");
    let (mut wire, reader) = wire::echo_wire();
    let (mut graphics, mut shown) = (Graphics::new(), stage::Shown::default());
    step(&mut m, &mut graphics, &mut shown, &mut wire);
    let (id, _) = ask(&wire);
    receive(&mut m, &mut wire, &ready(id, &converted));
    settle(|| {
        step(&mut m, &mut graphics, &mut shown, &mut wire);
        m.pdf.as_ref().is_some_and(|pdf| pdf.pages == 3)
    });
    let text = screen(&m);
    assert!(text.contains("Pages · 3"), "{text}");
    assert!(
        !text.contains("Title ·") && !text.contains("Author ·"),
        "absent metadata is omitted, not blank: {text}"
    );
    drop(graphics);
    wire::finish(wire, reader);
}
