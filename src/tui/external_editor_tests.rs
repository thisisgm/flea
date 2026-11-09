use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn editor_argv_never_expands_shell_text() {
    let dir = TestDir::new("editor-argv");
    let path = dir.file("- file ; $(touch marker)", "original");
    let command = command(Some("'editor program' --wait \"two words\" '$HOME'"), &path).unwrap();
    assert_eq!(command.get_program(), "editor program");
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["--wait".as_ref(), "two words".as_ref(), "$HOME".as_ref(), "--".as_ref(), path.as_os_str()]);
    for editor in [None, Some("'bad"), Some("editor\\"), Some("''")] {
        assert!(super::command(editor, &path).is_err());
    }
    assert!(super::command(Some("editor"), dir.path()).is_err());
    assert_eq!(arguments("editor ; touch $(marker)").unwrap(), ["editor", ";", "touch", "$(marker)"]);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}

// E and bulk rename read one choice: $VISUAL first, then $EDITOR, and a blank value counts as unset.
#[test]
fn the_editor_choice_prefers_visual_then_editor() {
    let some = |text: &str| Some(text.to_string());
    assert_eq!(pick(some("visual"), some("editor")).as_deref(), Some("visual"));
    assert_eq!(pick(None, some("editor")).as_deref(), Some("editor"));
    assert_eq!(pick(some("  "), some("editor")).as_deref(), Some("editor"));
    assert_eq!(pick(some("visual"), None).as_deref(), Some("visual"));
    assert_eq!(pick(some(""), some(" ")), None);
}

// A missing program is named as the command line spells it, the first word without its quotes; one that ran and exited 127 keeps its status.
#[test]
fn a_missing_editor_program_is_named_by_its_first_word() {
    use std::os::unix::process::ExitStatusExt;
    let not_found = std::process::ExitStatus::from_raw(127 << 8);
    let named = |selected: &str| missing(selected, not_found).map(|error| error.to_string());
    assert_eq!(
        named("'/no where/ed' --wait").as_deref(),
        Some("Could not find the program /no where/ed")
    );
    assert_eq!(
        named("/nonexistent/flea-ed").as_deref(),
        Some("Could not find the program /nonexistent/flea-ed")
    );
    assert_eq!(
        named("'bad").as_deref(),
        Some("Could not find the program 'bad")
    );
    assert_eq!(
        named("flea-no-such-editor --wait").as_deref(),
        Some("Could not find the program flea-no-such-editor")
    );
    assert!(
        named("/bin/sh -c 'exit 127'").is_none(),
        "an editor that ran keeps its own 127"
    );
    assert!(named("sh").is_none(), "a program found on PATH ran");
    assert!(missing(
        "/nonexistent/flea-ed",
        std::process::ExitStatus::from_raw(1 << 8)
    )
    .is_none());
}
