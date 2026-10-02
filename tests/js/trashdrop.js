.import "../../ui/js/Drag.js" as Drag
.import "../../ui/js/TrashDrop.js" as TrashDrop

// A stub pane holding four rows of /d in numbering 7; what its backend is sent lands in sent.
function pane(sent, rows, held) {
    return {
        path: "/d",
        trashedFirst: -1,
        rowFor: function (i) { return (i < 0 || i >= rows.length) ? null : rows[i] },
        join: function (a, b) { return a + "/" + b },
        backend: { dirDev: 42, heldListing: held, send: function (msg) { sent.push(msg) } }
    }
}

// Exercises owned and refused drops, lift-time numbering and cursor landing through the real decisions.
function run(check) {
    var rows = [{ n: "omarchy", d: true }, { n: "a.txt", d: false }, { n: "b.txt", d: false }, { n: "c.txt", d: false }]

    // A drag lifted here carries paths, and the drop trashes exactly those paths.
    var sent = []
    var lifter = pane(sent, rows, 7)
    var wire = Drag.mimeFor(lifter, [1, 3], false)
    var urls = wire["text/uri-list"].split("\r\n")
    check("an own drag with paths is taken", TrashDrop.accepts(wire[Drag.ROWS_MIME], urls, "/d"), true)
    check("the drop trashes those paths", TrashDrop.drop(lifter, wire[Drag.ROWS_MIME], urls), true)
    check("and sends one trash of the carried paths", JSON.stringify(sent),
          JSON.stringify([{ c: "trash", paths: ["/d/a.txt", "/d/c.txt"] }]))
    check("the cursor lands where the first trashed row was", lifter.trashedFirst, 1)
    check("the bar says what the drop will do", TrashDrop.line(wire[Drag.ROWS_MIME], urls, "/d"), "Move 2 items to Trash")

    // Paths survive the listing changing under the drag, so another directory still takes them,
    // but the cursor has no row of its own to land on there.
    var moved = []
    var elsewhere = pane(moved, rows, 9)
    elsewhere.path = "/e"
    check("paths still trash from another listing", TrashDrop.drop(elsewhere, wire[Drag.ROWS_MIME], urls), true)
    check("naming the same files", moved.length === 1 ? moved[0].paths.join(",") : "nothing sent", "/d/a.txt,/d/c.txt")
    check("with no cursor landing in a listing they were not in", elsewhere.trashedFirst, -1)

    // Nothing Flea did not lift here may trash: a foreign drag, another Flea, a drag with no marker.
    var refused = []
    var guard = pane(refused, rows, 7)
    check("a foreign drag is refused", TrashDrop.drop(guard, "", ["file:///d/a.txt"]), false)
    check("another Flea's drag is refused", TrashDrop.drop(guard, "other-flea\n1\nmove\n/d\n42\n7", ["file:///d/a.txt"]), false)
    var gvfs = "/run/user/1000/gvfs/smb-share:server=nas,share=data"
    check("a drag lifted on a GVFS mount is refused, as d is",
          TrashDrop.accepts(Drag.markerPayload([1], false, gvfs, 42, 7), ["file://" + gvfs + "/a.txt"], gvfs), false)
    check("and none of them reached the backend", refused.length, 0)
    check("a refused drag says nothing", TrashDrop.line("", ["file:///d/a.txt"], "/d"), "")

    // A selection too wide to carry paths trashes by index, in the numbering it was lifted in.
    var wide = Drag.markerPayload([3, 0, 2], false, "/d", 42, 7)
    var byIndex = []
    var widePane = pane(byIndex, rows, 8)
    check("a wide drag on its own listing is taken", TrashDrop.drop(widePane, wide, []), true)
    check("as rows named in the numbering of the lift, not the one held now", JSON.stringify(byIndex),
          JSON.stringify([{ c: "trash", rows: [3, 0, 2], listing: 7 }]))
    check("the bar counts the rows the marker carries", TrashDrop.line(wide, [], "/d"), "Move 3 items to Trash")
    check("the cursor lands on the minimum inside an unsorted selection", widePane.trashedFirst, 0)

    // Select All is uncapped; reversed rows put the minimum last and exceed QV4's apply capacity.
    var all = []
    for (var i = 1000000; i > 0; i--) all.push(i + 16)
    var allSent = []
    var allPane = pane(allSent, rows, 8)
    check("an uncapped selection reaches the backend", TrashDrop.drop(allPane, Drag.markerPayload(all, false, "/d", 42, 7), []), true)
    var whole = allSent.length === 1 && allSent[0].c === "trash" && allSent[0].listing === 7
            && allSent[0].rows.length === all.length
    for (var j = 0; whole && j < all.length; j++) whole = allSent[0].rows[j] === all[j]
    check("every selected row keeps its lift-time numbering", whole, true)
    check("the cursor lands on the minimum even when it is last", allPane.trashedFirst, 17)
    var away = pane([], rows, 7)
    away.path = "/e"
    check("a wide drag over another listing cannot name its rows", TrashDrop.drop(away, wide, []), false)
    check("a wide drag whose lift had no numbering is refused",
          TrashDrop.accepts(Drag.markerPayload([1], false, "/d", 42, 0), [], "/d"), false)
    check("a malformed row field is refused whole",
          TrashDrop.accepts(Drag.INSTANCE + "\n1,x\nmove\n/d\n42\n7", [], "/d"), false)
    check("the marker reads its numbering back", Drag.markerListing(wide), 7)
    check("and an older marker reads as none", Drag.markerListing(Drag.INSTANCE + "\n1\nmove\n/d\n42"), 0)
}
