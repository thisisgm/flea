use super::super::{Office, Reason, State};

#[test]
fn office_a_fake_container_is_unsupported_before_any_conversion() {
    let mut office = Office::default();
    office.open("/d/letter.odt", 0);
    office.reject_container();
    assert_eq!(
        office.state,
        State::Unsupported(Reason::NotOffice),
        "a fake container falls to the unsupported card"
    );
    assert_eq!(office.id, 0, "no conversion request is in flight for it");
    let p = office.projection().expect("the unsupported card projects");
    assert_eq!(
        (p.glyph, p.error_tone, p.key, p.open_with),
        ("file", false, "document.office_unsupported", true)
    );
}
