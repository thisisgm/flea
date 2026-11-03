use super::*;
use crate::backend::testdir::TestDir;
use crate::tui::render::frame_tests::plain;
/// Existing path text may contain controls even though typed input is already filtered.
#[test]
fn editor_output_filters_terminal_controls_without_changing_the_value() {
    let value = "a\u{1b}[2Jb\u{7}c\r\nd\t\u{9b}31m\u{202e}txt\u{2066}é";
    let editor = Editor::new("path", value.into(), "/".into());
    let line = plain(&editor.field("", 100, &Theme::from_text("")).0);
    assert!(
        line.chars().all(render::safe),
        "untrusted controls reached terminal output"
    );
    assert!(line.starts_with(&render::clean(value)));
    assert_eq!(
        editor.value, value,
        "rendering must preserve the filesystem spelling"
    );
}

/// Filtering must also hold during rename selection and scrolling without touching the file.
#[test]
fn rename_filters_controls_with_selection_and_after_cursor_movement() {
    let sandbox = TestDir::new("tui-editor-controls");
    let name = "a\u{1b}[2J\u{202e}é.txt";
    let path = sandbox.file(name, "original");
    let mut editor = Editor::rename(name.into(), path.clone(), false).unwrap();
    // The field contributes only trusted SGR; stripping those leaves entirely safe text.
    let theme = Theme::from_text("");
    let line = plain(&editor.field("", 100, &theme).0);
    assert!(line.chars().all(render::safe));
    assert_eq!(editor.value, name);
    assert!(editor.valid());
    for _ in 0..name.chars().count() {
        editor.update(&Key {
            name: "Left".into(),
            text: "".into(),
            mods: "".into(),
            pointer: None,
        });
        let line = plain(&editor.field("", 8, &theme).0);
        assert!(line.chars().all(render::safe));
        assert_eq!(editor.value, name);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}

// Each refused name gets the sentence for its own fault, never one that names a character it does not hold.
#[test]
fn a_refused_name_is_told_the_fault_it_has() {
    for (name, said) in [
        ("a/b", "A name cannot contain /."),
        ("..", "A name cannot be a single dot or a double dot."),
        (".", "A name cannot be a single dot or a double dot."),
        ("nul\0byte", "A name cannot contain a null character."),
    ] {
        let mut editor = Editor::new("mkdir", name.into(), "/".into());
        assert!(!editor.valid(), "{:?}", name);
        assert_eq!(editor.error, said, "{:?}", name);
    }
}

// The typed text is foreground on the fill, the completion follows the caret in the lifted muted ink, and the terminal's caret sits after the text.
#[test]
fn path_field_draws_the_ghost_in_the_extension_ink_after_the_caret() {
    let theme = Theme::from_text("");
    let editor = Editor::new("path", "Me".into(), "/".into());
    let (out, cells, caret) = editor.field("dia/", 40, &theme);
    assert_eq!(
        out,
        format!(
            "\x1b[0m{}{}Me\x1b[0m{}{}dia/ ",
            theme.field, theme.foreground, theme.field, theme.ext
        )
    );
    assert_eq!(
        (cells, caret),
        (7, Some(2)),
        "the board fits Me, dia/ and the caret's cell"
    );
    // No room for the ghost: the text keeps its caret cell and the completion goes.
    assert_eq!(
        editor.field("dia/", 6, &theme),
        (
            format!(
                "\x1b[0m{}{}Me\x1b[0m{}{} ",
                theme.field, theme.foreground, theme.field, theme.ext
            ),
            3,
            Some(2)
        )
    );
}
// Rename opens on the stem: the selection is on the wash, the kept extension in the lifted ink, and no caret shows while text is selected.
#[test]
fn rename_field_selects_the_stem_and_new_folder_selects_its_whole_name() {
    let sandbox = TestDir::new("tui-editor-field");
    let path = sandbox.file("notes.txt", "original");
    let theme = Theme::from_text("");
    let editor = Editor::rename("notes.txt".into(), path.clone(), false).unwrap();
    let (out, cells, caret) = editor.field("", 100, &theme);
    assert_eq!(
        out,
        format!(
            "\x1b[0m{}{}notes\x1b[0m{}{}.txt\x1b[0m{}{} ",
            theme.stem, theme.foreground, theme.field, theme.ext, theme.field, theme.ext
        )
    );
    assert_eq!((cells, caret), (10, None));
    let folder = Editor::named("mkdir", "New Folder".into(), "/".into());
    let (out, cells, caret) = folder.field("", 100, &theme);
    assert_eq!(
        out,
        format!(
            "\x1b[0m{}{}New Folder\x1b[0m{}{} ",
            theme.stem, theme.foreground, theme.field, theme.ext
        )
    );
    assert_eq!((cells, caret), (11, None));
    let mut typed = Editor::named("mkdir", "New Folder".into(), "/".into());
    typed.update(&Key::character('é', ""));
    assert_eq!(
        (typed.value.as_str(), typed.field("", 100, &theme).2),
        ("é", Some(1)),
        "typing replaces the whole selected name"
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}
// A wide or long name scrolls so the caret stays inside the field, and the field never draws past its columns.
#[test]
fn field_scrolls_unicode_to_keep_the_caret_inside_its_columns() {
    let theme = Theme::from_text("");
    let editor = Editor::new("filter", "新しいフォルダ名前".into(), "/".into());
    for columns in 1..24 {
        let (out, cells, caret) = editor.field("", columns, &theme);
        assert_eq!(
            super::super::cells::text_width(&plain(&out)),
            cells,
            "{columns}"
        );
        assert!(cells <= columns, "{columns}: {cells}");
        assert!(caret.is_some_and(|at| at < columns), "{columns}: {caret:?}");
        // The caret ends the value, so the scrolled field keeps the last character just before it.
        if columns >= 3 {
            assert!(
                plain(&out).trim_end().ends_with('前'),
                "{columns}: {:?}",
                plain(&out)
            );
        }
    }
    assert_eq!(editor.field("", 0, &theme), (String::new(), 0, None));
}
#[test]
fn rename_preserves_extension_unicode_and_changed_identity() {
    let sandbox = TestDir::new("tui-editor");
    let path = sandbox.file("café.txt", "original");
    let mut editor = Editor::rename("café.txt".into(), path.clone(), false).unwrap();
    editor.update(&Key::character('新', ""));
    assert_eq!(editor.value, "新.txt");
    editor.update(&Key::named("Left", ""));
    editor.update(&Key::named("Delete", ""));
    assert_eq!(editor.value, ".txt");
    editor.replace("../escape".into());
    assert!(!editor.valid());
    editor.replace("renamed.txt".into());
    assert!(editor.valid());
    let moved = sandbox.join("old");
    assert!(
        path.is_absolute() && path.starts_with(sandbox.path()) && moved.starts_with(sandbox.path())
    );
    std::fs::rename(&path, moved).unwrap();
    sandbox.file("café.txt", "replacement");
    assert!(!editor.valid());
}
