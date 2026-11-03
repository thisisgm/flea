use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn editor_argv_prefers_editor_then_visual_and_never_expands_shell_text() {
    let dir = TestDir::new("editor-argv");
    let path = dir.file("- file ; $(touch marker)", "original");
    let command = command(Some("'editor program' --wait \"two words\" '$HOME'"), Some("ignored"), &path).unwrap();
    assert_eq!(command.get_program(), "editor program");
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["--wait".as_ref(), "two words".as_ref(), "$HOME".as_ref(), "--".as_ref(), path.as_os_str()]);
    assert_eq!(super::command(Some(" "), Some("visual"), &path).unwrap().get_program(), "visual");
    for editor in [None, Some("'bad"), Some("editor\\"), Some("''")] {
        assert!(super::command(editor, None, &path).is_err());
    }
    assert!(super::command(Some("editor"), None, dir.path()).is_err());
    assert_eq!(arguments("editor ; touch $(marker)").unwrap(), ["editor", ";", "touch", "$(marker)"]);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}
