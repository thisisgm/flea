.import "../../ui/js/Marquee.js" as Marquee
.import "../../ui/js/Selection.js" as Selection
.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Tap.js" as Tap
.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap

function pane() {
    return { selection: Selection.create(), selectionVersion: 0, selectionAnchor: 0, cursorIndex: 0,
        shown: null, shownTotal: 100,
        selectedIndices: function () { return this.selection.indices() },
        showRow: function () {},
        setCursor: function (index) { this.cursorIndex = index },
        commitOpenRename: function () {},
        selectOnly: function (index) { this.selection.only(index); this.cursorIndex = index; this.selectionAnchor = index },
        toggleSelectAt: function (index) { this.selection.toggle(index); this.cursorIndex = index; this.selectionAnchor = index },
        extendSelectionTo: function (index) { Filter.extendToRow(this, index) }
    }
}

function run(check) {
    function box(x1, y1, x2, y2, columns, count) { return Marquee.cells(x1, y1, x2, y2, 100, 20, columns || 1, count || 100) }
    function update(p, state, rectangle, columns, count, reverse, map) {
        Marquee.update(p, state, rectangle, columns || 1, count || 100, false, reverse || false,
            map || function (index) { return index })
    }
    check("a zero-width band marks nothing", box(2, 0, 2, 50), null)
    check("a band right of all cells marks nothing", box(101, 0, 120, 50), null)
    check("a band above all cells marks nothing", box(1, -50, 50, -1), null)
    check("an exact bottom boundary excludes the next row", box(1, 0, 50, 80).bottom, 3)
    var p = pane()
    p.selection.only(20)
    p.cursorIndex = 20
    p.selectionAnchor = 18
    var before = p.selection
    var state = Marquee.begin(p, false)
    update(p, state, box(50, 90, 1, 21), 1, 100, true)
    check("upward band marks four intersecting rows live", p.selectedIndices().join(","), "1,2,3,4")
    check("a plain band preserves its rollback object", before.indices().join(","), "20")
    check("the cursor stays still while the band moves", p.cursorIndex, 20)
    check("the last row to enter follows upward motion", state.last, 1)
    var version = p.selectionVersion
    update(p, state, box(50, 89, 1, 22), 1, 100, true)
    check("moving within the same cells emits no selection update", p.selectionVersion, version)
    Marquee.finish(p, state, false)
    check("release moves the cursor to the last entering row", p.cursorIndex, 1)
    check("release leaves marks for keyboard operations", p.selection.count(), 4)

    state = Marquee.begin(p, false)
    update(p, state, box(1, 50, 50, 99))
    check("a fresh plain band replaces the old marks", p.selectedIndices().join(","), "2,3,4")
    Marquee.finish(p, state, true)
    check("Escape restores the exact preceding marks", p.selectedIndices().join(","), "1,2,3,4")
    check("Escape restores the cursor", p.cursorIndex, 1)
    check("Escape restores the anchor", p.selectionAnchor, 1)

    state = Marquee.begin(p, true)
    update(p, state, box(1, 60, 50, 120))
    check("Ctrl at press adds intersections to preceding marks", p.selectedIndices().join(","), "1,2,3,4,5")
    update(p, state, box(1, 80, 50, 99))
    check("shrinking a Ctrl band preserves original overlap", p.selectedIndices().join(","), "1,2,3,4")
    update(p, state, null)
    check("leaving every row keeps the original Ctrl marks", p.selectedIndices().join(","), "1,2,3,4")

    p = pane()
    state = Marquee.begin(p, false)
    update(p, state, box(101, 1, 299, 39, 3, 8), 3, 8)
    check("grid intersections respect columns and row boundaries", p.selectedIndices().join(","), "1,2,4,5")
    update(p, state, box(201, 21, 330, 90, 3, 8), 3, 8)
    check("grid trailing empty cells never become marks", p.selectedIndices().join(","), "5")

    p = pane()
    state = Marquee.begin(p, false)
    var filtered = [10, 15, 24, 47]
    update(p, state, box(1, 0, 50, 61), 1, 4, false, function (index) { return Filter.at(filtered, index) })
    check("filtered bands store listing indices, never hidden rows", p.selectedIndices().join(","), "10,15,24,47")
    p = pane()
    state = Marquee.begin(p, false)
    update(p, state, box(1, 0, 50, 61), 1, 4, false, function (index) { return 150 + index })
    check("active Columns converts its held-relative positions", p.selectedIndices().join(","), "150,151,152,153")
    var otherPane = pane()
    otherPane.selection.only(8)
    check("a second pane owns independent marks", otherPane.selectedIndices().join(","), "8")

    var visited = 0
    Marquee.difference({left: 0, right: 0, top: 0, bottom: 100000},
        {left: 0, right: 0, top: 0, bottom: 99999}, 1, 100001, false, false, function () { visited++ })
    check("extending a large band visits only the entering row", visited, 1)

    var boxes = [null], failures = 0
    for (var left = 0; left < 3; left++) {
        for (var right = left; right < 3; right++) {
            for (var top = 0; top < 3; top++) {
                for (var bottom = top; bottom < 3; bottom++) boxes.push({left: left, right: right, top: top, bottom: bottom})
            }
        }
    }
    function includes(rect, index) {
        return rect !== null && index % 3 >= rect.left && index % 3 <= rect.right
            && Math.floor(index / 3) >= rect.top && Math.floor(index / 3) <= rect.bottom
    }
    for (var a = 0; a < boxes.length; a++) {
        for (var b = 0; b < boxes.length; b++) {
            var expected = []
            for (var index = 0; index < 8; index++) {
                if (includes(boxes[a], index) && !includes(boxes[b], index)) expected.push(index)
            }
            for (var direction = 0; direction < 4; direction++) {
                var actual = []
                Marquee.difference(boxes[a], boxes[b], 3, 8, !!(direction & 1), !!(direction & 2),
                    function (index) { actual.push(index) })
                actual.sort(function (a, b) { return a - b })
                if (actual.join(",") !== expected.join(",")) failures++
            }
        }
    }
    check("all 5476 small-grid overlap/direction cases match independent cell membership", failures, 0)

    p = pane()
    p.selection.only(9)
    Tap.tapped(2, 1, Qt.NoModifier, p)
    check("plain click feeds the same mark set as the band", p.selectedIndices().join(","), "2")
    Tap.tapped(5, 1, Qt.ControlModifier, p)
    check("Ctrl click adds one without moving other marks", p.selectedIndices().join(","), "2,5")
    Tap.tapped(5, 2, Qt.ControlModifier, p)
    check("a modified double click only toggles once", p.selectedIndices().join(","), "2,5")
    Tap.tapped(5, 1, Qt.ControlModifier, p)
    check("Ctrl click removes an existing mark", p.selectedIndices().join(","), "2")
    Tap.tapped(7, 1, Qt.ShiftModifier, p)
    check("Shift click uses the existing cursor anchor", p.selectedIndices().join(","), "5,6,7")

    var preset = Keymap.preset
    for (var i = 0; i < Keymap.PRESETS.length; i++) {
        Keymap.setPreset(Keymap.PRESETS[i])
        var cancelled = 0
        var root = {selectionBand: {cancel: function () { cancelled++ }}}
        check("Escape during a band is consumed under " + Keymap.PRESETS[i],
            Focus.handleKey({key: Qt.Key_Escape, text: "", modifiers: Qt.NoModifier}, root, null), true)
        check("Escape cancels before another key context under " + Keymap.PRESETS[i], cancelled, 1)
        Focus.handleKey({key: Qt.Key_Escape, text: "", modifiers: Qt.ControlModifier}, root, null)
        check("held Ctrl cannot prevent band cancellation under " + Keymap.PRESETS[i], cancelled, 2)
        check("a write key cannot act until the band releases under " + Keymap.PRESETS[i],
            Focus.handleKey({key: Qt.Key_D, text: "d", modifiers: Qt.NoModifier}, root, null), true)
    }
    Keymap.setPreset(preset)
}
