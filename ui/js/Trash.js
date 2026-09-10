.pragma library

.import "Ops.js" as Ops

// The dd pair. A single d used to trash, and it sat in the middle of the letters a name is typed
// with, so the third keystroke of "Vid" trashed the row (issue 7). Delete and Ctrl+Delete are not
// letters and still go on one press, through Ops.trash directly.

// How long the first d stays armed. Long enough to be a deliberate pair, short enough that an arm
// nobody finished cannot be completed by a d typed minutes later. ui/Pane.qml runs a Timer on the
// same figure that clears the stamp, so the rows' armed tint and the pair's window expire together.
var ARM_MS = 1500

// Whether the second d would take this row: the selection when there is one, else the cursor row,
// the rule Ops.targetIndices keeps for every write. The views tint exactly these while the pair is
// armed, so what a d will remove is on screen before it is pressed.
function targeted(pane, index) {
    return pane.selectionCount() > 0 ? pane.isSelected(index) : index === pane.cursorIndex
}

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
