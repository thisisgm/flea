.import "../../ui/js/PickerOps.js" as PickerOps
.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/PickerWire.js" as Wire
.import "../../ui/js/Ops.js" as Ops

// A stub in the shape of tests/js/ops.js's windowedPane: the chooser holds a window of four rows
// starting at listing index 2 out of a listing of ten, so a mark can sit before, inside or past it.
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
        total: 10,
        rows: rows,
        cursorIndex: 4,
        marks: [],
        shown: null,
        folderMode: false,
        req: { multiple: true },
        history: [],
        relisted: "",
        seated: null,
        said: [],
        stuck: [],
        errors: [],
        transfer: Ops.emptyTransfer(),
        sent: [],
        trashPending: [],
        backend: { send: function (request) { state.sent.push(JSON.stringify(request)) } },
        rowFor: function (i) { return (i - 2 < 0 || i - 2 >= rows.length) ? null : rows[i - 2] },
        message: function (text, isError) { state.said.push(text); state.errors.push(isError) },
        sticky: function (text) { state.stuck.push(text) },
        open: function (next) { state.history = state.history.concat([state.path]); state.path = next },
        openWithoutHistory: function (next) { state.relisted = next },
        navigate: { seat: function (path) { state.seated = path } }
    }
    for (var key in over) {
        state[key] = over[key]
    }
    return state
}

function run(check) {
    // ---- indicesFor: a mark is a path, and only a row the window holds can give it an index ----
    var s = stubState({ marks: [{ path: "/d/c.png", bytes: 30 }, { path: "/d/a.png", bytes: 10 }] })
    check("marks in this directory resolve to ascending listing indices",
          PickerOps.indicesFor(s).join(","), "2,4")
    s = stubState({ marks: [{ path: "/d/a.png", bytes: 10 }, { path: "/e/a.png", bytes: 10 }] })
    check("a mark from another directory is dropped", PickerOps.indicesFor(s).join(","), "2")
    // Marked while the window stood elsewhere: the path is real, the row is not held, so no index.
    s = stubState({ marks: [{ path: "/d/z.png", bytes: 99 }, { path: "/d/d.txt", bytes: 40 }] })
    check("a mark past the held window is dropped", PickerOps.indicesFor(s).join(","), "5")
    check("no marks is no indices", PickerOps.indicesFor(stubState({})).length, 0)

    // ---- pathsFor: the marks standing here, or the cursor row alone ----
    check("with no marks pathsFor answers the cursor row alone",
          PickerOps.pathsFor(stubState({})).join(","), "/d/c.png")
    s = stubState({ marks: [{ path: "/d/z.png", bytes: 99 }, { path: "/e/a.png", bytes: 10 }, { path: "/d/a.png", bytes: 10 }] })
    check("pathsFor keeps every mark in this directory, held or not, and drops the other directory's",
          PickerOps.pathsFor(s).join(","), "/d/z.png,/d/a.png")
    check("a cursor past the window answers nothing", PickerOps.pathsFor(stubState({ cursorIndex: 9 })).length, 0)
    s = stubState({ path: Picker.RECENT, marks: [{ path: "/d/a.png", bytes: 10 }] })
    check("in Recent the cursor row's own path answers", PickerOps.pathsFor(s).join(","), "/c.png")

    // ---- trash: absolute paths, including a mark outside the held window ----
    s = stubState({ marks: [{ path: "/d/z.png", bytes: 99 }, { path: "/e/a.png", bytes: 10 }, { path: "/d/a.png", bytes: 10 }] })
    PickerOps.trash(s)
    check("trash sends every mark in this directory as an absolute path",
          s.sent.join("|"), '{"c":"trash","paths":["/d/z.png","/d/a.png"]}')
    check("trash keeps the exact paths for its count-only reply",
          JSON.stringify(s.trashPending), '["/d/z.png","/d/a.png"]')
    s.marks = [{ path: "/d/d.txt", bytes: 40 }]
    PickerOps.trash(s)
    check("a second trash waits because count-only replies cannot identify requests",
          s.sent.length + "|" + JSON.stringify(s.trashPending),
          '1|["/d/z.png","/d/a.png"]')
    s = stubState({ trashPending: [] })
    PickerOps.trash(s)
    check("trash falls back to the cursor row", s.sent.join("|"),
          '{"c":"trash","paths":["/d/c.png"]}')
    s = stubState({ cursorIndex: 9 })
    PickerOps.trash(s)
    check("trash sends nothing without a target", s.sent.length, 0)

    // ---- refresh: a re-read, never a history entry ----
    s = stubState({ history: ["/"] })
    PickerOps.refresh(s, "/d/new.png")
    check("refresh re-lists the directory the window stands in", s.relisted, "/d")
    check("refresh does not push history", s.history.join(","), "/")
    check("refresh seats the cursor on the written path", s.seated, "/d/new.png")
    s = stubState({})
    PickerOps.refresh(s, "")
    check("a refresh with no path seats nothing", s.seated, null)

    // ---- dropMarks: what left the disk leaves the marks ----
    s = stubState({ marks: [{ path: "/d/a.png", bytes: 10 }, { path: "/d/c.png", bytes: 30 }, { path: "/e/a.png", bytes: 10 }] })
    PickerOps.dropMarks(s, ["/d/c.png", "/d/never.png"])
    check("marks are pruned by path", Picker.paths(s.marks).join(","), "/d/a.png,/e/a.png")
    check("a pruned mark keeps its weight", Picker.totalBytes(s.marks), 20)

    // ---- selectAll: every drawn markable row, and a refusal for a single-item request ----
    s = stubState({ marks: [{ path: "/d/c.png", bytes: 30 }, { path: "/e/a.png", bytes: 10 }] })
    PickerOps.selectAll(s)
    check("select all marks every held file once and keeps the marks it had",
          Picker.paths(s.marks).join(","), "/d/c.png,/e/a.png,/d/a.png,/d/d.txt")
    s = stubState({ folderMode: true })
    PickerOps.selectAll(s)
    check("a folder request marks the folders alone", Picker.paths(s.marks).join(","), "/d/sub")
    s = stubState({ shown: [3, 4] })
    PickerOps.selectAll(s)
    check("select all honours the chip", Picker.paths(s.marks).join(","), "/d/c.png")
    s = stubState({ shown: [2, 9] })
    PickerOps.selectAll(s)
    check("a drawn row past the held window cannot be marked", Picker.paths(s.marks).join(","), "/d/a.png")
    s = stubState({ req: { multiple: false }, marks: [{ path: "/d/a.png", bytes: 10 }] })
    PickerOps.selectAll(s)
    check("a single-file request refuses select all", s.said.join("|"), "The caller takes one file only.")
    check("and leaves the mark it had", Picker.paths(s.marks).join(","), "/d/a.png")
    s = stubState({ req: { multiple: false }, folderMode: true })
    PickerOps.selectAll(s)
    check("a single-folder request says folder", s.said.join("|"), "The caller takes one folder only.")

    // ---- transfer footer: each wire model uses the browser's own line builders exactly ----
    s = stubState({})
    Wire.transferStarted(s, 7, 3, false)
    check("a started transfer shows Ops.progressLine", s.stuck[0], Ops.progressLine(s.transfer))
    Wire.transferProgress(s, 7, 0, "a.txt", 50, 100)
    check("a sampled transfer shows Ops.progressLine", s.stuck[1], Ops.progressLine(s.transfer))
    Wire.transferItem(s, 7, 0, "a.txt")
    check("an item-done transfer shows Ops.progressLine", s.stuck[2], Ops.progressLine(s.transfer))
    var finished = Ops.transferDone(s.transfer, 3, 0, false)
    Wire.transferDone(s, 7, 3, 0, 0, false)
    check("a finished transfer shows Ops.transferDone", s.said[0], finished)
    check("a finished transfer clears the sticky slot", s.stuck[s.stuck.length - 1], "")
}
