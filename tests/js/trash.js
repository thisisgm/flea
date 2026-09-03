.import "../../ui/js/Trash.js" as Trash
.import "../../ui/js/Ops.js" as Ops

// The dd pair, issue 7. A single d used to trash and sat among the letters a name is typed with, so
// the third keystroke of "Vid" trashed the row. These drive Trash.arm directly, because the stamp
// and the window are the whole of the policy and neither needs a window to be true.

// What Ops.trash reaches for, plus the stamp Trash.arm reads and writes.
function pane() {
    var p = {
        trashArmedAt: 0,
        cursorIndex: 3,
        trashedIdx: [],
        said: "",
        selectedIndices: function () { return [] }
    }
    p.message = function (text, isError) { p.said = text }
    p.backend = { trash: function (idx) { p.trashedIdx = idx } }
    return p
}

function run(check) {
    var p = pane()
    Trash.arm(p)
    check("the first d trashes nothing and says what to press", p.trashedIdx.length, 0)
    check("and the sentence names both routes", p.said,
          "Press d again to trash, or Delete on its own.")
    check("and it leaves the pair armed", p.trashArmedAt > 0, true)
    Trash.arm(p)
    check("the second d inside the window trashes the cursor row", p.trashedIdx.join(","), "3")
    check("and disarms, so a third d only arms again", p.trashArmedAt, 0)

    // A stamp older than the window is not half a pair, however long it has stood.
    var stale = pane()
    stale.trashArmedAt = Date.now() - 60000
    Trash.arm(stale)
    check("a d a minute after the first is a fresh arm, not the second of a pair",
          stale.trashedIdx.length, 0)
    check("and it re-arms rather than completing a pair nobody meant",
          stale.trashArmedAt > Date.now() - 1000, true)

    // Delete is not a letter a name is typed with, so it never armed and still goes on one press.
    var direct = pane()
    Ops.trash(direct)
    check("Delete trashes on one press, with no arming", direct.trashedIdx.join(","), "3")
    check("and it leaves no arm behind it", direct.trashArmedAt, 0)

    // The selection wins over the cursor, the rule Ops.targetIndices keeps for every write.
    var picked = pane()
    picked.selectedIndices = function () { return [1, 4] }
    picked.trashArmedAt = Date.now()
    Trash.arm(picked)
    check("the pair trashes the selection when there is one", picked.trashedIdx.join(","), "1,4")
}
