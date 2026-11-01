.import "../../ui/js/Columns.js" as Columns
.import "../../ui/js/ColumnMenu.js" as ColumnMenu
.import "../../ui/js/ColumnFit.js" as ColumnFit
.import "../../ui/js/Swap.js" as Swap
.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/Errors.js" as Errors
.import "../../ui/js/Ops.js" as Ops
.import "sourcefixture.js" as Source

// Below about 659 px of window the four fixed columns claimed the whole row and the filename had a
// negative slot, so ui/Row.qml drew every column except the one a file manager exists for. These
// are the floors that stop it: the name never loses, the metadata drops instead.

// This box's own resolved tokens, read off the running app's tokens() seam at base-size 14:
//   rowPaddingX=14 gap=9 iconSize=23 columnMode=70 columnSize=70 columnDate=125 columnKind=130
// nameMin is 20 characters of the same 7.8125 px advance the fixed columns are sized from.
var BOX = { rowPaddingX: 14, gap: 9, iconSize: 23, nameMin: 156, mode: 70, size: 70, date: 125, kind: 130 }

// A second set that shares no number with the first, so nothing here can pass on a constant.
var OTHER = { rowPaddingX: 6, gap: 4, iconSize: 16, nameMin: 100, mode: 40, size: 50, date: 80, kind: 60 }

// The chooser's own tokens: BOX with Theme.column.pickerDate in place of the window's date, which
// the seam resolves to 80 at base-size 14, SendPicker.html's own slot.
var PICKER = { rowPaddingX: 14, gap: 9, iconSize: 23, nameMin: 156, mode: 70, size: 70, date: 80, kind: 130 }

// The chooser's list area on this box: Hyprland floats the picker at 875 px and ui/PickerPlaces.qml
// takes Theme.space(150), 175 px of it, measured off the window Hyprland reported for flea --pick.
var PICKER_SLOT = 700

// The anchor chain in ui/Row.qml, walked here independently of ui/js/Columns.js: the row, less its
// padding either side, the mark and the gap after it, and every drawn column with its own gap.
function nameSlot(width, s, t) {
    var used = t.rowPaddingX + t.iconSize + t.gap + t.rowPaddingX
    if (s.mode) used += t.mode + t.gap
    if (s.size) used += t.size + t.gap
    if (s.date) used += t.date + t.gap
    if (s.kind) used += t.kind + t.gap
    return width - used
}

function run(check) {

    var dual = {rowPaddingX: 14, gap: 9, iconSize: 13 * 1.45, nameMin: 180, size: 70, date: 125}
    check("dual date fits the board's 431px floor", Columns.names(Columns.dualSet(431, dual, [])), "name,size,date")
    check("dual date drops below its name floor", Columns.names(Columns.dualSet(430, dual, [])), "name,size")
    check("hidden dual size releases its actual width", Columns.names(Columns.dualSet(361, dual, ["size"])), "name,date")
    check("dual minimum still keeps Name", Columns.names(Columns.dualSet(200, dual, [])), "name")
    runHidden(check)
    runPicker(check)
    var f = Columns.floors(BOX)
    // 216 is the name at its floor with no metadata at all: 14 + 23 + 9 + 156 + 14.
    check("mode needs the name's floor plus its own column and gap", f.mode, 295)
    check("size needs mode's floor plus its own", f.size, 374)
    check("date needs size's floor plus its own", f.date, 508)
    check("kind needs date's floor plus its own", f.kind, 647)
    // A wider column can never outlive a narrower one, which is what makes the drop order an order.
    check("the four floors nest, widest last",
          f.mode < f.size && f.size < f.date && f.date < f.kind, true)

    // 732 is the list area of the 900 px window Flea asks for, beside this box's 168 px rail.
    check("the default window draws every column",
          Columns.names(Columns.set(732, BOX)), "name,mode,size,date,kind")
    check("a column is kept at exactly its floor",
          Columns.set(647, BOX).kind, true)
    check("and dropped one pixel under it",
          Columns.set(646, BOX).kind, false)
    check("kind goes first and the other three stay",
          Columns.names(Columns.set(646, BOX)), "name,mode,size,date")
    check("date goes second",
          Columns.names(Columns.set(507, BOX)), "name,mode,size")
    check("size goes third",
          Columns.names(Columns.set(373, BOX)), "name,mode")
    check("mode goes last, and the last layout is the mark and the name",
          Columns.names(Columns.set(294, BOX)), "name")
    // 453 is the list area at the 621 px window Hyprland handed Flea beside three terminals.
    check("the width that drew no name at all now draws the name, mode and size",
          Columns.names(Columns.set(453, BOX)), "name,mode,size")

    // The whole point: at no width does a column survive that would put the name under its floor.
    var everyWidthKeepsTheName = true
    var neverGrowsAsItNarrows = true
    var previous = null
    for (var w = 2000; w >= 216; w--) {
        var s = Columns.set(w, BOX)
        if (nameSlot(w, s, BOX) < BOX.nameMin)
            everyWidthKeepsTheName = false
        if (previous !== null) {
            if ((s.mode && !previous.mode) || (s.size && !previous.size)
                || (s.date && !previous.date) || (s.kind && !previous.kind))
                neverGrowsAsItNarrows = false
        }
        previous = s
    }
    check("every width from the name's own floor up keeps the name at or above it",
          everyWidthKeepsTheName, true)
    check("no column ever comes back as the row narrows",
          neverGrowsAsItNarrows, true)

    // Under the name's own floor there is nothing left to drop, so the name takes what is left
    // rather than the layout inventing a column to lose. ui/MatchText.qml clamps the rest.
    check("under the last rung the set is empty rather than undefined",
          Columns.names(Columns.set(100, BOX)), "name")
    check("a zero width answers rather than throwing", Columns.names(Columns.set(0, BOX)), "name")
    check("a negative width answers the same", Columns.names(Columns.set(-500, BOX)), "name")

    // Nothing above is a constant: the same arithmetic on a token set sharing none of those numbers.
    var g = Columns.floors(OTHER)
    check("another token set moves every floor with it",
          g.mode + "|" + g.size + "|" + g.date + "|" + g.kind, "176|230|314|378")
    check("and keeps them nested",
          g.mode < g.size && g.size < g.date && g.date < g.kind, true)
    check("and keeps the name above its own floor there too",
          nameSlot(g.kind, Columns.set(g.kind, OTHER), OTHER) >= OTHER.nameMin, true)

    // The seam ui/Ipc.qml reads is this string, and the header and a row must produce the same one.
    check("the set names the columns left to right, not in drop order",
          Columns.names({ mode: true, size: true, date: true, kind: true }),
          "name,mode,size,date,kind")
    check("the name is in the set even when everything else is gone",
          Columns.names({ mode: false, size: false, date: false, kind: false }), "name")

    runColumnCount(check)
    runListWidths(check)
    runStoredWidths(check)
    runPeekKey(check)
    runPeekPending(check)
    runHeaderDrag(check)
    runColumnsLimitWire(check)
    runColRoot(check)
    runNeighbourAsks(check)
    runColumnMenu(check)
    runFolderHold(check)
    runTabWiring(check)
}

// ColumnsWidth board (#167, #69): the columns count follows the window width, 2 below 900 up to 5 from 2300, capped by the View limit shipping at 3.
function runColumnCount(check) {
    check("a default install at 2560 px shows 3 columns", Columns.columnCountForWidth(2560), 3)
    check("a stored 5 still opens 5 on a wide window", Columns.columnCountForWidth(2560, 5), 5)
    check("a narrow window still shows 2 on the shipped default", Columns.columnCountForWidth(800), 2)
    check("below 900 px the view draws 2 columns", Columns.columnCountForWidth(899, 5), 2)
    check("at 900 px the view draws 3 columns", Columns.columnCountForWidth(900, 5), 3)
    check("at 1700 px the view draws 4 columns", Columns.columnCountForWidth(1700, 5), 4)
    check("at 2300 px the view draws 5 columns", Columns.columnCountForWidth(2300, 5), 5)
    check("just under each step stays down", Columns.columnCountForWidth(1699, 5)
        + "|" + Columns.columnCountForWidth(2299, 5), "3|4")
    check("the limit caps a wide window", Columns.columnCountForWidth(2300, 3), 3)
    check("the limit caps a narrow window too", Columns.columnCountForWidth(1200, 2), 2)
    check("a limit below the floor still draws 2", Columns.columnCountForWidth(2300, 1), 2)
    check("a missing limit reads as the shipped 3", Columns.columnCountForWidth(2300), 3)
    check("an empty limit reads as the shipped 3, not the floor", Columns.cappedLimit(""), 3)
    check("a false limit reads as the shipped 3, not the floor", Columns.cappedLimit(false), 3)
    check("2 columns need no ancestor", Columns.ancestorsForCount(2), 0)
    check("3 columns read one ancestor", Columns.ancestorsForCount(3), 1)
    check("4 columns read two ancestors", Columns.ancestorsForCount(4), 2)
    check("5 columns read three ancestors", Columns.ancestorsForCount(5), 3)
}

// ListColumns040 board: a dragged edge clamps, and a double click fits the widest held value, never a directory scan.
function runListWidths(check) {
    check("a drag inside the rails lands as drawn", Columns.clampListWidth(100), 100)
    check("a drag under the floor clamps to it", Columns.clampListWidth(10), Columns.MIN_LIST_WIDTH)
    check("a drag past the ceiling clamps to it", Columns.clampListWidth(9999), Columns.MAX_LIST_WIDTH)
    check("a drag rounds to whole pixels", Columns.clampListWidth(100.6), 101)
    check("autofit takes the widest held cell", Columns.autofitWidth([70, 120, 90], 70), 120)
    check("autofit narrows a dragged-wide column to the widest held cell",
        Columns.autofitWidth([60, 80], 300), 80)
    check("autofit clamps a wide cell to the ceiling",
        Columns.autofitWidth([9999], 70), Columns.MAX_LIST_WIDTH)
    check("autofit on no held rows keeps the column", Columns.autofitWidth([], 70), 70)

    runColumnFit(check)
}

// ui.json carries whatever a hand edit wrote, so storedWidth takes only a finite number here.
function runStoredWidths(check) {
    check("a stored number lands as drawn", Columns.storedNumber(120), 120)
    check("and rounds to whole pixels", Columns.storedNumber(120.6), 121)
    check("null keeps the measured width", isNaN(Columns.storedNumber(null)), true)
    check("an empty string keeps it too", isNaN(Columns.storedNumber("")), true)
    check("false keeps it too", isNaN(Columns.storedNumber(false)), true)
    check("an empty array keeps it too", isNaN(Columns.storedNumber([])), true)
    check("true is not a width", isNaN(Columns.storedNumber(true)), true)
    check("a numeric string is not a width", isNaN(Columns.storedNumber("120")), true)
    check("a negative is not a width", isNaN(Columns.storedNumber(-40)), true)
    check("NaN is not a width", isNaN(Columns.storedNumber(NaN)), true)
    check("Infinity is not a width", isNaN(Columns.storedNumber(Infinity)), true)
}

// The peek cache key: path plus what ordered it, so a stale ancestor column never survives the hidden-last toggle.
function runPeekKey(check) {
    check("the plain order keys plain", Columns.peekKey("/a", false, false), "/a\n00")
    check("hidden keys apart", Columns.peekKey("/a", true, false), "/a\n10")
    check("hidden-last keys apart too", Columns.peekKey("/a", false, true), "/a\n01")
    check("both keys apart together", Columns.peekKey("/a", true, true), "/a\n11")
    check("an absent flag reads as off", Columns.peekKey("/a"), "/a\n00")
    check("a repair count asks apart from the pane's own",
        Columns.sentKey(Columns.peekKey("/a", false, false), 1) !== Columns.sentKey(Columns.peekKey("/a", false, false), 35), true)
    check("the same count asks the same", Columns.sentKey("/a\n00", 35), "/a\n00\n35")
    check("a folder order tags its key and direction", Columns.orderTag("size", true), "size1")
    check("an ascending order tags 0", Columns.orderTag("mtime", false), "mtime0")
    check("no order tags nothing", Columns.orderTag("", true), "")
    check("an absent order tags nothing", Columns.orderTag(undefined, undefined), "")
    check("a folder order joins the sent key", Columns.sentKey("/a\n00", 35, "size1"), "/a\n00\n35\nsize1")
    check("name order sends the key it always did", Columns.sentKey("/a\n00", 35, ""), "/a\n00\n35")
    check("a reply under the order before a re-sort answers no ask under the new one",
        Columns.hasAsk(Columns.trackAsk({}, Columns.sentKey("/a\n00", 35, "size1")), Columns.sentKey("/a\n00", 35, "")), false)
    var area = Source.source("ui/ColumnsArea.qml")
    check("the columns view matches a reply on its sent count and echoed order",
        area.indexOf("Columns.peekKey(path, hidden, hiddenLast), order = Columns.orderTag(by, desc), sent = Columns.sentKey(key, first, order)") >= 0, true)
    check("and asks with the pane's window size and the folder's order",
        area.indexOf("sent = Columns.sentKey(key, root.pane.windowSize, order)") >= 0, true)
    check("a reply under an order the folder has left is dropped",
        area.indexOf("if (order !== root.orderOf(path)) return") >= 0, true)
    check("while stored rows keep no count, so a resize orphans no column",
        area.indexOf("ViewState.state.hiddenLast === true, root.pane.windowSize)") < 0, true)
}

// Live cover: tests/ui.sh columns. Only a reply this view asked for lands in it, so a repair peek or a path-bar Tab never fills a column with wrong rows.
function runPeekPending(check) {
    var key = Columns.peekKey("/a", true, false), other = Columns.peekKey("/b", true, false)
    check("an ask is outstanding once tracked", Columns.hasAsk(Columns.trackAsk({}, key), key), true)
    check("anything else is another client's", Columns.hasAsk({}, key), false)
    check("a stored reply drops its own ask", Columns.hasAsk(Columns.dropAsk(Columns.trackAsk({}, key), key), key), false)
    check("a stored reply keeps a sibling ask", Columns.hasAsk(Columns.dropAsk(Columns.trackAsk(Columns.trackAsk({}, key), other), key), other), true)
    // A repair peek for the same path and flags is another client's when the count differs.
    var own = Columns.sentKey(Columns.peekKey("/a", false, false), 35), repair = Columns.sentKey(Columns.peekKey("/a", false, false), 1)
    var held = Columns.trackAsk({}, own)
    check("a first-1 reply for the same path and flags is refused while the pane's own ask stands",
        Columns.hasAsk(held, repair), false)
    check("while the pane's own reply is kept", Columns.hasAsk(held, own), true)
    check("and the repair reply drops no ask of its own",
        Columns.hasAsk(Columns.dropAsk(held, repair), own), true)
    var area = Source.source("ui/ColumnsArea.qml")
    check("the columns view tracks its own asks", area.indexOf("Columns.trackAsk(root.pending, sent)") >= 0, true)
    check("and stores only a reply it asked for", area.indexOf("if (!Columns.hasAsk(root.pending, sent)) return") >= 0, true)
    check("and drops the ask it stored", area.indexOf("Columns.dropAsk(root.pending, sent)") >= 0, true)
    check("and takes only the landing's matching drawn-folder asks", area.indexOf("root.pending = kept.pending") >= 0, true)
    var orderBody = Source.slice(area, "onPeekOrderChanged", "onChildPathChanged")
    check("and re-asks the shown ancestors under the new key", orderBody.indexOf("refreshNeighbours") >= 0, true)
    var peekBody = Source.slice(area, "readonly property string peekOrder", "onPeekOrderChanged")
    check("and the order it watches carries both flags",
        peekBody.indexOf("showHidden") >= 0 && peekBody.indexOf("hiddenLast") >= 0, true)
}

// The header drag writes once on release after a real move, so a click pins nothing and a fit survives its own release.
function runHeaderDrag(check) {
    var header = Source.source("ui/Header.qml")
    var beginBody = Source.slice(header, "function beginDrag", "function moveDrag")
    check("a press that never travels marks nothing to write",
        beginBody.indexOf("root.dragMoved = false") >= 0, true)
    var moveBody = Source.slice(header, "function moveDrag", "function endDrag")
    var guardAt = moveBody.indexOf("Math.abs(x - root.dragStartX) < 1"), armAt = moveBody.indexOf("root.dragMoved = true")
    check("only a travelled pointer arms the write",
        guardAt >= 0 && armAt > guardAt, true)
    check("and the release writes only when armed",
        Source.slice(header, "function endDrag", "function fittedWidth").indexOf("if (root.dragMoved)") >= 0, true)
    var fitBody = Source.slice(header, "function autofitColumn", "function autofitAll")
    check("a fit ends the drag so its release writes nothing",
        fitBody.indexOf('root.dragKey = ""') >= 0, true)
}

// The window passes the raw stored limit through, so a hand-edited false or "" reaches cappedLimit instead of coercing to 0.
function runColumnsLimitWire(check) {
    var area = Source.source("ui/ColumnsArea.qml")
    check("the limit is not an int property",
        area.indexOf("readonly property var columnsLimit") >= 0, true)
}

// ColumnFit.cellText names the strings ui/Row.qml draws, so autofit measures them: a link fits "link", a folder its walk size.
function runColumnFit(check) {
    check("a file fits its size", ColumnFit.cellText("size", {p: 33188, d: false, s: 18000}, [], null), "18.0 kB")
    check("a link fits its kind, not its target length",
        ColumnFit.cellText("size", {p: 41453, d: false, s: 18}, [], null), "link")
    check("a folder without a walk fits the bare mark",
        ColumnFit.cellText("size", {p: 16877, d: true, s: 60}, [], null), "·")
    check("a folder with a walk fits its size",
        ColumnFit.cellText("size", {p: 16877, d: true, s: 60}, [], {bytes: 124700000, partial: false}), "124.7 MB")
    check("a partial walk keeps its mark",
        ColumnFit.cellText("size", {p: 16877, d: true, s: 60}, [], {bytes: 1000, partial: true}), ">1.0 kB")
    check("a row with no mtime fits the bare mark",
        ColumnFit.cellText("date", {m: null}, [], null), "--")
    check("a kind past the dictionary reads empty, never a crash",
        ColumnFit.cellText("kind", {k: 99}, ["File"], null), "")
}

// SendPicker.html draws a chooser row as the name, a 70 px size and an 80 px date, and nothing
// else, so ui/PickerList.qml hands ui/Row.qml Picker.HIDDEN_COLS instead of the window's own set.
// Without it the chooser inherited whatever the header menu had switched on for the browser window.

function runPicker(check) {
    // The negative control: the chooser's slot affords all five, which is what it drew with the
    // window's set and Mode and Kind switched on.
    check("the chooser's own slot is wide enough for every column",
          Columns.names(Columns.set(PICKER_SLOT, PICKER)), "name,mode,size,date,kind")
    check("the chooser draws the board's three and nothing else",
          Columns.names(Columns.set(PICKER_SLOT, PICKER, Picker.HIDDEN_COLS)), "name,size,date")

    // Not only at that width: no width brings a column the chooser's board does not have.
    var everDrawn = false
    for (var w = 3000; w >= 0; w--) {
        var s = Columns.set(w, PICKER, Picker.HIDDEN_COLS)
        if (s.mode || s.kind)
            everDrawn = true
    }
    check("no width at all draws Mode or Kind in the chooser", everDrawn, false)
}

// e39 neighbour gate: refresh computes its asks fresh, so a width or path step asks the parent first time.
function runNeighbourAsks(check) {
    check("a narrow window asks no ancestor", Columns.neighbourAsks("/x/y", 800, 3).join("|"), "")
    check("widening asks the parent", Columns.neighbourAsks("/x/y", 1100, 3).join("|"), "/x")
    check("an empty path asks nothing", Columns.neighbourAsks("", 1100, 3).join("|"), "")
    check("a fresh path asks its parent", Columns.neighbourAsks("/x/y", 1100, 3).join("|"), "/x")
    check("at / no width asks anything", Columns.neighbourAsks("/", 2560, 5).join("|"), "")
    check("a wide window asks three ancestors", Columns.neighbourAsks("/a/b/c", 2560, 5).join("|"), "/|/a|/a/b")
    var area = Source.source("ui/ColumnsArea.qml")
    var refreshBody = Source.slice(area, "function refreshNeighbours", "function askMeta")
    check("refresh asks the fresh neighbour list", refreshBody.indexOf("Columns.neighbourAsks(root.pane.path, root.width, root.columnsLimit)") >= 0, true)
    check("and reads no stale show gate there", refreshBody.indexOf("showParent") < 0 && refreshBody.indexOf("parentShown") < 0, true)
    var moveBody = Source.slice(area, "Columns.folderDataHold(root.cursorIsDir", "folderFallback.stop()")
    check("a folder wait stops the old work cap first", moveBody.indexOf("thirdSwap.stopCap()") >= 0, true)
}

// e39 folder hold: the unanswered wait owns the bound, so file readiness and the old cap stay out of the swap.
function runFolderHold(check) {
    var area = Source.source("ui/ColumnsArea.qml")
    check("the wait follows the unanswered folder", area.indexOf("folderWaiting: Columns.folderDataHold(root.cursorIsDir, root.answered(root.childPath))") >= 0, true)
    check("the swap holds it", area.indexOf("folderHold: root.folderWaiting") >= 0, true)
    var swap = Source.source("ui/PreviewSwap.qml")
    check("structural: check() names the folder guard", Source.slice(swap, "function check()", "function release").indexOf("if (root.folderHold)") >= 0, true)
    check("structural: cap names the folder guard", Source.slice(swap, "id: cap", "The first sync queues a repaint").indexOf("if (root.folderHold)") >= 0, true)
    // Structural ordering only: a comment or string still saying return passes these; tests/preview-swap.qml proves the returns run.
    var checkBody = Source.slice(swap, "function check()", "function release")
    var checkGuard = checkBody.indexOf("if (root.folderHold)")
    var checkReturn = checkGuard >= 0 ? checkBody.indexOf("return", checkGuard) : -1
    var checkNext = checkGuard >= 0 ? checkBody.indexOf("if (root.holding", checkGuard) : -1
    check("structural: check() orders guard before release arm", checkGuard >= 0 && checkReturn > checkGuard && checkReturn < checkNext, true)
    var capBody = Source.slice(swap, "id: cap", "The first sync queues a repaint")
    var capGuard = capBody.indexOf("if (root.folderHold)")
    var capReturn = capGuard >= 0 ? capBody.indexOf("return", capGuard) : -1
    var capNext = capGuard >= 0 ? capBody.indexOf("if (!root.holding", capGuard) : -1
    check("structural: cap orders guard before fallback arm", capGuard >= 0 && capReturn > capGuard && capReturn < capNext, true)
    check("the folder guard runs under Qt in preview-swap", Source.source("tests/preview-swap.qml").indexOf('Quickshell.env("PREVIEW_SWAP_FOLDERGUARD")') >= 0, true)
}

// Middle click opens the drawn folder in a tab, so each column wires its own path.
function runTabWiring(check) {
    var area = Source.source("ui/ColumnsArea.qml")
    // Sample input: slice("id: a onTabRequested Tap.tappedTab(row, p) id: b", "id: a", "id: b").
    var greatTab = Source.slice(area, "id: greatGrandparentLoader", "id: grandparentLoader")
    var grandTab = Source.slice(area, "id: grandparentLoader", "id: parentColumn")
    var parentTab = Source.slice(area, "id: parentColumn", "id: active")
    var activeTab = Source.slice(area, "id: active", "id: childColumn")
    var childTab = Source.slice(area, "id: childColumn", "id: preview")
    check("great-grandparent tab stays in its own column", greatTab.split("onTabRequested").length - 1, 1)
    check("great-grandparent opens its drawn folder in a tab", greatTab.indexOf("Tap.tappedTab(row, root.greatGrandparentPath, root.pane)") >= 0, true)
    check("grandparent tab stays in its own column", grandTab.split("onTabRequested").length - 1, 1)
    check("grandparent opens its drawn folder in a tab", grandTab.indexOf("Tap.tappedTab(row, root.grandparentPath, root.pane)") >= 0, true)
    check("parent tab stays in its own column", parentTab.split("onTabRequested").length - 1, 1)
    check("parent opens its drawn folder in a tab", parentTab.indexOf("Tap.tappedTab(row, root.parentPath, root.pane)") >= 0, true)
    check("current tab stays in its own column", activeTab.split("onTabRequested").length - 1, 1)
    check("current opens its drawn folder in a tab", activeTab.indexOf("Tap.tappedTab(row, root.pane.path, root.pane)") >= 0, true)
    check("child tab stays in its own column", childTab.split("onTabRequested").length - 1, 1)
    check("child opens its drawn folder in a tab", childTab.indexOf("Tap.tappedTab(row, root.shownChildPath, root.pane)") >= 0, true)
    check("each of the five columns answers a tab", area.split("onTabRequested").length - 1, 5)
}

// w8 colroot: at / no ancestor repeats the active column, so the slot stays blank and Left stays a no-op.
function runColRoot(check) {
    check("at / no ancestor column is shown", Columns.ancestors("/", 3).join("|"), "")
    check("at /home only the parent (/) is", Columns.ancestors("/home", 3).join("|"), "/")
    check("at /home/gm parent and grandparent are", Columns.ancestors("/home/gm", 3).join("|"), "/|/home")
    check("one ancestor asked is the parent", Columns.ancestors("/home/gm", 1).join("|"), "/home")
    check("three deep names three ancestors", Columns.ancestors("/a/b/c", 3).join("|"), "/|/a|/a/b")
    check("the root shows no parent", Columns.ancestorShown("/", 1), false)
    check("a child of the root shows its parent", Columns.ancestorShown("/home", 1), true)
    check("a child of the root shows no grandparent", Columns.ancestorShown("/home", 2), false)
    check("depth two shows two ancestors", Columns.ancestorShown("/home/gm", 2), true)
    check("depth two shows no third ancestor", Columns.ancestorShown("/home/gm", 3), false)
    var distinct = true
    var paths = ["/", "/home", "/home/gm", "/a/b/c/d"]
    for (var i = 0; i < paths.length; i++) {
        var chain = Columns.ancestors(paths[i], 3).concat([paths[i]])
        for (var j = 0; j + 1 < chain.length; j++)
            if (chain[j] === chain[j + 1])
                distinct = false
    }
    check("no shown ancestor ever equals the column to its right", distinct, true)
    var atRoot = { path: "/", listingPath: "", listingState: "", listInFlight: false, pendingSelect: "kept", opened: [] }
    atRoot.open = function (target) { atRoot.opened.push(target) }
    Nav.parent(atRoot)
    check("left at / opens nothing", atRoot.opened.length, 0)
    check("and plants no select on the climb that did not happen", atRoot.pendingSelect, "kept")
    var area = Source.source("ui/ColumnsArea.qml")
    check("refresh computes its asks fresh, see runNeighbourAsks",
        area.indexOf("Columns.neighbourAsks(root.pane.path, root.width, root.columnsLimit)") >= 0, true)
    var greatBody = Source.slice(area, "id: greatGrandparentLoader", "id: grandparentLoader")
    check("great-grandparent draws rows only while its ancestor shows", greatBody.indexOf("root.greatGrandparentShown ? root.rowsFor(root.greatGrandparentPath)") >= 0, true)
    var grandBody = Source.slice(area, "id: grandparentLoader", "id: parentColumn")
    check("grandparent draws rows only while its ancestor shows", grandBody.indexOf("root.grandparentShown ? root.rowsFor(root.grandparentPath)") >= 0, true)
    var parentBody = Source.slice(area, "id: parentColumn", "id: active\n")
    check("parent draws rows only while its ancestor shows", parentBody.indexOf("(root.showParent && root.parentShown) ? root.rowsFor(root.parentPath)") >= 0, true)
    var readerBody = Source.slice(area, "function parentItemAt", "function grandparentItemAt")
    check("parent reader answers null while its ancestor is hidden", readerBody.indexOf("(root.showParent && root.parentShown) ? parentColumn.itemAtIndex(index) : null") >= 0, true)
}

// The user's own hidden set, subtracted from what the width affords: a hidden column never draws,
// and width still wins, so a column shown while the pane is too narrow stays dropped. The keys are
// the same "mode"/"size"/"date"/"kind" the header menu's col:<key> actions carry.
function runHidden(check) {
    var none = Columns.set(2000, BOX, [])
    check("an empty hidden set draws every column the width affords",
          [none.mode, none.size, none.date, none.kind].join(","), "true,true,true,true")

    var hid = Columns.set(2000, BOX, ["size", "kind"])
    check("a hidden column does not draw at a width that would afford it",
          [hid.mode, hid.size, hid.date, hid.kind].join(","), "true,false,true,false")

    var narrow = Columns.set(200, BOX, ["kind"])
    check("width still wins over a column the user wants back, Mode's own floor included",
          [narrow.mode, narrow.size, narrow.date, narrow.kind].join(","), "false,false,false,false")

    // #333: a hidden column reserves no width, so it cannot push out a column the user turned on.
    // 568 is the name's 216 plus size, date and kind with their gaps, Mode's 79 left out.
    check("a hidden column releases its width to the columns after it",
          Columns.names(Columns.set(568, BOX, ["mode"])), "name,size,date,kind")
    check("and the columns still drop one pixel under the floor that is left",
          Columns.names(Columns.set(567, BOX, ["mode"])), "name,size,date")
    check("hidden floors leave the hidden column's own floor out",
          [Columns.floors(BOX, ["mode"]).size, Columns.floors(BOX, ["mode"]).kind].join(","), "295,568")
    // The report's ui.json: Mode hidden at a stored 480, Size and Modified at 480, Kind at 295, in a 1388 px list.
    var stored = Object.assign({}, BOX, { mode: 480, size: 480, date: 480, kind: 295 })
    check("a wide hidden Mode no longer hides Modified",
          Columns.names(Columns.set(1388, stored, ["mode"])), "name,size,date")

    var undefinedSet = Columns.set(2000, BOX)
    check("a caller that passes no hidden set draws as before",
          [undefinedSet.mode, undefinedSet.size, undefinedSet.date, undefinedSet.kind].join(","), "true,true,true,true")
}

// Neighbour-column backgrounds (e41): a peek's empty space navigates to its drawn directory and opens the background menu once the rows land, never over the wrong path.
function runColumnMenu(check) {
    check("a trailing slash trims except at the root", ColumnMenu.trimSlash("/a/"), "/a")
    check("the root keeps its own slash", ColumnMenu.trimSlash("/"), "/")
    check("same path ignores a trailing slash", ColumnMenu.samePath("/a/", "/a"), true)
    check("different paths stay different", ColumnMenu.samePath("/a", "/b"), false)
    check("an idle pane may arm", ColumnMenu.canArm({}), "")
    check("a listing in flight refuses before any intent", ColumnMenu.canArm({listInFlight: true}), "loading")
    check("an open trash refuses", ColumnMenu.canArm({trash: {opened: true}}), "trash")
    check("a confirming trash refuses", ColumnMenu.canArm({trash: {confirming: true}}), "trash")
    check("a search refuses", ColumnMenu.canArm({searchMode: "typing"}), "search")
    check("a card waiting refuses", ColumnMenu.canArm({collide: {pending: {}}}), "collide")
    check("an open menu refuses a second", ColumnMenu.canArm({menuVisible: true}), "menu")
    check("a menu action in flight refuses too", ColumnMenu.canArm({menuActions: {opened: true}}), "menu")
    check("an open rename refuses", ColumnMenu.canArm({renamingIndex: 2}), "rename")
    check("a pending rename refuses as well", ColumnMenu.canArm({renamePending: true}), "rename")
    check("a filter line with the caret refuses", ColumnMenu.canArm({filterTyping: true}), "filter")
    check("a selection band refuses", ColumnMenu.canArm({selectionBand: {}}), "band")
    check("an open collision card refuses", ColumnMenu.canArm({collide: {opened: true}}), "collide")
    check("a null collision slot arms", ColumnMenu.canArm({collide: {pending: null}}), "")
    check("an empty trash slot arms", ColumnMenu.canArm({trash: {}}), "")
    check("no pane refuses", ColumnMenu.canArm(null), "no pane")
    check("the shown folder opens where it stands", ColumnMenu.directOpen("/a", "/a/"), true)
    check("another folder navigates instead", ColumnMenu.directOpen("/a", "/b"), false)
    check("no target navigates nowhere", ColumnMenu.directOpen("", "/b"), false)
    function bgRoute(overrides) {
        var p = {path: "/a", listInFlight: false, pendingBackground: "", pendingBackgroundAt: null, said: [], opened: [], asked: []}
        p.message = function (t) { p.said.push(t) }
        p.open = function (base) { p.asked.push(base); if (overrides && overrides.starts === true) p.listInFlight = true }
        var m = {opened: [], openBackground: function (at) { m.opened.push(at) }}
        return {pane: p, menu: m}
    }
    var sampleMenu = {opened: 0, openBackground: function () { sampleMenu.opened++ }}
    check("the sample opens the shown folder where it stands", ColumnMenu.routeBackground({path: "/a"}, "/a", {x: 1}, sampleMenu), "opened")
    check("through the real menu entrance", sampleMenu.opened, 1)
    var idle = bgRoute({starts: true})
    check("an idle hop arms and navigates", ColumnMenu.routeBackground(idle.pane, "/b", {x: 1, y: 2}, idle.menu), "navigating")
    check("with the drawn target stored", idle.pane.pendingBackground, "/b")
    check("and the listing asked for", idle.pane.asked.join("|"), "/b")
    var busyRoute = bgRoute()
    busyRoute.pane.listInFlight = true
    check("a busy hop refuses before storing anything", ColumnMenu.routeBackground(busyRoute.pane, "/b", {x: 1, y: 2}, busyRoute.menu), "refused:loading")
    check("leaving no intent behind", busyRoute.pane.pendingBackground, "")
    check("and asking for no listing", busyRoute.pane.asked.length, 0)
    check("while the refusal says to wait", busyRoute.pane.said.join("|"), "A directory is already loading.")
    var same = bgRoute()
    check("the shown folder opens where it stands", ColumnMenu.routeBackground(same.pane, "/a/", {x: 3, y: 4}, same.menu), "opened")
    check("leaving the listing alone", same.pane.asked.length, 0)
    check("at the stored point", same.menu.opened.length, 1)
    var unstated = bgRoute()
    check("a targetless tap is ignored", ColumnMenu.routeBackground(unstated.pane, "", {x: 1, y: 2}, unstated.menu), "ignored")
    var unstarted = bgRoute()
    check("an open that never starts is refused", ColumnMenu.routeBackground(unstarted.pane, "/b", {x: 1, y: 2}, unstarted.menu), "refused:not-started")
    check("clearing the intent it just stored", unstarted.pane.pendingBackground, "")
    check("a ready listing opens its pending menu", ColumnMenu.shouldOpen("/a", "/a", "ready"), true)
    check("an empty listing opens too", ColumnMenu.shouldOpen("/a", "/a/", "empty"), true)
    check("a locked listing drops it", ColumnMenu.shouldOpen("/a", "/a", "locked"), false)
    check("an error drops it", ColumnMenu.shouldOpen("/a", "/a", "error"), false)
    check("a loading listing opens nothing", ColumnMenu.shouldOpen("/a", "/a", "loading"), false)
    check("a later unrelated listing opens nothing", ColumnMenu.shouldOpen("/b", "/a", "ready"), false)
    check("nothing pending opens nothing", ColumnMenu.shouldOpen("/a", "", "ready"), false)
    var bg = {path: "/a", listingState: "ready", searchMode: "", trash: {},
        pendingBackground: "/a", pendingBackgroundAt: {x: 1, y: 2}, opened: []}
    bg.openBackgroundMenu = function (at) { bg.opened.push(at) }
    Nav.applyPendingBackground(bg)
    check("the drawn directory opens its menu once the rows land", bg.opened.length, 1)
    check("and the intent is consumed", bg.pendingBackground, "")
    var stale = {path: "/b", listingState: "ready", searchMode: "", trash: {},
        pendingBackground: "/a", pendingBackgroundAt: {x: 1, y: 2}, opened: []}
    stale.openBackgroundMenu = function (at) { stale.opened.push(at) }
    Nav.applyPendingBackground(stale)
    check("a later unrelated listing consumes without opening", stale.opened.length, 0)
    check("and leaves nothing pending", stale.pendingBackground, "")
    var locked = {path: "/a", listingState: "locked", searchMode: "", trash: {},
        pendingBackground: "/a", pendingBackgroundAt: {x: 1, y: 2}, opened: []}
    locked.openBackgroundMenu = function (at) { locked.opened.push(at) }
    Nav.applyPendingBackground(locked)
    check("an unreadable listing drops without opening", locked.opened.length, 0)
    check("and leaves nothing pending", locked.pendingBackground, "")
    var flight = {swap: {drop: function () {}}, listInFlight: true, listedSeen: true}
    check("a stale error ends only its own request", Swap.failListing(flight, "stale"), false)
    check("leaving the armed listing in flight", flight.listInFlight, true)
    var armed = {path: "/b", listingState: "ready", searchMode: "", trash: {},
        pendingBackground: "/b", pendingBackgroundAt: {x: 9, y: 9}, opened: []}
    armed.openBackgroundMenu = function (at) { armed.opened.push(at) }
    Nav.applyPendingBackground(armed)
    check("the armed menu still opens on its own rows", armed.opened.length, 1)
    Nav.applyPendingBackground(armed)
    check("and only once", armed.opened.length, 1)
    var failedFlight = {swap: {phase: Swap.idle(), drop: function () {}}, listInFlight: true, listedSeen: true}
    check("an actual list failure ends the listing", Swap.failListing(failedFlight, "scan"), true)
    var doomed = {path: "/b", listingState: "ready", searchMode: "", trash: {},
        pendingBackground: "/b", pendingBackgroundAt: {x: 1, y: 1}, opened: []}
    doomed.openBackgroundMenu = function (at) { doomed.opened.push(at) }
    Nav.clearPendingBackground(doomed)
    Nav.applyPendingBackground(doomed)
    check("a failed listing drops the intent before any rows", doomed.opened.length, 0)
    var failed = {pendingBackground: "/b", pendingBackgroundAt: {x: 1, y: 2}}
    Nav.clearPendingBackground(failed)
    check("a failed listing clears the intent", failed.pendingBackground, "")
    check("with nothing left to open on", failed.pendingBackgroundAt, null)
    Nav.applyPendingBackground(bg)
    check("a consumed intent never opens twice", bg.opened.length, 1)
    // The actual PaneWire onFailed callback, compiled from shipped source and run under realistic doubles: the only execution noticing a dropped return or an if(true).
    // Sample input: "function onFailed(where, input, message, mode) {" opens the brace scan at depth 1.
    var onFailedMark = "function onFailed(where, input, message, mode) {"
    var onFailedAt = Source.source("ui/PaneWire.qml").indexOf(onFailedMark)
    if (onFailedAt < 0)
        throw new Error("sourcefixture: missing onFailed")
    var onFailedSrc = Source.source("ui/PaneWire.qml")
    var scanAt = onFailedAt + onFailedMark.length, depth = 1, quote = "", lineComment = false
    while (depth > 0 && scanAt < onFailedSrc.length) {
        var ch = onFailedSrc.charAt(scanAt)
        if (lineComment) {
            if (ch === "\n") lineComment = false
        } else if (quote.length > 0) {
            if (ch === quote) quote = ""
        } else if (ch === "/" && onFailedSrc.charAt(scanAt + 1) === "/") lineComment = true
        else if (ch === '"' || ch === "'") quote = ch
        else if (ch === "{") depth += 1
        else if (ch === "}") depth -= 1
        scanAt += 1
    }
    if (depth > 0)
        throw new Error("sourcefixture: unterminated onFailed")
    var onFailed = eval("(function (pane, root, where, input, message, mode) {"
        + onFailedSrc.substring(onFailedAt + onFailedMark.length, scanAt - 1) + "})")
    function failedDoubles() {
        var p = {path: "/a", listingPath: "/b", listInFlight: true, listedSeen: true,
            pendingMenu: true, pendingBackground: "/b", pendingBackgroundAt: {x: 1, y: 1},
            total: 5, held: 0, rows: [], kindNames: [], cursorIndex: 0, renamingIndex: -1,
            renameRequest: null, renamePending: false, renameKeepsPointerRow: false, renameError: "",
            transfer: {id: 0}, searchMode: "", clipPending: null, pathsPending: null,
            listingState: "loading", stateMessage: "", lockedMode: 0, said: [], stuck: []}
        p.message = function (t) { p.said.push(t) }
        p.sticky = function (t) { p.stuck.push(t) }
        p.swap = {phase: Swap.idle(), drop: function () {}, landParked: function () { return false }}
        p.backend = {heldListing: 0}
        var r = {renameOnArrival: "", stale: false, anchor: null, retryId: 0, retryPaths: [],
            retryFolder: "", retryListing: "", retrySelectionText: ""}
        return {pane: p, root: r}
    }
    var staleFailed = failedDoubles()
    onFailed(staleFailed.pane, staleFailed.root, "stale", "trash [0]", "rows out of date", 0)
    check("a stale failure keeps the background intent", staleFailed.pane.pendingBackground, "/b")
    check("and keeps the row intent with it", staleFailed.pane.pendingMenu, true)
    check("and leaves the armed listing in flight", staleFailed.pane.listInFlight, true)
    var sortFailed = failedDoubles()
    onFailed(sortFailed.pane, sortFailed.root, "sort", "mode", "no such order", 0)
    check("a refused sort keeps both intents too", sortFailed.pane.pendingBackground, "/b")
    check("and the row one beside it", sortFailed.pane.pendingMenu, true)
    var scanFailed = failedDoubles()
    onFailed(scanFailed.pane, scanFailed.root, "scan", "/b", "permission denied", 0)
    check("an actual list failure drops the background intent", scanFailed.pane.pendingBackground, "")
    check("and the row intent with it", scanFailed.pane.pendingMenu, false)
    check("and ends the listing", scanFailed.pane.listInFlight, false)
    var deadBackend = failedDoubles()
    onFailed(deadBackend.pane, deadBackend.root, "backend", "", "child is gone", 0)
    check("a dead backend drops both intents at once", deadBackend.pane.pendingBackground, "")
    check("with the row one beside it", deadBackend.pane.pendingMenu, false)
    var afterSearch = failedDoubles()
    afterSearch.pane.searchGen = 3
    afterSearch.pane.pendingSelect = "/b/kept.txt"
    onFailed(afterSearch.pane, afterSearch.root, "scan", "/b", "permission denied", 0)
    check("a listing failing after a search ended keeps the cursor file it armed", afterSearch.pane.pendingSelect, "/b/kept.txt")
    var relistFailed = failedDoubles()
    relistFailed.pane.searchGen = 3
    relistFailed.pane.pendingSelect = "/b/kept.txt"
    relistFailed.pane.swap.phase = Swap.begin(Swap.idle(), "ready", "", {holdWalk: true, relist: true}, 0)
    onFailed(relistFailed.pane, relistFailed.root, "scan", "/b", "permission denied", 0)
    check("while the Esc relist failing leaves none armed", relistFailed.pane.pendingSelect, "")
    // A stranded twin with a matching request closes it, and an undo-shaped one with none re-reads the listing.
    function strandedDoubles(request) {
        var p = {path: "/a", listingPath: "/a", listInFlight: false, listedSeen: false,
            pendingMenu: false, pendingBackground: "", pendingBackgroundAt: null,
            total: 0, held: 0, rows: [], kindNames: [], cursorIndex: 0, renamingIndex: 2,
            renameRequest: request, renamePending: false, renameKeepsPointerRow: false, renameError: "",
            transfer: {id: 0}, searchMode: "", clipPending: null, pathsPending: null,
            listingState: "ready", stateMessage: "", lockedMode: 0, said: [], refreshed: []}
        p.message = function (t) { p.said.push(t) }
        p.refresh = function (sel) { p.refreshed.push(sel) }
        p.renameEditor = function () { return null }
        p.swap = {phase: Swap.idle(), drop: function () {}, landParked: function () { return false }}
        p.backend = {heldListing: 0}
        var r = {renameOnArrival: "", stale: false, anchor: null, retryId: 0, retryPaths: [],
            retryFolder: "", retryListing: "", retrySelectionText: "", renamed: []}
        r.refreshRename = function (req, sel) { r.renamed.push(sel) }
        return {pane: p, root: r}
    }
    var strandedTwin = strandedDoubles({source: "/a/old.txt", destination: "/a/new.txt"})
    onFailed(strandedTwin.pane, strandedTwin.root, "rename-stranded", "/a/old.txt", "input/output failed", 0)
    check("a stranded rename closes its request", strandedTwin.pane.renameRequest, null)
    check("a stranded rename ends the edit selecting nothing", strandedTwin.root.renamed.length === 1 && strandedTwin.root.renamed[0] === "", true)
    var strandedUndo = strandedDoubles(null)
    onFailed(strandedUndo.pane, strandedUndo.root, "rename-stranded", "/a/old.txt", "input/output failed", 0)
    check("an undo-shaped stranded failure re-reads the listing selecting nothing", strandedUndo.pane.refreshed.length === 1 && strandedUndo.pane.refreshed[0] === "", true)
    function bgListing() {
        var p = {listInFlight: false, path: "/a", listingPath: "", pendingBackground: "", pendingBackgroundAt: null,
            searchMode: "", filterQuery: "", filterTyping: false, listingState: "ready", stateMessage: "", lockedMode: 0,
            total: 1, held: 0, rows: [], kindNames: [], thumbState: null, dirSizeState: null, cursorIndex: 0,
            trashArmedAt: 0, renamingIndex: -1, storageClass: "", storageKnown: true, windowSize: 10, showHidden: false,
            listingPreferences: "", appliedListingPreferences: "", said: [], sent: []}
        p.message = function (t) { p.said.push(t) }
        p.clearSelection = function () {}
        p.listArea = {primeSettle: function () {}}
        p.swap = {hold: function () { return false }}
        p.backend = {list: function (path) { p.sent.push(path) }, askFsInfo: function () {}}
        return p
    }
    var keeps = bgListing()
    keeps.pendingBackground = "/b"
    keeps.pendingBackgroundAt = {x: 1, y: 1}
    Nav.openWithoutHistory(keeps, "/b")
    check("a hop to the armed target keeps its request", keeps.sent.join("|"), "/b")
    check("and keeps the intent for its landing", keeps.pendingBackground, "/b")
    var drops = bgListing()
    drops.pendingBackground = "/b"
    drops.pendingBackgroundAt = {x: 1, y: 1}
    Nav.openWithoutHistory(drops, "/c")
    check("a hop elsewhere still asks for its own directory", drops.sent.join("|"), "/c")
    check("while the waiting intent is dropped", drops.pendingBackground, "")
    var pane = Source.source("ui/ColumnPane.qml")
    check("a peek's empty space has its own route", pane.indexOf("signal neighbourBackgroundRequested(var eventPoint)") >= 0, true)
    var bgBody = Source.slice(pane, "Empty space below the last row", "delegate: Flea.ColumnRow")
    check("and a peek emits it instead of the active menu", bgBody.indexOf("root.neighbourBackgroundRequested(eventPoint)") >= 0, true)
    var area = Source.source("ui/ColumnsArea.qml")
    check("the area navigates a neighbour background", area.indexOf("function menuOnNeighbourBackground(base, eventPoint)") >= 0, true)
    var helper = Source.slice(area, "function menuOnNeighbourBackground", "function parentItemAt")
    check("the helper delegates to the executed routing", helper.indexOf("ColumnMenu.routeBackground(root.pane, base,") >= 0, true)
    check("the parent routes its drawn directory", area.indexOf("menuOnNeighbourBackground(root.parentPath, eventPoint)") >= 0, true)
    check("the parent stays gated on its shown ancestor", area.indexOf("if (root.showParent && root.parentShown)") >= 0, true)
    check("the grandparent routes its own", area.indexOf("menuOnNeighbourBackground(root.grandparentPath, eventPoint)") >= 0, true)
    check("the grandparent stays gated on its shown ancestor", area.indexOf("if (root.showGrandparent && root.grandparentShown)") >= 0, true)
    check("the great-grandparent routes its own", area.indexOf("menuOnNeighbourBackground(root.greatGrandparentPath, eventPoint)") >= 0, true)
    check("the great-grandparent stays gated on its shown ancestor", area.indexOf("if (root.showGreatGrandparent && root.greatGrandparentShown)") >= 0, true)
    check("the child routes its drawn folder", area.indexOf("menuOnNeighbourBackground(root.shownChildPath, eventPoint)") >= 0, true)
    var childBody = Source.slice(area, "id: childColumn", "Flea.SelectionPreview")
    check("and the child waits on the shown folder, never the pending one",
        childBody.indexOf("root.shownChildPath") >= 0 && childBody.indexOf("root.childPath") < 0, true)
    check("and a file preview offers no directory", childBody.indexOf("root.shownIsDir") >= 0, true)
    var wire = Source.source("ui/PaneWire.qml")
    check("a failed listing drops the intent", wire.indexOf("Nav.clearPendingBackground(pane)") >= 0, true)
    var failedHead = Source.slice(wire, "function onFailed(where, input, message, mode) {", "var terminal =")
    check("unrelated errors keep the intent until the listing ends",
        failedHead.indexOf("pendingMenu") < 0 && failedHead.indexOf("clearPendingBackground") < 0, true)
    var deadBackend = Source.slice(wire, "A dead backend ends every listing", "var request = pane.renameRequest")
    check("a dead backend still drops both deferred menus",
        deadBackend.indexOf("pane.pendingMenu = false") >= 0 && deadBackend.indexOf("Nav.clearPendingBackground(pane)") >= 0, true)
    var failedTail = Source.slice(wire, "if (!listingEnded) {", "Neither the child")
    check("the ended listing drops both deferred menus",
        failedTail.indexOf("pane.pendingMenu = false") >= 0 && failedTail.indexOf("Nav.clearPendingBackground(pane)") >= 0, true)
    var nav = Source.source("ui/js/Nav.js")
    check("a landed listing consumes it by identity", nav.indexOf("ColumnMenu.applyPendingBackground(pane)") >= 0, true)
    check("a hop elsewhere drops the waiting intent", nav.indexOf("ColumnMenu.samePath(newPath, pane.pendingBackground)") >= 0, true)
    var menu = Source.source("ui/js/ColumnMenu.js")
    check("the consume opens only its own successful listing",
        menu.indexOf("ColumnMenu.shouldOpen") < 0 && menu.indexOf("shouldOpen(pane.path, target, pane.listingState)") >= 0, true)
}
