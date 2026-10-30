//! A fake office container falls to the unsupported card before any conversion is asked.
use super::*;

/// A real .docx container: a stored zip holding [Content_Types].xml and word/document.xml.
pub(super) const REAL_DOCX: [u8; 258] = [
    80, 75, 3, 4, 20, 0, 0, 0, 0, 0, 142, 188, 89, 93, 199, 28, 23, 60, 8, 0, 0, 0, 8, 0, 0, 0, 19,
    0, 0, 0, 91, 67, 111, 110, 116, 101, 110, 116, 95, 84, 121, 112, 101, 115, 93, 46, 120, 109,
    108, 60, 84, 121, 112, 101, 115, 47, 62, 80, 75, 3, 4, 20, 0, 0, 0, 0, 0, 142, 188, 89, 93, 22,
    195, 122, 25, 4, 0, 0, 0, 4, 0, 0, 0, 17, 0, 0, 0, 119, 111, 114, 100, 47, 100, 111, 99, 117,
    109, 101, 110, 116, 46, 120, 109, 108, 60, 119, 47, 62, 80, 75, 1, 2, 20, 3, 20, 0, 0, 0, 0, 0,
    142, 188, 89, 93, 199, 28, 23, 60, 8, 0, 0, 0, 8, 0, 0, 0, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    128, 1, 0, 0, 0, 0, 91, 67, 111, 110, 116, 101, 110, 116, 95, 84, 121, 112, 101, 115, 93, 46,
    120, 109, 108, 80, 75, 1, 2, 20, 3, 20, 0, 0, 0, 0, 0, 142, 188, 89, 93, 22, 195, 122, 25, 4,
    0, 0, 0, 4, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 1, 57, 0, 0, 0, 119, 111, 114,
    100, 47, 100, 111, 99, 117, 109, 101, 110, 116, 46, 120, 109, 108, 80, 75, 5, 6, 0, 0, 0, 0, 2,
    0, 2, 0, 128, 0, 0, 0, 108, 0, 0, 0, 0, 0,
];

#[gpui::test]
fn pv2_office_a_fake_container_falls_to_unsupported_before_any_conversion(cx: &mut TestAppContext) {
    let root = root();
    let fake = root.0.join("letter.odt");
    std::fs::write(&fake, b"PK not really an office document").unwrap();
    let (view, asked, _) = office(cx, true);
    open(cx, view, fake.to_str().unwrap());
    draw(cx, view);
    assert!(
        asked.borrow().is_empty(),
        "no conversion is asked for a fake container: {:?}",
        asked.borrow()
    );
    let failure = view
        .update(cx, |v, _, _| v.failure(&crate::test_catalogs::english()))
        .unwrap()
        .expect("the fake file draws the unsupported card");
    assert_eq!(
        (failure.sentence.as_str(), failure.glyph, failure.error_tone),
        ("This file cannot be previewed.", "file", false)
    );
    assert_eq!(failure.open_with.as_deref(), Some("Open With\u{2026}"));
}
