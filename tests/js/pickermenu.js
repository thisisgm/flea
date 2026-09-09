.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/PickerMenu.js" as PickerMenu

// ui/js/PickerMenu.js: where a right click aims the chooser's menu and where a chosen row goes,
// over ui/PickerState.qml reduced to what the two read, with a log of every call they make.

function stubState(over) {
    var state = {
        path: "/home/jw/docs",
        recent: false,
        held: 0,
        rows: [{ n: "a.txt", d: false, s: 1 }, { n: "b.txt", d: false, s: 2 }, { n: "c.txt", d: false, s: 3 }],
        marks: [],
        cursorIndex: 0,
        shownTotal: 3,
        windowSize: 60,
        thumbState: "thumbs",
        dirSizeState: "sizes",
        fetching: false,
        filterQuery: "",
        filterTyping: false,
        focusView: "list",
        calls: [],
        said: [],
        backend: {
            sortBy: "name", sortDesc: false,
            sort: function (key, desc) { state.calls.push("sort " + key + " " + desc) },
            window: function (start, size) { state.calls.push("window " + start) },
            duplicate: function (path) { state.calls.push("duplicate " + path) }
        },
        rowFor: function (index) { return index >= 0 && index < state.rows.length ? state.rows[index] : null },
        join: function (base, name) { return Picker.rowPath(base, name) },
        dropMarks: function (paths) { state.calls.push("drop " + paths.join(",")) },
        setCursor: function (index) { state.cursorIndex = index; state.calls.push("cursor " + index) },
        clearSelection: function () {},
        cancel: function () { state.calls.push("cancel") },
        message: function (text) { state.said.push(text) }
    }
    for (var key in over)
        state[key] = over[key]
    return state
}

function stubOps() {
    var ops = { visibleRows: 7, moved: [], moveCursor: function (delta) { ops.moved.push(delta) } }
    return ops
}

function run(check) {
    var a = { path: "/home/jw/docs/a.txt", bytes: 1 }
    var b = { path: "/home/jw/docs/b.txt", bytes: 2 }
    var elsewhere = { path: "/home/jw/pics/z.png", bytes: 9 }

    // ---- aim: ui/js/Tap.js tappedMenu's rule for marks ----
    var onMarked = stubState({ marks: [a, b] })
    check("a right click on a marked row keeps every mark and moves the cursor there",
          PickerMenu.aim(onMarked, 1) + "|" + onMarked.calls.join(";"), "true|cursor 1")
    var offMarked = stubState({ marks: [a, elsewhere] })
    check("a right click outside the marks drops this directory's marks and leaves another's",
          PickerMenu.aim(offMarked, 2) + "|" + offMarked.calls.join(";"),
          "true|drop /home/jw/docs/a.txt;cursor 2")
    var none = stubState()
    check("with no mark standing the row only takes the cursor",
          PickerMenu.aim(none, 2) + "|" + none.calls.join(";"), "true|cursor 2")
    var recent = stubState({ path: "recent", recent: true, marks: [a],
                             rows: [{ n: "/home/jw/docs/b.txt", d: false, s: 2 }] })
    check("in Recent no mark's parent is the token, so nothing drops",
          PickerMenu.aim(recent, 0) + "|" + recent.calls.join(";"), "true|cursor 0")
    var gone = stubState({ marks: [a] })
    check("a right click past the held rows aims at nothing and touches nothing",
          PickerMenu.aim(gone, 7) + "|" + gone.calls.length, "false|0")

    // ---- route: the prefixes, the unbuilt rows, and the key table's own path ----
    var toggled = []
    var columns = { toggleColumn: function (key) { toggled.push(key) } }
    var sorted = stubState()
    PickerMenu.route("sort:size", sorted, stubOps(), columns)
    check("Sort by Size asks the backend as the header's click does and re-seats the cursor",
          sorted.calls.join(";") + "|" + sorted.backend.sortBy + ":" + sorted.backend.sortDesc,
          "sort size false;cursor 0;window 0|size:false")
    var inRecent = stubState({ recent: true })
    PickerMenu.route("sort:size", inRecent, stubOps(), columns)
    check("Sort by in Recent says so and asks for nothing",
          inRecent.said.join("") + "|" + inRecent.calls.length,
          "Recent keeps the history's own order.|0")
    PickerMenu.route("col:size", stubState(), stubOps(), columns)
    check("col: toggles the column through the store handed in", toggled.join(","), "size")
    var unbuilt = stubState()
    var ops = stubOps()
    PickerMenu.route("open", unbuilt, ops, columns)
    PickerMenu.route("copypath", unbuilt, ops, columns)
    check("the rows no picker verb answers yet say so in the footer, each by its own name",
          unbuilt.said.join("|"),
          "Open is not built in the chooser yet.|Copy path is not built in the chooser yet.")
    PickerMenu.route("cancel", unbuilt, ops, columns)
    check("every other row takes the key table's own route through PickerKeys.act",
          unbuilt.calls.join(";"), "cancel")
    PickerMenu.route("cursorDown", unbuilt, ops, columns)
    check("and reaches the list's ops the way a key does", ops.moved.join(","), "1")
    PickerMenu.route("trash", unbuilt, ops, columns)
    check("a shared action with no verb yet still answers through the key table's line",
          unbuilt.said[unbuilt.said.length - 1], "Move to Trash is not built in the chooser yet.")

    // ---- duplicate: ui/js/Ops.js duplicate over the chooser, the cursor row alone ----
    var dup = stubState({ marks: [a, b], cursorIndex: 2 })
    PickerMenu.route("duplicate", dup, stubOps(), columns)
    check("Duplicate sends the cursor row's path and never the marks",
          dup.calls.join(";") + "|" + dup.said.length, "duplicate /home/jw/docs/c.txt|0")
    var dupRecent = stubState({ path: Picker.RECENT, recent: true, cursorIndex: 0,
                                rows: [{ n: "home/jw/docs/b.txt", d: false, s: 2 }] })
    PickerMenu.route("duplicate", dupRecent, stubOps(), columns)
    check("in Recent the row's own absolute path is what goes out",
          dupRecent.calls.join(";"), "duplicate /home/jw/docs/b.txt")
    var dupEmpty = stubState({ rows: [], cursorIndex: 0 })
    PickerMenu.route("duplicate", dupEmpty, stubOps(), columns)
    check("with no row under the cursor nothing is sent", dupEmpty.calls.length, 0)
}
