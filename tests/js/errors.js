.import "../../ui/js/Errors.js" as Errors

function run(check) {
    check("a permission denial names access rather than the path",
          Errors.sentence("scan", "Permission denied (os error 13)"),
          "Permission was denied; check access and try again.")
    // The backend's own wording is arbitrary, so the match is case folded before it is looked for.
    check("and it is found whatever case the backend used",
          Errors.sentence("scan", "PERMISSION DENIED"),
          "Permission was denied; check access and try again.")
    check("any other scan failure is the generic directory sentence",
          Errors.sentence("scan", "No such file or directory"),
          "That directory could not be read; check the path and try again.")
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
    check("an unknown origin falls back rather than leaking it",
          Errors.sentence("whatever", "/home/gm/secret/path"),
          "That action could not be completed; try again.")
    // A non-string message must not throw, because the wire can carry a number or null.
    check("a message that is not a string is still one sentence",
          Errors.sentence("scan", null),
          "That directory could not be read; check the path and try again.")

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
          "The copy is complete; the name it came from could not be fully removed and may now be incomplete, so check it before deleting anything.")
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

    // The whole line the pane draws, which is where the mode string and the sentence meet. A denial
    // whose directory the backend could not stat either has no mode string, and a surface that drew
    // nothing at all there would be the blank-frame defect wearing a lock.
    var deniedSentence = "Permission was denied; check access and try again."
    check("a locked pane draws the mode string when there is one",
          Errors.paneLine("locked", deniedSentence, 0o40750), "rwxr-x--- · not yours")
    check("and falls back to the sentence when the stat failed too",
          Errors.paneLine("locked", deniedSentence, 0), deniedSentence)
    check("a mode never leaks into a state that is not locked",
          Errors.paneLine("error", "That directory could not be read; check the path and try again.", 0o40750),
          "That directory could not be read; check the path and try again.")
    check("nothing to say stays nothing, so the surface hides rather than draws a bare mark",
          Errors.paneLine("locked", null, 0), "")
}
