.import "../../ui/js/Errors.js" as Errors

/** Exercise user-facing error mappings with the Qt test runner's equality assertion callback. */
function run(check) {
    // StatusBar board rule 4, both refusal lanes: a refused hop names the directory that refused,
    // and a refusal of the directory already on screen names nothing, because the breadcrumb does.
    check("a refused hop names the directory that refused",
          Errors.sentence("scan", "Permission denied (os error 13)", "inner"),
          "Permission denied on inner")
    check("and a refusal of the path on screen is the refusal and nothing else",
          Errors.sentence("scan", "Permission denied (os error 13)", ""),
          "Permission denied")
    // The backend's own wording is arbitrary, so the match is case folded before it is looked for.
    check("and it is found whatever case the backend used",
          Errors.sentence("scan", "PERMISSION DENIED", "inner"),
          "Permission denied on inner")
    check("any other refused hop names its directory too",
          Errors.sentence("scan", "No such file or directory", "gone"),
          "That directory could not be read: gone")
    check("and the bare sentence is what is left without one",
          Errors.sentence("scan", "No such file or directory"),
          "That directory could not be read.")
    // Size and mtime are real orders now, so the one refusal left is a key the wire never defined.
    check("a column that is no sort key at all is refused in the operator's words",
          Errors.sentence("sort", "no such sort key; send name, size or mtime"),
          "Sorting by that column is not available.")
    check("and a sort refusal this UI has never seen reads the same",
          Errors.sentence("sort", "some later refusal"),
          "Sorting by that column is not available.")
    check("a read failure says the backend stopped",
          Errors.sentence("read", "EOF"),
          "The backend stopped responding; reopen Flea and try again.")
    // The state file's own refusal: the change is still on screen, so the sentence says what did not last.
    check("a refused ui.json write says the setting did not last",
          Errors.sentence("state", ""),
          "That setting could not be saved.")
    // And the other way round: a ui.json main() could not read is left alone, so nothing in it is used.
    check("an unreadable ui.json says the defaults are what is on screen",
          Errors.sentence("statefile", ""),
          "Your saved settings could not be read, so these are the defaults.")
    check("an unknown origin falls back rather than leaking it",
          Errors.sentence("whatever", "/home/gm/secret/path"),
          "That action could not be completed; try again.")
    // A non-string message must not throw, because the wire can carry a number or null.
    check("a message that is not a string is still one sentence",
          Errors.sentence("scan", null),
          "That directory could not be read.")

    // The write operations, whose failures the operator is about to act on rather than just read.
    check("an empty journal reads back as the backend's own sentence",
          Errors.sentence("undo", "there is nothing to undo"),
          "There is nothing to undo.")
    check("an undo sentence that already ends in a stop does not gain a second one",
          Errors.sentence("undo", "That is gone."),
          "That is gone.")
    check("an empty undo message still yields a sentence rather than a bare stop",
          Errors.sentence("undo", ""),
          "That action could not be completed; try again.")
    check("a rename onto a taken name says which problem it is",
          Errors.sentence("rename", "File exists (os error 17)"),
          "A file with that name is already here.")
    check("and any other rename failure stays generic rather than leaking errno",
          Errors.sentence("rename", "Permission denied (os error 13)"),
          "That file could not be renamed.")

    // The sentence promises the copy, warns the other name may be incomplete, and names no direction.
    check("a rename that kept its copy says so, with no path and no errno",
          Errors.sentence("rename-kept", "Permission denied (os error 13)"),
          "Copied, but the old name was only partly removed. Check it.")
    check("that sentence never leaks the errno",
          Errors.sentence("rename-kept", "Permission denied (os error 13)").indexOf("os error") < 0,
          true)
    check("and it never tells an operator who pressed undo that something was renamed",
          Errors.sentence("rename-kept", "Permission denied (os error 13)").indexOf("Renamed") < 0,
          true)
    check("a duplicate failure names the operation",
          Errors.sentence("duplicate", "every copy name is taken"),
          "That file could not be duplicated.")
    check("a trash failure names the operation",
          Errors.sentence("trash", "gio missing"),
          "That could not be moved to Trash.")
    // src/backend/rowguard.rs: rows read from a replaced listing were refused before anything resolved.
    check("a stale refusal says nothing was done, not that something failed partway",
          Errors.sentence("stale", "the listing changed before this request arrived"),
          "The listing changed before that arrived, so nothing was done.")
    check("a transfer failure reads back as the backend's own sentence",
          Errors.sentence("transfer", "the destination is not a directory"),
          "The destination is not a directory.")

    // src/backend/ops.rs words the collision "a folder or file with that name already exists", while
    // rename's own predicate looks for the errno's "file exists". A branch reusing that spelling
    // never fires and falls through to the catch-all, which reads plausibly and proves nothing.
    check("a folder onto a taken name says which problem it is",
          Errors.sentence("mkdir", "a folder or file with that name already exists"),
          "A folder or file with that name is already here.")
    check("a rename onto a taken name still matches the errno spelling it gets",
          Errors.sentence("rename", "File exists (os error 17)"),
          "A file with that name is already here.")
    check("any other mkdir failure reaches the mkdir branch, not the catch-all",
          Errors.sentence("mkdir", "Permission denied (os error 13)"),
          "That folder could not be created.")
    // src/error.rs from_io passes std::io::Error::to_string through verbatim, so mkdir cannot
    // capitalise its message the way transfer does without printing an errno at the operator.
    check("and it never leaks the errno the backend passed through",
          Errors.sentence("mkdir", "Permission denied (os error 13)").indexOf("os error") >= 0,
          false)

    // The Locked pane state, which States.dc.html draws as the lock mark over the directory's own
    // mode string. It is a listing denial and nothing else: every other failure stays generic Error.
    check("a denied listing is the canvas's Locked state, not the generic error",
          Errors.listingState("scan", "Permission denied (os error 13)"),
          "locked")
    check("and it is found whatever case the backend used, as the sentence is",
          Errors.listingState("scan", "PERMISSION DENIED"),
          "locked")
    check("a directory that is simply missing is still the generic error",
          Errors.listingState("scan", "No such file or directory"),
          "error")
    check("a denial that is not a listing never reaches the pane as Locked",
          Errors.listingState("rename", "Permission denied (os error 13)"),
          "error")
    check("a message that is not a string classifies rather than throwing",
          Errors.listingState("scan", null),
          "error")

    // The mode string the Locked surface draws. /root on this box is 0o40750, and the canvas's own
    // mock is 0o40700; an owner who could both list and enter would not have been denied, so those
    // two prove the operator is not the owner and the line says so.
    check("the measured mode of /root reads back as its own string",
          Errors.lockedLine(0o40750), "rwxr-x--- · not yours")
    check("the canvas's own mock renders verbatim",
          Errors.lockedLine(0o40700), "rwx------ · not yours")
    check("a directory nobody can list claims no owner, because it could be yours",
          Errors.lockedLine(0o40000), "---------")
    check("an owner who cannot enter could be you, so the line claims nothing",
          Errors.lockedLine(0o40640), "rw-r-----")
    check("an owner who cannot list could be you either",
          Errors.lockedLine(0o40300), "-wx------")
    // src/backend/meta.rs answers mode 0 when the stat itself failed, and a real st_mode always
    // carries its file-type bits, so a zero is "I could not look" and never a mode of 000.
    check("the backend's stat-failed zero draws no line rather than a false 000",
          Errors.lockedLine(0), "")
    check("and a mode that never arrived draws none either",
          Errors.lockedLine(undefined), "")

    // The pane block draws the sentence and the mode as two lines now, so the only thing left to
    // decide is whether there is a mode to draw at all; States board rule 3.

    // The credentialed mount can identify the timeout and shell statuses without reading GIO text.
    check("the helper's own deadline names the host, not the credential",
          Errors.connectFailure(124, "smb://host/share"), "Connect failed: host did not respond")
    check("a helper that could not be run at all says so",
          Errors.connectFailure(127, "smb://host/share"), "Connect failed: authentication helper is unavailable")
    // The helper suppresses GIO's text and returns its status; 1 cannot tell a bad password
    // from a TLS failure, a bad path or an unreachable share. The URI is not diagnostic evidence.
    for (var uri of ["davs://host/dav", "dav://host/dav", "ftps://host/", "ftp://host/",
                     "smb://host/share", "sftp://host/", "DAVS://host/dav", "", null]) {
        for (var code of [1, 2, 42]) {
            check("an unclassified mount failure stays neutral: " + uri + " status " + code,
                  Errors.connectFailure(code, uri), "Connect failed: the network location could not be opened")
        }
        check("a non-executable helper is named for every scheme: " + uri,
              Errors.connectFailure(126, uri), "Connect failed: authentication helper is unavailable")
        check("a missing helper is named for every scheme: " + uri,
              Errors.connectFailure(127, uri), "Connect failed: authentication helper is unavailable")
        check("a timeout is named for every scheme: " + uri,
              Errors.connectFailure(124, uri), "Connect failed: host did not respond")
    }
    check("the refusal does not echo URI credentials or paths",
          Errors.connectFailure(1, "davs://user:secret@host/private"),
          "Connect failed: the network location could not be opened")
}
