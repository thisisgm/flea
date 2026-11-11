//! The backend's own words as a sentence: which first words are raised and which names keep their case.
use super::english;
use crate::ui_logic_status_feedback::errors::{capitalised, sentence};

/// S1B-4: a backend error that opens on a file name keeps the name's own case; a plain first word is still raised.
#[test]
fn a_leading_file_name_keeps_its_case_and_a_plain_word_is_raised() {
    assert_eq!(
        capitalised("link/evil.txt: Cannot extract through symlink link/evil.txt"),
        "link/evil.txt: Cannot extract through symlink link/evil.txt."
    );
    assert_eq!(capitalised("notes.txt is in use"), "notes.txt is in use.");
    assert_eq!(
        capitalised("the clipboard could not be read"),
        "The clipboard could not be read."
    );
    assert_eq!(capitalised("can't write here"), "Can't write here.");
    assert_eq!(
        capitalised("can\u{2019}t write here"),
        "Can\u{2019}t write here."
    );
    assert_eq!(capitalised("read-only drive"), "Read-only drive.");
    assert_eq!(
        capitalised("notes: permission denied"),
        "notes: permission denied.",
        "the backend's name: reason form keeps an extensionless name's case"
    );
    assert_eq!(
        sentence(
            "archive",
            "link/evil.txt: Cannot extract through symlink",
            ""
        )
        .render(&english()),
        "link/evil.txt: Cannot extract through symlink."
    );
}
