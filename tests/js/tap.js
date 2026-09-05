.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Tap.js" as Tap

// keys.toml's [[pointer]] table is the click contract, and this is what holds ui/js/Tap.js to it.
// Every listing and neighbour row of Keymap.POINTER is driven here, so a click cannot change meaning
// without the table saying so and the table cannot advertise a click the code does not make. The
// rail's two rows are ui/SidebarRow.qml's own and carry no JavaScript, so they are counted and not
// driven; the counts below are what stops a row being quietly dropped from either side.

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
        commitOpenRename: function () { if (this.renamingIndex >= 0) this.did.push("commitRename") },
        selectedIndices: function () { return this.picked },
        clearSelection: function () { this.picked = []; this.did.push("clearSelection") },
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
    if (tape === "clearSelection,setCursor") return "selectOnly"
    if (tape === "commitRename,clearSelection,setCursor") return "commitRename"
    if (tape === "clearSelection,setCursor,clearSelection,setCursor,open") return "open"
    if (tape === "clearSelection,setCursor,clearSelection,setCursor,reveal") return "reveal"
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

function countWhere(where) {
    return Keymap.POINTER.filter(function (row) { return row.where === where }).length
}

function run(check) {
    var rows = Keymap.POINTER
    // The denominator first: an empty table would pass every loop below by having nothing in it.
    check("the pointer table reached the tests at all", rows.length, 16)
    check("the table declares the listing's clicks", countWhere("listing"), 9)
    check("the table declares the neighbour columns' clicks", countWhere("neighbour"), 4)
    check("the table declares the rail's clicks", countWhere("rail"), 2)
    // Issue 20's back button belongs to no row, so it is declared against the window itself and
    // ui/shell.qml is what carries it; there is nothing here for driveListing to press.
    check("the table declares the window's own buttons", countWhere("window"), 1)
    check("every row lands in one of those four places",
          countWhere("listing") + countWhere("neighbour") + countWhere("rail") + countWhere("window"),
          rows.length)

    var drivenListing = 0
    var drivenNeighbour = 0
    for (var i = 0; i < rows.length; i++) {
        if (rows[i].where === "listing") {
            drivenListing += 1
            check("listing, " + rows[i].press + ", does " + rows[i].does, driveListing(rows[i]), rows[i].does)
        }
        if (rows[i].where === "neighbour") {
            drivenNeighbour += 1
            check("neighbour column, " + rows[i].press + " on a " + rows[i].row + ", does " + rows[i].does,
                  driveNeighbour(rows[i]), rows[i].does)
        }
    }
    check("every listing row of the table was driven", drivenListing, 9)
    check("every neighbour row of the table was driven", drivenNeighbour, 4)

    // The operator's own defect, stated as the thing that must never come back: a single left click
    // reached act("open") on any row, in every view, before 2026-09-02.
    var single = pane()
    Tap.tapped(4, 1, Qt.NoModifier, single)
    check("one left tap opens nothing", single.did.indexOf("open"), -1)
    check("and it does move the cursor to the row it landed on", single.cursor, 4)
    check("and it drops the selection first, so the next shift+click has a visible anchor",
          single.did.join(","), "clearSelection,setCursor")

    // The second tap is what opens, and the cursor it opens is the one the first tap set.
    var double_ = pane()
    Tap.tapped(4, 1, Qt.NoModifier, double_)
    Tap.tapped(4, 2, Qt.NoModifier, double_)
    check("the second tap opens", double_.did.join(","),
          "clearSelection,setCursor,clearSelection,setCursor,open")

    // A triple click is one open and not two: tapCount keeps counting while the taps keep coming.
    var triple = pane()
    for (var t = 1; t <= 3; t++)
        Tap.tapped(4, t, Qt.NoModifier, triple)
    check("a third tap does not open a second time",
          triple.did.filter(function (v) { return v === "open" }).length, 1)

    // The second tap on a search result takes you to the file rather than launching it, which is
    // ui/js/Search.js activateAction's whole job and which it had no caller for until 2026-09-02.
    var result = pane()
    result.searchMode = "results"
    Tap.tapped(4, 1, Qt.NoModifier, result)
    Tap.tapped(4, 2, Qt.NoModifier, result)
    check("a double click on a search result reveals it instead of opening it",
          result.did.join(","), "clearSelection,setCursor,clearSelection,setCursor,reveal")

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
