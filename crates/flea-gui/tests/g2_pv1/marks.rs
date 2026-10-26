//! The bar's mark: a code file wears its language logo while language marks are on, every other file its kind's mark.
use flea::ui_logic_pv1::{kinds::State, rect::Shape};
use flea_gui::g2::pv1::{Body, Subject};

fn file(name: &str, icon: &str, lang_marks: bool) -> Subject {
    Subject {
        name: name.into(),
        icon: icon.into(),
        mode: 0o100644,
        lang_marks,
        state: State::Code,
        shape: Shape::unknown(State::Code),
        body: Body::Text {
            lines: Vec::new(),
            count: String::new(),
        },
    }
}

/// The key of the mark the bar draws for a subject.
fn key(subject: &Subject) -> String {
    subject.mark().svg().0.to_string()
}

#[test]
fn the_bar_wears_each_code_files_language_logo() {
    for (name, icon, logo) in [
        ("main.rs", "text-x-rust", "rust"),
        ("app.py", "text-x-python", "python"),
        ("app.js", "text-x-javascript", "javascript"),
        ("run.sh", "text-x-script", "gnubash"),
    ] {
        assert_eq!(
            key(&file(name, icon, true)),
            format!("flea-lang:{logo}"),
            "{name} wears its language logo"
        );
    }
}

#[test]
fn a_file_without_a_language_keeps_its_kind_mark() {
    assert_eq!(
        key(&file("notes.txt", "text-plain", true)),
        "flea-mark:file-text"
    );
}

#[test]
fn language_marks_off_keeps_the_kind_mark() {
    assert_eq!(
        key(&file("main.rs", "text-x-rust", false)),
        "flea-mark:file"
    );
}
