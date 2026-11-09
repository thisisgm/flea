use super::english;
use crate::ui_logic_fb1::errors::{
    clause, connect_failure, extract_refusal, listing_state, locked_line, os_reason, passthrough,
    sentence, ListingState,
};

fn said(place: &str, message: &str, named: &str) -> String {
    sentence(place, message, named).render(&english())
}

#[test]
fn a_scan_refusal_names_the_directory_that_refused_and_the_board_sentence_otherwise() {
    assert_eq!(
        said("scan", "Permission denied (os error 13)", "inner"),
        "inner cannot be opened: you do not have permission."
    );
    // The board's one sentence replaces the frozen colon form, in the pane and the status bar alike.
    assert_eq!(
        said("scan", "Permission denied (os error 13)", ""),
        "You don’t have permission to open this folder."
    );
    assert_eq!(
        said("scan", "PERMISSION DENIED", "inner"),
        "inner cannot be opened: you do not have permission."
    );
    assert_eq!(
        said("scan", "No such file or directory", "gone"),
        "gone could not be read."
    );
    assert_eq!(
        said("scan", "No such file or directory", ""),
        "That folder could not be read."
    );
    assert_eq!(said("scan", "", ""), "That folder could not be read.");
}

#[test]
fn a_dead_share_names_itself() {
    assert_eq!(
        said("scan", "file is no longer available", "gone"),
        "That share changed, so this folder is no longer available."
    );
    assert_eq!(
        said("scan", "not connected", "gone"),
        "That share is disconnected; remount it."
    );
    assert_eq!(
        said("scan", "operation timed out", "gone"),
        "That share is not responding."
    );
}

#[test]
fn fixed_origins_read_their_own_sentence() {
    let sort = "Sorting by that column is not available.";
    assert_eq!(
        said("sort", "no such sort key; send name, size or mtime", ""),
        sort
    );
    assert_eq!(said("sort", "some later refusal", ""), sort);
    assert_eq!(
        said("read", "EOF", ""),
        "Flea stopped responding. Reopen Flea and try again."
    );
    assert_eq!(said("state", "", ""), "That setting could not be saved.");
    assert_eq!(
        said("statefile", "", ""),
        "Your saved settings could not be read, so these are the defaults."
    );
    assert_eq!(
        said("whatever", "/home/gm/secret/path", ""),
        "That action could not be completed; try again."
    );
    assert_eq!(
        said(
            "stale",
            "the folder changed before this could run, so nothing was done",
            ""
        ),
        "The listing changed before that arrived, so nothing was done."
    );
}

#[test]
fn write_operations_keep_the_backend_sentence_as_a_diagnostic_argument() {
    assert_eq!(
        said("undo", "there is nothing to undo", ""),
        "There is nothing to undo."
    );
    assert_eq!(said("undo", "That is gone.", ""), "That is gone.");
    assert_eq!(
        said("undo", "", ""),
        "That action could not be completed; try again."
    );
    assert_eq!(
        said("window", "/hung is not responding.", ""),
        "/hung is not responding."
    );
    assert_eq!(
        said("window", "", ""),
        "That action could not be completed; try again."
    );
    assert_eq!(
        said("transfer", "the destination is not a folder", ""),
        "The destination is not a folder."
    );
    assert_eq!(
        said("archive", "that archive is encrypted", ""),
        "That archive is encrypted."
    );
    assert_eq!(
        said("link", "this drive cannot hold links", ""),
        "This drive cannot hold links."
    );
    assert_eq!(
        said(
            "rename-stranded",
            "the file was left as .flea-case-1-0 in this folder: input/output failed",
            ""
        ),
        "The file was left as .flea-case-1-0 in this folder: input/output failed."
    );
    let backend = sentence("transfer", "the destination is not a folder", "");
    assert_eq!(backend.key, "feedback.error.backend");
    assert_eq!(backend.args, ["The destination is not a folder."]);
}

#[test]
fn rename_and_mkdir_say_which_problem_without_errno() {
    assert_eq!(
        said("rename", "File exists (os error 17)", ""),
        "An item with that name is already here."
    );
    assert_eq!(
        said("rename", "Permission denied (os error 13)", ""),
        "That file could not be renamed."
    );
    assert_eq!(
        said(
            "rename",
            "':' is not allowed in a name on this vfat drive",
            ""
        ),
        "':' is not allowed in a name on this vfat drive."
    );
    assert_eq!(
        said("mkdir", "'CON' is reserved on this vfat drive", ""),
        "'CON' is reserved on this vfat drive."
    );
    assert_eq!(
        said("rename", "the drive is read-only", ""),
        "The drive is read-only."
    );
    assert_eq!(
        said("mkdir", "the drive is read-only", ""),
        "The drive is read-only."
    );
    assert_eq!(
        said(
            "mkdir",
            "a folder or file with that name already exists",
            ""
        ),
        "An item with that name is already here."
    );
    let mkdir = said("mkdir", "Permission denied (os error 13)", "");
    assert_eq!(mkdir, "That folder could not be created.");
    assert!(!mkdir.contains("os error"));
}

#[test]
fn a_kept_rename_names_no_path_errno_or_direction() {
    let kept = said("rename-kept", "Permission denied (os error 13)", "");
    assert_eq!(
        kept,
        "Copied to the new name, but the original was only partly removed. Check the original."
    );
    assert!(!kept.contains("os error") && !kept.contains("Renamed"));
}

#[test]
fn duplicate_trash_and_a_stopped_archive_name_the_operation() {
    assert_eq!(
        said("duplicate", "every copy name is taken", ""),
        "That file could not be duplicated."
    );
    assert_eq!(
        said("trash", "gio missing", ""),
        "That item could not be moved to Trash."
    );
    assert_eq!(
        said("trash", "the drive is read-only", ""),
        "The drive is read-only."
    );
    assert_eq!(
        said("archive", "the backend is not running", "doc.zip"),
        "doc.zip could not be opened because the Flea backend stopped; reopen Flea and try again."
    );
    assert_eq!(
        said("archive", "the backend is not running", ""),
        "This archive could not be opened because the Flea backend stopped; reopen Flea and try again."
    );
}

#[test]
fn fixed_os_causes_have_their_own_keys_and_longer_text_stays_a_diagnostic() {
    for (words, key) in [
        ("item not found", "feedback.os.not_found"),
        ("permission denied", "feedback.os.permission_denied"),
        ("already exists", "feedback.os.already_exists"),
        ("connection refused", "feedback.os.connection_refused"),
        ("not connected", "feedback.os.not_connected"),
        ("operation timed out", "feedback.os.timed_out"),
        ("not enough memory", "feedback.os.out_of_memory"),
        (
            "the drive could not read or write the data",
            "feedback.os.io",
        ),
        ("the drive is full", "feedback.os.full"),
        ("the drive is read-only", "feedback.os.read_only"),
        ("file is no longer available", "feedback.os.stale"),
        (
            "the operation could not finish",
            "feedback.os.could_not_finish",
        ),
    ] {
        assert_eq!(os_reason(words), Some(key), "{words}");
        let catalog = english();
        assert_ne!(catalog.tr(key), key, "{key} has English");
    }
    assert_eq!(os_reason("could not copy: the drive is full"), None);
    assert_eq!(passthrough("the drive is full").key, "feedback.os.full");
}

#[test]
fn only_a_listing_denial_is_locked() {
    assert_eq!(
        listing_state("scan", "Permission denied (os error 13)"),
        ListingState::Locked
    );
    assert_eq!(
        listing_state("scan", "PERMISSION DENIED"),
        ListingState::Locked
    );
    assert_eq!(
        listing_state("scan", "No such file or directory"),
        ListingState::Error
    );
    assert_eq!(
        listing_state("rename", "Permission denied (os error 13)"),
        ListingState::Error
    );
    assert_eq!(listing_state("scan", ""), ListingState::Error);
}

#[test]
fn the_locked_line_claims_another_owner_only_when_the_mode_proves_it() {
    let catalog = english();
    assert_eq!(locked_line(0o40750, &catalog), "rwxr-x--- · not yours");
    assert_eq!(locked_line(0o40700, &catalog), "rwx------ · not yours");
    assert_eq!(locked_line(0o40000, &catalog), "---------");
    assert_eq!(locked_line(0o40640, &catalog), "rw-r-----");
    assert_eq!(locked_line(0o40300, &catalog), "-wx------");
    assert_eq!(locked_line(0, &catalog), "");
    assert_eq!(locked_line(0o41777, &catalog), "rwxrwxrwt · not yours");
    assert_eq!(locked_line(0o46640, &catalog), "rwSr-S---");
}

#[test]
fn a_mount_helper_status_names_its_cause_and_never_echoes_the_uri() {
    let catalog = english();
    let say = |code| catalog.tr(connect_failure(code));
    assert_eq!(say(124), "Could not connect: the server did not respond.");
    assert_eq!(
        say(126),
        "Could not connect: Flea could not ask for the password."
    );
    assert_eq!(
        say(127),
        "Could not connect: Flea could not ask for the password."
    );
    assert_eq!(
        say(77),
        "Could not connect: the server did not accept the password."
    );
    for code in [1, 2, 42, 0, -1] {
        assert_eq!(
            say(code),
            "Could not connect: the network location could not be opened."
        );
    }
    assert!(!say(1).contains("secret") && !say(1).contains("://"));
}

#[test]
fn an_empty_paste_image_answers_the_empty_clipboard_sentence() {
    assert_eq!(
        said("pasteimage", "the clipboard holds no image", ""),
        "There is nothing to paste; y copies and x cuts."
    );
    assert_eq!(
        said("pasteimage", "the clipboard could not be read", ""),
        "The clipboard could not be read."
    );
}

#[test]
fn an_extract_refusal_reads_back_its_key_and_names_whatever_quotes_the_names_hold() {
    let twins = |message: &str| extract_refusal(message).map(|refusal| (refusal.key, refusal.args));
    assert_eq!(
        twins("'x' and 'y' and 'X' and 'Y' would take one name on this drive, so nothing was extracted"),
        Some(("archive.done.twins", vec!["x' and 'y".to_owned(), "X' and 'Y".to_owned()])),
        "the split is where the two names fold to one"
    );
    assert_eq!(
        said("archive", "'a: b' cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive", ""),
        "“a: b” cannot be extracted onto this drive: ':' is not allowed in a name on this vfat drive."
    );
    assert_eq!(extract_refusal("A.zip is corrupt"), None);
    assert_eq!(
        clause("the drive is read-only", &english()),
        "the drive is read-only",
        "English keeps the backend's own case inside a sentence"
    );
    assert_eq!(clause("gio: no trash", &english()), "gio: no trash");
}
