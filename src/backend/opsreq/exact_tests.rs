// A transfer item names a file that is not UTF-8 by its lossy spelling, kept apart so tests.rs stays inside its ceiling.
use super::super::transferitem_line;

#[test]
fn an_item_line_shows_a_raw_name_in_its_display_spelling() {
    use flea::bytename::ExactText as _;
    use std::os::unix::ffi::OsStrExt;
    let exact = std::ffi::OsStr::from_bytes(b"clip-\xff.txt")
        .exact_text()
        .into_owned();
    for ok in [true, false] {
        let line = transferitem_line(12, 0, &exact, ok, "denied");
        assert_eq!(
            crate::json::field_str(&line, "name").as_deref(),
            Some("clip-\u{fffd}.txt"),
            "{line}"
        );
    }
}
