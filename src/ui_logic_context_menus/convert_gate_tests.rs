// Convert on one PDF or one office document (EY-563): the menu gate the boards convert-pdf-pages and convert-office need.
use super::build::{listing_entries, Context};
use super::testkit::*;

fn st(change: impl FnOnce(&mut Context)) -> Context {
    let mut p = state();
    change(&mut p);
    p
}

#[test]
fn convert_takes_one_pdf_or_office_document() {
    let document = |count: usize| {
        listing_entries(&st(|p| {
            p.row_is_image = false;
            p.row_is_document = true;
            p.selection_count = count
        }))
    };
    check(
        "one PDF or office document offers Convert",
        label(&english(), entry(&document(1), "convert")),
        "Convert\u{2026}".into(),
    );
    check(
        "two documents offer neither Convert nor Combine Images",
        (
            entry(&document(2), "convert").is_none(),
            entry(&document(2), "combineImages").is_none(),
        ),
        (true, true),
    );
    check(
        "a missing converter removes Convert on a document too",
        entry(
            &listing_entries(&st(|p| {
                p.row_is_image = false;
                p.row_is_document = true;
                p.can_convert = false
            })),
            "convert",
        )
        .is_none(),
        true,
    );
}
