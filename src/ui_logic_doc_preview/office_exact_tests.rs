// A document whose name is not UTF-8 keeps its group, beside office_tests.rs at its ceiling.
use super::*;

#[test]
fn a_raw_named_document_keeps_its_group() {
    use std::os::unix::ffi::OsStrExt;
    let exact = crate::bytename::path_text(std::path::Path::new(std::ffi::OsStr::from_bytes(
        b"/d/r\xff.docx",
    )))
    .into_owned();
    assert_eq!(
        Group::of(&exact),
        Some(Group::Document),
        "the extension sits before the exact tail"
    );
}
