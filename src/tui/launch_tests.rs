use super::*;

// A helper that is not there is named in the sentence; any other spawn failure keeps its own cause.
#[test]
fn a_missing_helper_program_is_named_in_a_sentence() {
    let error = detached(Command::new("/nonexistent/flea-helper")).unwrap_err();
    assert_eq!(
        crate::error::io_message(&error),
        "Could not find the program /nonexistent/flea-helper"
    );
    let terminal = Command::new("xdg-terminal-exec");
    let named = missing(io::Error::from(io::ErrorKind::NotFound), &terminal);
    assert_eq!(
        crate::error::io_message(&named),
        "Could not find the program xdg-terminal-exec"
    );
    let denied = missing(io::Error::from(io::ErrorKind::PermissionDenied), &terminal);
    assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);
}
