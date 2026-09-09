.import "../../ui/js/PickerWire.js" as Wire
.import "../../ui/js/Ops.js" as Ops
.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/PickerOps.js" as PickerOps

// The chooser's state as ui/PickerWire.qml's handlers read it: a window of four rows from listing
// index 2, the cursor on c.png, and every write recorded so a reply can be checked without a window.
function stubState(over) {
    var rows = [
        { n: "a.png", d: false, s: 10 },
        { n: "sub", d: true, s: 0 },
        { n: "c.png", d: false, s: 30 },
        { n: "d.txt", d: false, s: 40 }
    ]
    var state = {
        path: "/d",
        held: 2,
        rows: rows,
        cursorIndex: 4,
        marks: [],
        transfer: Ops.emptyTransfer(),
        renameOnArrival: "",
        renameFromPath: "",
        renameListingPath: "",
        renamePointerPath: "",
        renamingIndex: -1,
        listingState: "ready",
        relisted: "",
        seated: null,
        said: [],
        errors: [],
        stuck: [],
        selectedIndices: function () { return PickerOps.indicesFor(state) },
        rowFor: function (i) { return (i - 2 < 0 || i - 2 >= rows.length) ? null : rows[i - 2] },
        join: function (base, name) { return Picker.rowPath(base, name) },
        message: function (text, isError) { state.said.push(text); state.errors.push(isError) },
        sticky: function (text) { state.stuck.push(text) },
        openWithoutHistory: function (next) { state.relisted = next },
        navigate: { seat: function (path) { state.seated = path } }
    }
    for (var key in over) {
        state[key] = over[key]
    }
    return state
}

function run(check) {
    // ---- trashed: the line, the marks the trash took, and a re-read that seats nothing ----
    var s = stubState({ marks: [{ path: "/d/c.png", bytes: 30 }, { path: "/d/a.png", bytes: 10 }, { path: "/e/a.png", bytes: 10 }] })
    Wire.trashed(s, 2, 0)
    check("trashed says the browser's own line", s.said.join("|"), "Moved 2 items to Trash · z undoes")
    check("a trash that took something is not an error", s.errors[0], false)
    check("the marks the trash took leave the list", Picker.paths(s.marks).join(","), "/e/a.png")
    check("trashed clears the sticky line first", s.stuck.join("|"), "")
    check("trashed re-reads the directory", s.relisted, "/d")
    check("trashed seats nothing", s.seated, null)
    s = stubState({ marks: [{ path: "/e/a.png", bytes: 10 }] })
    Wire.trashed(s, 1, 0)
    check("with no mark here the cursor row was trashed and the marks stand", Picker.paths(s.marks).join(","), "/e/a.png")
    s = stubState({ marks: [{ path: "/d/a.png", bytes: 10 }] })
    Wire.trashed(s, 0, 1)
    check("a trash that took nothing keeps its marks", Picker.paths(s.marks).join(","), "/d/a.png")
    check("and says so in the error role", s.said.join("|") + ":" + s.errors[0], "That item could not be moved to Trash.:true")
    check("and still re-reads", s.relisted, "/d")

    // ---- renamed, made, duplicated, undone: each re-read seats the path the write produced ----
    s = stubState({ renameFromPath: "/d/c.png", renameListingPath: "/d",
                    marks: [{ path: "/d/c.png", bytes: 30 }] })
    Wire.renamed(s, true, "/d/b.png")
    check("renamed seats the new name", s.relisted + " " + s.seated, "/d /d/b.png")
    check("renamed moves a mark to the new path", Picker.paths(s.marks).join(","), "/d/b.png")
    check("renamed spends the source path", s.renameFromPath, "")
    s = stubState({ renameFromPath: "/d/c.png", renameListingPath: "/d", renamePointerPath: "/d/d.txt" })
    Wire.renamed(s, true, "/d/b.png")
    check("a click-away rename keeps the row the pointer chose", s.relisted + " " + s.seated, "/d /d/d.txt")
    check("the click-away path is one shot", s.renamePointerPath, "")
    s = stubState({ path: "/e", renameFromPath: "/d/c.png", renameListingPath: "/d",
                    renamePointerPath: "/d/d.txt" })
    Wire.renamed(s, true, "/d/b.png")
    check("a rename reply after navigation does not re-read the new directory", s.relisted, "")
    s = stubState({ path: "recent", renameFromPath: "/d/c.png", renameListingPath: "recent" })
    Wire.renamed(s, true, "/d/b.png")
    check("a rename from Recent refreshes that listing", s.relisted + " " + s.seated, "recent /d/b.png")
    s = stubState({})
    Wire.made(s, true, "/d/New Folder")
    check("made remembers the folder for the editor", s.renameOnArrival, "/d/New Folder")
    check("made says the created line", s.said.join("|"), "Created New Folder · z undoes")
    check("made seats the folder", s.seated, "/d/New Folder")
    s = stubState({})
    Wire.duplicated(s, true, "/d/a copy.png")
    check("duplicated says the leaf", s.said.join("|"), "Duplicated to a copy.png · z undoes")
    check("duplicated seats the copy", s.seated, "/d/a copy.png")
    s = stubState({})
    Wire.undone(s, "trash", true)
    check("undone says what came back", s.said.join("|"), "Put it back from Trash.")
    check("undone re-reads and seats nothing", s.relisted + " " + s.seated, "/d null")

    // ---- transfer: the sticky line follows the wire, a foreign id is ignored ----
    s = stubState({})
    Wire.transferStarted(s, 7, 3, false)
    check("a started transfer holds its id", s.transfer.id, 7)
    check("and its head is the sticky line", s.stuck.join("|"), "Copying 1 of 3")
    Wire.transferProgress(s, 8, 0, "x.txt", 1, 2)
    check("a sample for another id is dropped", s.stuck.length, 1)
    Wire.transferProgress(s, 7, 0, "a.txt", 50, 100)
    check("a sample names the file", s.stuck[1], "Copying 1 of 3, a.txt")
    Wire.transferItem(s, 7, 0, "a.txt")
    check("an item done counts whole", s.transfer.done, 1)
    Wire.transferDone(s, 8, 3, 0, 0, false)
    check("a done for another id changes nothing", s.transfer.id, 7)
    Wire.transferDone(s, 7, 3, 0, 0, false)
    check("done clears the transfer", s.transfer.id, 0)
    check("done clears the sticky line", s.stuck[s.stuck.length - 1], "")
    check("done says the copied line", s.said.join("|"), "Copied 3 items · z undoes")
    check("done re-reads", s.relisted, "/d")
    s = stubState({ transfer: Ops.started(2, true, 2) })
    Wire.transferDone(s, 2, 0, 2, 0, false)
    check("a transfer that moved nothing is an error", s.errors[0] + ":" + s.said[0], "true:Moved 0 items, 2 failed")

    // ---- armRename: the editor opens only on the row the cursor really landed on ----
    s = stubState({ renameOnArrival: "/d/sub", cursorIndex: 3 })
    Wire.armRename(s)
    check("the cursor on the made folder arms the editor", s.renamingIndex, 3)
    check("and the path is spent", s.renameOnArrival, "")
    s = stubState({ renameOnArrival: "/d/sub", cursorIndex: 4 })
    Wire.armRename(s)
    check("the cursor elsewhere arms nothing", s.renamingIndex, -1)
    check("but the path is spent either way", s.renameOnArrival, "")
    s = stubState({ renameOnArrival: "/d/zzz", cursorIndex: 9 })
    Wire.armRename(s)
    check("a folder past the held window arms nothing", s.renamingIndex, -1)
    s = stubState({ cursorIndex: 3 })
    Wire.armRename(s)
    check("no pending folder is a no-op", s.renamingIndex, -1)

    // ---- failed: a sort is a notice, a listing failure blanks, an operation leaves the rows ----
    s = stubState({})
    Wire.failed(s, "sort", "unknown key")
    check("a refused sort is a plain notice", s.said.join("|") + ":" + s.errors[0], "Sorting by that column is not available.:false")
    check("and leaves the listing standing", s.listingState, "ready")
    s = stubState({ renameOnArrival: "/d/sub", transfer: Ops.started(3, false, 1) })
    Wire.failed(s, "scan", "permission denied")
    check("a scan failure says the backend's own line", s.said.join("|"), "permission denied")
    check("and empties the listing", s.listingState, "empty")
    check("and drops the armed editor", s.renameOnArrival, "")
    check("a scan failure leaves a running transfer alone", s.transfer.id, 3)
    s = stubState({ transfer: Ops.started(3, false, 1) })
    Wire.failed(s, "read", "")
    check("a backend that stopped ends the transfer", s.transfer.id, 0)
    check("and empties the listing", s.listingState, "empty")
    s = stubState({ renameOnArrival: "/d/sub", renameFromPath: "/d/c.png",
                    renameListingPath: "/d", renamePointerPath: "/d/d.txt" })
    Wire.failed(s, "rename", "File exists")
    check("a refused rename takes the sentence", s.said.join("|"), "A file with that name is already here.")
    check("and leaves the listing standing", s.listingState, "ready")
    check("and the armed editor waiting", s.renameOnArrival, "/d/sub")
    check("and re-reads nothing", s.relisted, "")
    check("and drops all pending rename state",
          s.renameFromPath + ":" + s.renameListingPath + ":" + s.renamePointerPath, "::")
    s = stubState({ listingState: "loading" })
    Wire.failed(s, "mkdir", "")
    check("a failure while loading empties the listing", s.listingState, "empty")
    s = stubState({ renameListingPath: "/d" })
    Wire.failed(s, "rename-kept", "")
    check("rename-kept re-reads and seats nothing", s.relisted + " " + s.seated, "/d null")
    check("rename-kept says the long sentence", s.said[0].indexOf("The copy is complete") === 0, true)
    s = stubState({ path: "/e", renameListingPath: "/d" })
    Wire.failed(s, "rename-kept", "")
    check("a late rename-kept failure does not re-read the new directory", s.relisted, "")
}
