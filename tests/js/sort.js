.import "../../ui/js/Sort.js" as Sort

// Sort.column is what ui/Header.qml's click reaches and Sort.next/Sort.reverse are s and S. Nothing
// else in ui/ reaches ui/Backend.qml's sort(), so a wrong gate here is the whole feature.
// What the real backend does per column is asserted in tests/protocol.sh: name, size and mtime are
// reordered with directories first in both directions, and mode and kind come back an error naming
// the key. The header no longer asks for those two, so a click here must send nothing.

// Only the members the sort path touches. The backend stub records the wire in order, because sort
// before window is the protocol's own rule and a window sent first would read the old order's rows.
function pane(sortBy, sortDesc) {
    var p = {
        windowSize: 200,
        total: 40,
        thumbState: "stale",
        dirSizeState: "stale",
        cursor: -1,
        cleared: 0,
        said: [],
        sent: []
    }
    p.message = function (text, isError) { p.said.push(text) }
    p.clearSelection = function () { p.cleared += 1 }
    p.setCursor = function (index) { p.cursor = index }
    p.backend = {
        sortBy: sortBy,
        sortDesc: sortDesc,
        sort: function (by, desc) { p.sent.push("sort " + by + " " + (desc ? "desc" : "asc")) },
        window: function (start, count) { p.sent.push("window " + start + " " + count) }
    }
    return p
}

function run(check) {
    // The mark moves on the click, not on the reply: the OEM optimistic rule this project follows.
    var same = pane("name", false)
    Sort.column(same, "name")
    check("clicking the sorted column asks for the reverse, then the reordered window",
          same.sent.join(","), "sort name desc,window 0 200")
    check("and the recorded direction flips at once, with no round trip", same.backend.sortDesc, true)
    check("and the recorded column stays name", same.backend.sortBy, "name")

    var back = pane("name", true)
    Sort.column(back, "name")
    check("clicking it again returns it to ascending", back.sent.join(","), "sort name asc,window 0 200")
    check("and records ascending", back.backend.sortDesc, false)

    // A re-sort moves every row, so everything keyed by a row index is as stale as a new listing's.
    var reset = pane("name", false)
    Sort.column(reset, "name")
    check("a re-sort drops the thumbnail cache", reset.thumbState === "stale", false)
    check("a re-sort drops the directory-size cache", reset.dirSizeState === "stale", false)
    check("a re-sort clears the selection, whose indices now name other files", reset.cleared, 1)
    check("a re-sort puts the cursor back on the first row", reset.cursor, 0)

    // Size and Date Modified are orders the backend produces, so a click records them the way it
    // records name: the mark, the caches, the selection and the cursor all move on the click.
    var size = pane("name", true)
    Sort.column(size, "size")
    check("a click on Size asks for size ascending, then the reordered window",
          size.sent.join(","), "sort size asc,window 0 200")
    check("and the mark moves onto Size at once, ascending whatever direction name was in",
          size.backend.sortBy + ":" + size.backend.sortDesc, "size:false")
    check("and the thumbnail cache goes with it", size.thumbState === "stale", false)
    check("and the directory-size cache goes with it", size.dirSizeState === "stale", false)
    check("and the selection and the cursor go with it", size.cleared + "|" + size.cursor, "1|0")

    var sizeAgain = pane("size", false)
    Sort.column(sizeAgain, "size")
    check("clicking Size while in size order reverses it", sizeAgain.sent.join(","), "sort size desc,window 0 200")
    check("and records the reverse", sizeAgain.backend.sortDesc, true)

    var date = pane("size", true)
    Sort.column(date, "mtime")
    check("a click on Date Modified asks for mtime ascending, not the reverse it would inherit",
          date.sent.join(","), "sort mtime asc,window 0 200")
    check("and the mark moves onto Date Modified", date.backend.sortBy + ":" + date.backend.sortDesc, "mtime:false")

    // Mode and Kind are labels, not orders. A click must not look like a sort that then errors.
    var mode = pane("name", true)
    Sort.column(mode, "mode")
    check("a click on Mode sends nothing", mode.sent.join(","), "")
    check("a click on Mode leaves the descending name sort alone", mode.backend.sortDesc, true)
    check("and the selection survives a sort that did not happen", mode.cleared, 0)

    var kind = pane("size", false)
    Sort.column(kind, "kind")
    check("a click on Kind sends nothing either", kind.sent.join(","), "")
    check("and the mark stays on the order the listing is really in", kind.backend.sortBy, "size")
    check("and the thumbnail cache is kept, because no row moved", kind.thumbState, "stale")

    // s steps to the next order the backend can produce, always ascending, and wraps: name, size,
    // mtime, name. The column and the direction are separate choices, so s from a reversed name
    // listing lands on size ascending rather than on name ascending.
    var next = pane("name", true)
    Sort.next(next)
    check("s from name steps to size, ascending", next.sent.join(","), "sort size asc,window 0 200")
    check("s records the column it landed on", next.backend.sortBy + ":" + next.backend.sortDesc, "size:false")

    var walked = pane("name", false)
    Sort.next(walked)
    Sort.next(walked)
    Sort.next(walked)
    check("three presses walk size, mtime and back to name",
          walked.sent.join(","),
          "sort size asc,window 0 200,sort mtime asc,window 0 200,sort name asc,window 0 200")
    check("and s says nothing of its own now that every step is a real order", walked.said.length, 0)

    // S reverses whichever order the listing is in, the capital-is-the-variant pair g/G and j/J use.
    var reverse = pane("mtime", false)
    Sort.reverse(reverse)
    check("S reverses the current order", reverse.sent.join(","), "sort mtime desc,window 0 200")
    check("S records the reverse", reverse.backend.sortDesc, true)

    // Asking for the order the listing is already in would drop every cache and the cursor to redraw
    // the same rows, so it is not asked for at all.
    var noop = pane("size", true)
    Sort.resort(noop, "size", true)
    check("asking for the order already shown sends nothing", noop.sent.length, 0)
    check("and leaves the cursor, the caches and the selection alone",
          noop.cursor + "|" + noop.thumbState + "|" + noop.dirSizeState + "|" + noop.cleared, "-1|stale|stale|0")

    // The same guard must not swallow a real reversal, which is the click every column answers.
    var again = pane("name", false)
    Sort.column(again, "name")
    Sort.column(again, "name")
    check("two clicks on the sorted column are two real re-sorts",
          again.sent.join(","),
          "sort name desc,window 0 200,sort name asc,window 0 200")

    // corner: a recorded order this list does not hold cannot wedge s; it wraps to the first one.
    var stray = pane("kind", true)
    Sort.next(stray)
    check("s from an order that is not in the list still lands on name",
          stray.sent.join(","), "sort name asc,window 0 200")
}
