.import "../../ui/js/Selection.js" as Selection
.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Marks.js" as Marks
.import "filterfixture.js" as Fixture

// Additive Shift ranges: a gesture keeps the marks made before it, so several blocks can be marked.
// A run of Shift+J/K or Shift+clicks with nothing moving the cursor in between is one gesture whose
// base is snapshotted once; any other cursor move or mark ends it. Uses the real Selection.

function pane(n) {
    var rows = []
    for (var i = 0; i < n; i++) rows.push({ n: "f" + i + ".txt", d: false })
    var p = {
        rows: rows, held: 0, total: n, cursorIndex: 0, cursorSeq: 0, shown: null, shownTotal: n,
        filterQuery: "", selectionAnchor: 0, selectionVersion: 0
    }
    p.selection = Selection.create()
    p.showRow = function () {}
    p.selectedIndices = function () { return p.selection.indices() }
    return p
}

function picks(p) { return p.selectedIndices().join(",") }
function down(p) { Filter.moveCursor(p, 1); Marks.follow(p) }
function up(p) { Filter.moveCursor(p, -1); Marks.follow(p) }
function click(p, i) {
    Filter.setCursor(p, i)
    p.selection.only(p.cursorIndex)
    p.selectionAnchor = p.cursorIndex
    p.selectionVersion++
}

function run(check) {
    // Two keyboard blocks: the second Shift run keeps the first.
    var p = pane(10)
    Marks.extend(p, 1)
    Marks.extend(p, 1)
    check("two Shift+J mark 0,1,2", picks(p), "0,1,2")
    down(p)
    down(p)
    down(p)
    Marks.extend(p, 1)
    check("a Shift run after plain moves keeps the first block", picks(p), "0,1,2,5,6")
    check("and the anchor latched where the new gesture started", p.selectionAnchor, 5)
    // The gesture's own range shrinks without dropping the base.
    Marks.extend(p, -1)
    check("Shift+K shrinks to the anchor and keeps the base", picks(p), "0,1,2,5")
    Marks.extend(p, -1)
    check("and keeps shrinking past it the same way", picks(p), "0,1,2,4,5")

    // A plain move away and back ends the gesture even when the cursor returns.
    var m = pane(10)
    Marks.extend(m, 1)
    Marks.extend(m, 1)
    check("two Shift+J mark 0,1,2", picks(m), "0,1,2")
    down(m)
    up(m)
    down(m)
    up(m)
    Marks.extend(m, -1)
    check("a move away and back starts the next Shift over", picks(m), "0,1,2")

    // Two Shift+click blocks: consecutive clicks share one base, a click between starts another.
    var q = pane(10)
    q.cursorIndex = 1
    Marks.extendToRow(q, 3)
    check("Shift+click marks the anchor block", picks(q), "1,2,3")
    Marks.extendToRow(q, 5)
    check("a second Shift+click grows the same gesture", picks(q), "1,2,3,4,5")
    Marks.toggleRow(q, 6)
    Marks.extendToRow(q, 8)
    check("a ctrl+click between starts a second block beside the first", picks(q), "1,2,3,4,5,6,7,8")
    q.selection.clear()
    Marks.extend(q, 1)
    check("clearing the selection ends every block and the next Shift starts over", picks(q), "8,9")

    // v mid-gesture ends it: the next Shift snapshots the marks v left, anchor and all.
    var r = pane(10)
    click(r, 1)
    Marks.extend(r, 1)
    Marks.extend(r, 1)
    Marks.toggleSelect(r)
    check("v toggles the cursor row out of the block", picks(r), "1,2")
    Marks.extend(r, 1)
    check("the next Shift keeps those marks beside its new range", picks(r), "1,2,3,4")

    // Ctrl+click ends it the same way.
    var s = pane(10)
    click(s, 1)
    Marks.extend(s, 1)
    Marks.extend(s, 1)
    Marks.toggleRow(s, 5)
    check("ctrl+click joins its row and moves the anchor", picks(s), "1,2,3,5")
    Marks.extend(s, 1)
    check("the next Shift keeps that set beside its new range", picks(s), "1,2,3,5,6")

    // Ctrl+A then Shift keeps everything: the new gesture snapshots all rows as its base.
    var t = pane(10)
    t.cursorIndex = 4
    Marks.selectAll(t)
    Marks.extend(t, 1)
    check("Shift after Ctrl+A still marks every row", t.selection.count(), 10)

    // A new gesture keeps its base whether the cursor begins inside or outside those marks.
    var inside = Fixture.pane()
    inside.selection.toggle(2)
    inside.selection.toggle(3)
    inside.selection.toggle(4)
    inside.cursorIndex = 3
    Marks.extend(inside, 1)
    Marks.extend(inside, -1)
    check("a horizontal-style gesture inside an existing selection keeps its base", Fixture.picks(inside), "2,3,4")
    var outside = Fixture.pane()
    outside.selection.toggle(0)
    outside.cursorIndex = 3
    Marks.extend(outside, 1)
    check("a gesture outside existing marks adds its range without replacing them", Fixture.picks(outside), "0,3,4")
    Marks.extend(outside, -1)
    check("reversing that gesture restores only its original base plus anchor", Fixture.picks(outside), "0,3")

    // Filtered: the range covers drawn rows only, and narrowing mid-gesture never resurrects one.
    var f = Fixture.pane("scr")
    Filter.setCursor(f, 1)
    Marks.extendToRow(f, 6)
    check("Shift+click over a filter skips the rows it hid", Fixture.picks(f), "1,5,6")
    Filter.apply(f, "scree")
    f.refresh()
    check("narrowing prunes the hidden base row", Fixture.picks(f), "5,6")
    Marks.extend(f, -1)
    check("the next Shift keeps the pruned marks and hides nothing back", Fixture.picks(f), "5,6")
}
