//! An office preview converts the document's first pages only, and its count says so.
use super::*;
use crate::ui_logic_pv2::viewer::{count_text, cut};

#[test]
fn a_preview_ready_line_carries_its_cut_and_a_new_subject_forgets_it() {
    let mut office = Office::default();
    office.open("/d/long.docx", 1);
    let ready = r#"{"t":"officepdf","id":1,"state":"ready","path":"/c/k.head30.pdf","first":30}"#;
    let parsed = parse(ready).expect("a ready line parses");
    assert_eq!(
        parsed.wire,
        Wire::Pdf {
            path: "/c/k.head30.pdf".into(),
            first: Some(30)
        }
    );
    assert_eq!(
        office.reply(&parsed),
        Applied::Pdf("/c/k.head30.pdf".into())
    );
    assert_eq!(office.first, Some(30));
    office.open("/d/short.docx", 2);
    assert_eq!(office.first, None, "the next subject starts with no cut");
}

#[test]
fn a_count_that_reaches_the_cut_reads_with_a_plus_and_a_whole_document_never_does() {
    assert_eq!(
        count_text(30, Some(30)),
        "30+",
        "the document may run past its first 30 pages"
    );
    assert_eq!(
        count_text(12, Some(30)),
        "12",
        "a preview that stops short of the cut is the whole document"
    );
    assert_eq!(
        count_text(300, None),
        "300",
        "Print's whole PDF and a plain PDF count exactly"
    );
    assert!(cut(31, Some(30)) && !cut(0, Some(30)) && !cut(30, None));
}
