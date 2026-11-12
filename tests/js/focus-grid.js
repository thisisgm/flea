.import "../../ui/js/Grid.js" as Grid
.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Marks.js" as Marks
.import "filterfixture.js" as Fixture

// Issue 162, muellan: grid j/k must follow the same visual neighbours as Down/Up.

// The pane ui/js/Focus.js reads, plus ui/Pane.qml's two wirings: extendSelection is Marks.extend.
function pane(viewMode) {
    var p = Fixture.pane()
    p.viewMode = viewMode
    p.cursorStride = 3
    p.preview = { active: false, isMedia: false, isPdf: false }
    p.searchMode = ""
    p.recentMode = ""
    p.renameEditor = function () { return null }
    p.message = function () {}
    p.act = function (action) { Focus.act(action, p) }
    p.extendSelection = function (delta) { Marks.extend(p, delta) }
    return p
}

function run(check) {
    var none = 0
    var shift = Qt.ShiftModifier
    var ctrl = Qt.ControlModifier
    function key(code, text, modifiers) {
        return { key: code, text: text, modifiers: modifiers }
    }
    var jPress = key(Qt.Key_J, "j", none)
    var kPress = key(Qt.Key_K, "k", none)
    var hPress = key(Qt.Key_H, "h", none)
    var lPress = key(Qt.Key_L, "l", none)
    var shiftJ = key(Qt.Key_J, "J", shift)
    var ctrlH = key(Qt.Key_H, "h", ctrl)

    var gridPane = pane("grid")
    gridPane.wrapAtEnds = true

    // j and k are a row of tiles at a time, from any cell they can leave.
    for (var step of [
        [jPress, "cursorDown", 0, 3], [jPress, "cursorDown", 2, 5], [jPress, "cursorDown", 3, 6],
        [kPress, "cursorUp", 3, 0], [kPress, "cursorUp", 5, 2], [kPress, "cursorUp", 6, 3]
    ]) {
        gridPane.cursorIndex = step[2]
        check("the grid takes " + step[0].text + " from " + step[2],
              Grid.arrow(step[0], step[1], gridPane), true)
        check("and " + step[0].text + " from " + step[2] + " lands on " + step[3],
              gridPane.cursorIndex, step[3])
    }

    // The issue's claim as an equivalence over every row: a letter and its arrow land on one cell.
    for (var pair of [[Qt.Key_Down, jPress, "cursorDown"], [Qt.Key_Up, kPress, "cursorUp"]]) {
        for (var from = 0; from < 7; from++) {
            gridPane.cursorIndex = from
            Grid.arrow(key(pair[0], "", none), pair[2], gridPane)
            var arrow = gridPane.cursorIndex
            gridPane.cursorIndex = from
            Grid.arrow(pair[1], pair[2], gridPane)
            check(pair[1].text + " from " + from + " lands where " + pair[2] + " does",
                  gridPane.cursorIndex, arrow)
        }
    }

    // The arrows themselves, unchanged: Down and Up move a tile row, Left and Right one tile.
    for (var move of [
        [Qt.Key_Right, "cursorRight", 2, 2], [Qt.Key_Left, "cursorLeft", 3, 3],
        [Qt.Key_Down, "cursorDown", 2, 5], [Qt.Key_Down, "cursorDown", 5, 5],
        [Qt.Key_Down, "cursorDown", 3, 6], [Qt.Key_Up, "cursorUp", 6, 3],
        [Qt.Key_Up, "cursorUp", 0, 0], [Qt.Key_Right, "cursorRight", 6, 6]
    ]) {
        gridPane.cursorIndex = move[2]
        Grid.arrow(key(move[0], "", none), move[1], gridPane)
        check("grid visual neighbour from " + move[2] + " with " + move[1], gridPane.cursorIndex, move[3])
    }

    // Issue 114, muellan: h and l clamp at the row's edge exactly as the arrows do.
    for (var walk of [
        [hPress, "cursorLeft", 3, 3], [hPress, "cursorLeft", 4, 3],
        [lPress, "cursorRight", 5, 5], [lPress, "cursorRight", 4, 5]
    ]) {
        gridPane.cursorIndex = walk[2]
        check("the grid takes " + walk[0].text + " from " + walk[2],
              Grid.arrow(walk[0], walk[1], gridPane), true)
        check("and " + walk[0].text + " from " + walk[2] + " leaves the cursor on " + walk[3],
              gridPane.cursorIndex, walk[3])
    }

    // The horizontal pair keeps its action guard: a foreign action behind h or Left falls through.
    gridPane.cursorIndex = 3
    check("h with a foreign action falls through", Grid.arrow(hPress, "parent", gridPane), false)
    check("and leaves the cursor alone", gridPane.cursorIndex, 3)
    check("Left with a foreign action falls through", Grid.arrow(key(Qt.Key_Left, "", none), "parent", gridPane), false)
    check("cursorLeft with no h or Left behind it does not move", Grid.arrow(key(Qt.Key_Down, "", none), "cursorLeft", gridPane), false)

    // The partial last row (seven rows over three columns leaves one cell) and the first row.
    for (var edge of [4, 5, 6]) {
        gridPane.cursorIndex = edge
        Grid.arrow(jPress, "cursorDown", gridPane)
        check("j from the partial last row's " + edge + " stays there", gridPane.cursorIndex, edge)
    }
    gridPane.cursorIndex = 0
    Grid.arrow(kPress, "cursorUp", gridPane)
    check("k at the first row stays there", gridPane.cursorIndex, 0)

    // A reflowed grid reads the new column count for the letters the way it does for the arrow.
    gridPane.cursorStride = 2
    gridPane.cursorIndex = 3
    Grid.arrow(key(Qt.Key_Down, "", none), "cursorDown", gridPane)
    check("grid arrows use the reflowed column count", gridPane.cursorIndex, 5)
    gridPane.cursorIndex = 3
    Grid.arrow(jPress, "cursorDown", gridPane)
    check("grid j uses the reflowed column count too", gridPane.cursorIndex, 5)

    // Filtered coordinates: "screen" matches rows 0, 5 and 6, so a two-tile row's third cell is row 6.
    gridPane.filterQuery = "screen"
    gridPane.refresh()
    gridPane.cursorIndex = 0
    Grid.arrow(key(Qt.Key_Down, "", none), "cursorDown", gridPane)
    check("filtered grid arrows address visible cells", gridPane.cursorIndex, 6)
    gridPane.cursorIndex = 0
    Grid.arrow(jPress, "cursorDown", gridPane)
    check("filtered grid j addresses visible cells too", gridPane.cursorIndex, 6)
    gridPane.cursorIndex = 6
    Grid.arrow(jPress, "cursorDown", gridPane)
    check("filtered grid j clamps at the last visible cell", gridPane.cursorIndex, 6)
    Grid.arrow(kPress, "cursorUp", gridPane)
    check("filtered grid k steps the same visible row back", gridPane.cursorIndex, 0)
    gridPane.cursorIndex = 6
    Grid.arrow(key(Qt.Key_Right, "", none), "cursorRight", gridPane)
    check("filtered final row has no right cell", gridPane.cursorIndex, 6)
    gridPane.filterQuery = ""
    gridPane.refresh()
    gridPane.cursorStride = 3

    // Every preset spells j and k on the same bare letters, and Focus.lookup is the whole route.
    var wasPreset = Keymap.preset
    for (var preset of Keymap.PRESETS) {
        Keymap.setPreset(preset)
        check(preset + " routes j to cursorDown", Focus.lookup(jPress, gridPane), "cursorDown")
        check(preset + " routes k to cursorUp", Focus.lookup(kPress, gridPane), "cursorUp")
    }
    Keymap.setPreset(wasPreset)

    // A modifier takes the letter out of the pair: ctrl+j resolves to nothing, and shift+J extends.
    check("ctrl j is not an arrow", Focus.lookup(key(Qt.Key_J, "", ctrl), gridPane), "")
    check("shift J is the extension pair", Focus.lookup(shiftJ, gridPane), "extendDown")
    gridPane.cursorIndex = 1
    gridPane.selection.clear()
    gridPane.selectionVersion = 0
    Grid.arrow(shiftJ, "extendDown", gridPane)
    check("the grid strides the extension by a row of tiles", gridPane.cursorIndex, 4)
    check("and selects the visual range it crossed", Fixture.picks(gridPane), "1,2,3,4")

    // The claim is the four steps and nothing else: g is still the listing's jump.
    check("g is not a tile step", Grid.arrow(key(Qt.Key_G, "g", none), "cursorFirst", gridPane), false)

    // The real key route strides in the grid and retains item order and wrap in the list.
    var routed = pane("grid")
    routed.cursorIndex = 2
    check("the pane's own key path consumes j in the grid", Focus.handleKey(jPress, routed, null), true)
    check("and it lands one row of tiles down, not one item", routed.cursorIndex, 5)
    var listed = pane("list")
    listed.cursorIndex = 2
    Focus.handleKey(jPress, listed, null)
    check("in the list j is still one item", listed.cursorIndex, 3)
    listed.wrapAtEnds = true
    listed.cursorIndex = 6
    Focus.handleKey(jPress, listed, null)
    check("and the list keeps the wrap the setting asks for", listed.cursorIndex, 0)

    // Modified horizontal input retains its preset binding.
    Keymap.setPreset("default")
    check("a modified h is not the grid's step", Focus.lookup(ctrlH, gridPane), "")
    routed.cursorIndex = 2
    Focus.handleKey(ctrlH, routed, null)
    check("through the pane, ctrl h does not move the grid cursor", routed.cursorIndex, 2)
    Keymap.setPreset("windows")
    check("a binding that claims ctrl h keeps it in the grid", Focus.lookup(ctrlH, gridPane), "toggleHidden")
    Keymap.setPreset(wasPreset)
}
