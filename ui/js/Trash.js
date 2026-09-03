.pragma library

.import "Ops.js" as Ops

// The dd pair. A single d used to trash, and it sat in the middle of the letters a name is typed
// with, so the third keystroke of "Vid" trashed the row (issue 7). Delete and Ctrl+Delete are not
// letters and still go on one press, through Ops.trash directly.

// How long the first d stays armed. Long enough to be a deliberate pair, short enough that an arm
// nobody finished cannot be completed by a d typed minutes later.
var ARM_MS = 1500

// ui/js/Focus.js clears pane.trashArmedAt for every other action, so an arm never survives the key
// after it; this reads the stamp before it writes one, which is what makes the second press the pair.
function arm(pane) {
    if (pane.trashArmedAt > 0 && Date.now() - pane.trashArmedAt < ARM_MS) {
        pane.trashArmedAt = 0
        Ops.trash(pane)
        return
    }
    pane.trashArmedAt = Date.now()
    pane.message("Press d again to trash, or Delete on its own.", false)
}
