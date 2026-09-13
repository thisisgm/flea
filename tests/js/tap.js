.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Tap.js" as Tap

// keys.toml's [[pointer]] table is the click contract, and this is what holds ui/js/Tap.js to it.
// Every listing and neighbour row of Keymap.POINTER is driven here, so a click cannot change meaning
// without the table saying so and the table cannot advertise a click the code does not make. The
// rail, chrome and window rows are QML with no JavaScript under them, so they are counted here and
// pressed at the real window by tests/ui.sh; the counts stop a row being dropped from either side.

// A pane that records what a click asked it to do and does nothing else: one verb per call, in order.
function pane() {
    return {
        did: [],
        cursor: -1,
        // Mirrors ui/Pane.qml's own guard: with no editor open the click-away commit is a no-op.
        renamingIndex: -1,
        // ui/js/Search.js activateAction reads this, and "results" is the only value that reveals.
        searchMode: "",
        // The selection ui/js/Tap.js reads before it decides what a right click means.
        picked: [],
        // Only tappedMiddle asks what kind of row it landed on; the listing's own taps never do.
        rowFor: function () { return null },
        commitOpenRename: function () { if (this.renamingIndex >= 0) this.did.push("commitRename") },
        selectedIndices: function () { return this.picked },
        clearSelection: function () { this.picked = []; this.did.push("clearSelection") },
        selectOnly: function (i) { this.picked = [i]; this.cursor = i; this.did.push("selectOnly") },
        setCursor: function (i) { this.cursor = i; this.did.push("setCursor") },
        toggleSelectAt: function (i) { this.cursor = i; this.did.push("toggleSelect") },
        extendSelectionTo: function (i) { this.cursor = i; this.did.push("extendSelect") },
        act: function (action) { this.did.push(action) }
    }
}

function menu() {
    return { at: "", openAt: function (p) { this.at = p.x + "," + p.y } }
}

// The one member ui/js/Tap.js reads off a tap's event point.
function eventPoint() {
    return { scenePosition: { x: 7, y: 9 } }
}

// The table's press column, turned back into the three things a TapHandler actually reports.
function press(text) {
    return {
        button: text.indexOf("right") >= 0 ? Qt.RightButton : Qt.LeftButton,
        taps: text.indexOf("x2") >= 0 ? 2 : 1,
        modifiers: (text.indexOf("ctrl") >= 0 ? Qt.ControlModifier : 0)
                 | (text.indexOf("shift") >= 0 ? Qt.ShiftModifier : 0)
    }
}

// The recorder's tape read back as the table's own does word. A double click sets the cursor on both
// of its taps, which is the whole point of choosing an idempotent single-click action over a timer,
// so that pair is a spelling of "open" and not a defect.
function verbOf(did) {
    var tape = did.join(",")
    if (tape === "") return "nothing"
    if (tape === "selectOnly") return "selectOnly"
    if (tape === "commitRename,selectOnly") return "commitRename"
    if (tape === "selectOnly,selectOnly,open") return "open"
    if (tape === "selectOnly,selectOnly,reveal") return "reveal"
    // A result reveals on the first tap, so a double click on one is still that single reveal, and
    // the columns view's one-tap open on a directory reads the same way.
    if (tape === "selectOnly,reveal") return "reveal"
    if (tape === "selectOnly,open") return "open"
    if (tape === "toggleSelect") return "toggleSelect"
    if (tape === "extendSelect") return "extendSelect"
    return tape
}

// A real TapHandler raises tapped once per tap, with tapCount counting up, so a double-click row is
// driven as the two taps it is and never as one call carrying a 2.
function driveListing(row) {
    var p = press(row.press)
    var sink = pane()
    // The table's row column, turned back into the pane state the click meets.
    if (row.row === "renaming")
        sink.renamingIndex = 2
    // A result is an ordinary listing row; what makes it one is the mode the pane is in.
    if (row.row === "result")
        sink.searchMode = "results"
    if (p.button === Qt.RightButton) {
        var raised = menu()
        Tap.tappedMenu(2, eventPoint(), sink, raised)
        if (raised.at !== "7,9")
            return "the menu opened at " + raised.at
        return sink.did.join(",") === "setCursor" ? "menu" : verbOf(sink.did)
    }
    for (var t = 1; t <= p.taps; t++)
        Tap.tapped(2, t, p.modifiers, sink)
    return verbOf(sink.did)
}

// A neighbour column answers a verb instead of acting, because the pane has no cursor on its rows.
function driveNeighbour(row) {
    var p = press(row.press)
    var entry = row.row === "file" ? { n: "notes.txt", d: false } : { n: "sub", d: true }
    var verb = ""
    for (var t = 1; t <= p.taps; t++) {
        var answer = Tap.tappedColumn(entry, p.button, t)
        if (answer.length > 0)
            verb = answer
    }
    return verb.length > 0 ? verb : "nothing"
}

// The columns view's middle column: an ordinary listing tap plus the one rule of its own, so the
// row kind has to be real here where driveListing never needs it.
function driveColumn(row) {
    var p = press(row.press)
    var sink = pane()
    var entry = row.row === "file" ? { n: "notes.txt", d: false } : { n: "sub", d: true }
    sink.rowFor = function () { return entry }
    for (var t = 1; t <= p.taps; t++)
        Tap.tappedMiddle(2, t, p.modifiers, sink)
    return verbOf(sink.did)
}

function countWhere(where) {
    return Keymap.POINTER.filter(function (row) { return row.where === where }).length
}

function run(check) {
    var rows = Keymap.POINTER
    // The denominator first: an empty table would pass every loop below by having nothing in it.
    check("the pointer table reached the tests at all", rows.length, 20)
    check("the table declares the listing's clicks", countWhere("listing"), 10)
    check("the table declares the middle column's own click", countWhere("column"), 1)
    check("the table declares the neighbour columns' clicks", countWhere("neighbour"), 4)
    check("the table declares the rail's clicks", countWhere("rail"), 2)
    // Issue 20's back button belongs to no row, so it is declared against the window itself and
    // ui/shell.qml is what carries it; nothing here can press it and tests/ui.sh does, with ydotool.
    check("the table declares the window's own buttons", countWhere("window"), 1)
    // Issue 45's crumbs are ui/ChromeBar.qml's own targets, above the listing and not in it, so
    // driveListing has nothing to press for them either; the same tests/ui.sh case clicks one.
    check("the table declares the chrome path's clicks", countWhere("chrome"), 2)
    check("every row lands in one of those six places",
          countWhere("listing") + countWhere("column") + countWhere("neighbour") + countWhere("rail")
          + countWhere("window") + countWhere("chrome"),
          rows.length)

    var drivenListing = 0
    var drivenColumn = 0
    var drivenNeighbour = 0
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].where === "listing") {
            drivenListing += 1
            check("listing, " + rows[i].press + ", does " + rows[i].does, driveListing(rows[i]), rows[i].does)
        }
        if (rows[i].where === "column") {
            drivenColumn += 1
            check("middle column, " + rows[i].press + " on a " + rows[i].row + ", does " + rows[i].does,
                  driveColumn(rows[i]), rows[i].does)
        }
        if (rows[i].where === "neighbour") {
            drivenNeighbour += 1
            check("neighbour column, " + rows[i].press + " on a " + rows[i].row + ", does " + rows[i].does,
                  driveNeighbour(rows[i]), rows[i].does)
        }
    }
    check("every listing row of the table was driven", drivenListing, 10)
    check("every middle column row of the table was driven", drivenColumn, 1)
    check("every neighbour row of the table was driven", drivenNeighbour, 4)

    // The operator's own defect, stated as the thing that must never come back: a single left click
    // reached act("open") on any row, in every view, before 2026-09-02.
    var single = pane()
    Tap.tapped(4, 1, Qt.NoModifier, single)
    check("one left tap opens nothing", single.did.indexOf("open"), -1)
    check("and it does move the cursor to the row it landed on", single.cursor, 4)
    check("and it marks only that row", single.selectedIndices().join(","), "4")
    single.picked = [1, 2, 3]
    Tap.tapped(4, 1, Qt.NoModifier, single)
    check("a plain tap replaces every old mark with its one row", single.selectedIndices().join(","), "4")
    var stale = pane()
    Tap.tapped(-1, 1, Qt.NoModifier, stale)
    check("a recycled delegate cannot select a negative row", stale.did.length, 0)

    // The second tap is what opens, and the cursor it opens is the one the first tap set.
    var double_ = pane()
    Tap.tapped(4, 1, Qt.NoModifier, double_)
    Tap.tapped(4, 2, Qt.NoModifier, double_)
    check("the second tap opens", double_.did.join(","),
          "selectOnly,selectOnly,open")

    // A triple click is one open and not two: tapCount keeps counting while the taps keep coming.
    var triple = pane()
    for (var t = 1; t <= 3; t++)
        Tap.tapped(4, t, Qt.NoModifier, triple)
    check("a third tap does not open a second time",
          triple.did.filter(function (v) { return v === "open" }).length, 1)

    // A tap on a search result takes you to the file rather than launching it, which is
    // ui/js/Search.js activateAction's whole job, and the operator's 2026-09-11 ruling is that the
    // first tap is what does it. Nothing else in the listing acts on one tap.
    var result = pane()
    result.searchMode = "results"
    Tap.tapped(4, 1, Qt.NoModifier, result)
    check("one tap on a search result reveals it instead of opening it",
          result.did.join(","), "selectOnly,reveal")
    // The reveal already moved the pane, so the row under a second tap is another directory's: it
    // must not select it and must not reveal a second time.
    Tap.tapped(4, 2, Qt.NoModifier, result)
    check("and the second tap of a double click adds nothing",
          result.did.join(","), "selectOnly,reveal")

    // The columns view's middle column. The operator's 2026-09-11 ruling is that one tap goes into a
    // directory there, the way both neighbour columns already do; ui/ColumnsArea.qml is the only
    // caller, so the list and the grid keep their two-tap rule.
    var column = pane()
    column.rowFor = function () { return { n: "sub", d: true } }
    Tap.tappedMiddle(4, 1, Qt.NoModifier, column)
    check("one tap on a directory in the middle column opens it",
          column.did.join(","), "selectOnly,open")
    Tap.tappedMiddle(4, 2, Qt.NoModifier, column)
    check("and the second tap of a double click opens nothing a second time",
          column.did.join(","), "selectOnly,open")

    // The same tap in the list and the grid still only selects, which is the whole point of scoping
    // the rule to one caller: this is tapped(), the function those two views reach.
    var listed = pane()
    listed.rowFor = function () { return { n: "sub", d: true } }
    Tap.tapped(4, 1, Qt.NoModifier, listed)
    check("one tap on a directory in the list view still opens nothing",
          listed.did.join(","), "selectOnly")

    // A file in the middle column is an ordinary row, so it waits for the second tap as it always did.
    var columnFile = pane()
    columnFile.rowFor = function () { return { n: "notes.txt", d: false } }
    Tap.tappedMiddle(4, 1, Qt.NoModifier, columnFile)
    check("one tap on a file in the middle column opens nothing", columnFile.did.join(","), "selectOnly")
    Tap.tappedMiddle(4, 2, Qt.NoModifier, columnFile)
    check("and the second tap opens it", columnFile.did.join(","), "selectOnly,selectOnly,open")

    // A modifier never opens anywhere, and the middle column's own rule must not be the exception.
    var columnCtrl = pane()
    columnCtrl.rowFor = function () { return { n: "sub", d: true } }
    Tap.tappedMiddle(4, 1, Qt.ControlModifier, columnCtrl)
    check("ctrl on a directory in the middle column still only selects",
          columnCtrl.did.join(","), "toggleSelect")
    var columnShift = pane()
    columnShift.rowFor = function () { return { n: "sub", d: true } }
    Tap.tappedMiddle(4, 1, Qt.ShiftModifier, columnShift)
    check("shift on a directory in the middle column still only extends",
          columnShift.did.join(","), "extendSelect")

    // A delegate can outlive its row by a frame, the same guard every reader of pane.rows carries.
    var columnGone = pane()
    Tap.tappedMiddle(-1, 1, Qt.NoModifier, columnGone)
    check("a recycled middle column delegate cannot act on a negative row", columnGone.did.length, 0)

    // A directory listed as a search result is still a result: the middle column reveals it rather
    // than opening it, exactly as the same row does in the list view.
    var columnResult = pane()
    columnResult.searchMode = "results"
    columnResult.rowFor = function () { return { n: "sub", d: true } }
    Tap.tappedMiddle(4, 1, Qt.NoModifier, columnResult)
    check("a directory that is a search result reveals from the middle column too",
          columnResult.did.join(","), "selectOnly,reveal")

    // Right click sets the cursor to the row under the pointer. Setting the cursor is NOT on its own
    // what makes every menu action address that row: ui/ContextMenu.qml builds every entry it draws
    // from the cursor row, while Move to Trash, Compress and Move to Dropbox dispatch through
    // Ops.targetIndices, which prefers the selection. So the pressed row has to decide both.
    var menued = pane()
    var raised = menu()
    Tap.tappedMenu(6, eventPoint(), menued, raised)
    check("right click moves the cursor to the row under the pointer", menued.cursor, 6)
    check("and opens the menu at the pointer, not at the row", raised.at, "7,9")
    check("and opens nothing", menued.did.indexOf("open"), -1)

    // The operator's own defect, stated as the thing that must never come back: rows 1 to 3 selected,
    // a right click on row 50, a menu offering Extract because row 50 was an archive, and Move to
    // Trash took rows 1, 2 and 3. Measured on the box before the fix, with those three files gone.
    var outside = pane()
    outside.picked = [1, 2, 3]
    Tap.tappedMenu(50, eventPoint(), outside, menu())
    check("a right click outside the selection drops it, so the menu acts on the row it describes",
          outside.selectedIndices().join(","), "")
    check("and the cursor is that row, which is what every entry is built from", outside.cursor, 50)

    // The other half of ui/js/Drag.js carried()'s rule, and the half that was already right.
    var inside = pane()
    inside.picked = [1, 2, 3]
    Tap.tappedMenu(2, eventPoint(), inside, menu())
    check("a right click inside the selection leaves it whole, as Finder does",
          inside.selectedIndices().join(","), "1,2,3")
    check("and still moves the cursor to the pressed row", inside.cursor, 2)

    // A delegate can outlive the row it was built for by a frame, which is why every reader of
    // pane.rows in this tree guards; the column's tap is a reader like any other.
    check("a neighbour tap on a row that is gone answers nothing",
          Tap.tappedColumn(null, Qt.LeftButton, 1), "")
    // The reveal already moved the pane, so the delegate under the second tap is another directory's.
    check("a second tap on a neighbour directory does not reveal twice",
          Tap.tappedColumn({ n: "sub", d: true }, Qt.LeftButton, 2), "")
}
