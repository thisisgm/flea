use super::*;

#[test]
fn office_types_include_legacy_documents_and_are_found_by_canonical_name() {
    let mime = Db::from_str("50:application/vnd.oasis.opendocument.text:*.odt\n50:text/rtf:*.rtf\n50:application/pdf:*.pdf\n50:application/msword:*.doc\n50:application/vnd.ms-excel:*.xls\n50:application/vnd.ms-powerpoint:*.ppt\n50:application/vnd.oasis.opendocument.presentation:*.odp\n");
    let aliases = Aliases::from_str("text/rtf application/rtf\n");
    assert!(is_office("/x/a.odt", &mime, &aliases));
    assert!(is_office("/x/a.rtf", &mime, &aliases), "an alias is canonicalised first");
    assert!(!is_office("/x/a.pdf", &mime, &aliases));
    assert!(is_office("/x/a.doc", &mime, &aliases), "legacy office formats convert too");
    for name in ["a.xls", "a.ppt", "a.odp"] { assert!(is_office(name, &mime, &aliases)); }
    assert!(!is_office("/x/a.unknown", &mime, &aliases));
}
