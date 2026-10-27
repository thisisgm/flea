.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Thumbs.js" as Thumbs
.import "../../ui/js/DirSizes.js" as DirSizes
.import "filterfixture.js" as Fixture
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/Ops.js" as Ops

// The fixtures and the stub pane live in filterfixture.js, so this file stays checks.

function run(check) {
    check("no query means no filter at all, which every view reads as the identity",
          Filter.shown(Fixture.listing(), 0, ""), null)
    check("a query keeps the rows whose name contains it", Filter.shown(Fixture.listing(), 0, "scr").join(","), "0,1,5,6")
    // ui/js/Match.js paints the run and is case-insensitive, so the test deciding which rows survive
    // has to be that same one, or a row matches and shows no run to say why.
    check("matching is case-insensitive, so capital Screens survives a lowercase query",
          Filter.shown(Fixture.listing(), 0, "screen").join(","), "0,5,6")
    check("and it matches anywhere in the name, not only at the start",
          Filter.shown(Fixture.listing(), 0, "2026").join(","), "5,6")
    check("a query nothing matches keeps an empty list", Filter.shown(Fixture.listing(), 0, "zzz").length, 0)
    check("and an empty match list is still a filter, so it is not null",
          Filter.shown(Fixture.listing(), 0, "zzz") === null, false)
    // The pane holds a window, not the listing, so a match is named by its listing row.
    check("the answers are listing rows, so a held window offsets them",
          Filter.shown(Fixture.listing(), 400, "2026").join(","), "405,406")

    // The filter keeps a subsequence of what the backend sent, and the backend groups directories
    // first; this is the check that fails if anyone ever ranks or reorders the matches.
    var mixed = Filter.shown(Fixture.listing(), 0, "scr")
    check("a filtered view keeps the backend's order, so directories still come first",
          mixed.map(function (i) { return Fixture.listing()[i].d }).join(","), "true,true,false,false")
    check("and the match list is strictly ascending, which is what keeps that true",
          mixed.slice().sort(function (a, b) { return a - b }).join(","), mixed.join(","))
    check("and it really dropped rows, so the check above has a denominator", mixed.length < Fixture.listing().length, true)

    check("a view position resolves to the listing row it draws", Filter.at(mixed, 2), 5)
    check("and back again", Filter.viewOf(mixed, 5), 2)
    check("a row the filter hides has no view position", Filter.viewOf(mixed, 3), -1)
    // A shrinking match list can leave a delegate on a position past the end for a frame.
    check("a view position past the end is a listing row nothing draws", Filter.at(mixed, 9), -1)
    check("with no filter both directions are the identity",
          Filter.at(null, 12) + "|" + Filter.viewOf(null, 12), "12|12")

    // SearchFilter rules 1 and 2: one sentence, on the strip that created it, carrying its own scope.
    check("the strip says what it kept, out of the rows it could test",
          Filter.summary(mixed, 7, 7), "4 of 7 shown")
    check("and names the directory those rows are a window on when it is not all of it",
          Filter.summary(mixed, 7, 104812), "4 of 7 shown · of 104,812 in this folder")
    check("and a four-figure window groups the head it kept",
          Filter.summary(mixed, 1204, 1204), "4 of 1,204 shown")
    check("and groups the loaded half of a scoped one too",
          Filter.summary(mixed, 1204, 104812), "4 of 1,204 shown · of 104,812 in this folder")
    check("nothing matching is a count like any other, and keeps the same scope",
          Filter.summary([], 350, 104812), "0 of 350 shown · of 104,812 in this folder")
    check("no filter draws no sentence at all", Filter.summary(null, 7, 7), "")

    check("the rows shown between two ends skip the ones the filter hid",
          Filter.between(mixed, 1, 6).join(","), "1,5,6")
    check("the ends are inclusive and their order does not matter", Filter.between(mixed, 6, 1).join(","), "1,5,6")
    check("a request is cut back to the rows the filter still shows",
          Filter.keep([1, 2, 3, 5], mixed).join(","), "1,5")
    check("with no filter a request passes through untouched", Filter.keep([1, 2, 3], null).join(","), "1,2,3")
    check("a thumbnail plan is cut on its asks and keeps its drops",
          JSON.stringify(Filter.cut({ ask: [1, 3, 5], drop: [9] }, mixed, Thumbs.empty())),
          JSON.stringify({ ask: [1, 5], drop: [9] }))
    var thumbnailState = Thumbs.applied(Thumbs.empty(), {ask: [400, 401, 403, 405, 406], drop: []})
    thumbnailState = Thumbs.remember(thumbnailState, 403, "/cache/403.png", 240)
    var sparse = [401, 405]
    var sparseRows = [{t: true}, {t: true}, {d: true}, {t: true}, {d: true}, {t: true}, {t: true}]
    // Issue 315: the planners walk the rows drawn, each looked up in the held window or the filter's, never the run between.
    var sparsePane = Fixture.pane("", 400)
    sparsePane.rows = sparseRows
    sparsePane.total = 5000
    sparsePane.filterQuery = "x"
    sparsePane.filterAnswer = { query: "x", list: [12, 401, 405, 2340] }
    sparsePane.refresh()
    sparsePane.filterHeld = 0
    sparsePane.filterRows = [{ n: "a", t: true, r: 12 }, { n: "b", r: 401 }, { n: "c", r: 405 }]
    sparsePane.thumbState = thumbnailState
    sparsePane.dirSizeState = DirSizes.empty()
    check("the rows drawn between two view positions carry their listing row and the row held for them, from either window, or null for one not landed",
          Filter.drawn(sparsePane, 0, 3).map(function (d) { return d.index + ":" + (d.row === null ? "-" : d.row.n || d.row.t) }).join(" "), "12:a 401:true 405:true 2340:-")
    check("and the ends clamp to the matches", Filter.drawn(sparsePane, 3, 9).map(function (d) { return d.index }).join(","), "2340")
    var narrowedPlan = Filter.planThumbs(sparsePane, 1, 2, "media")
    check("a narrowed viewport cancels every hidden pending thumbnail, including gaps between matches and rows scrolled off",
          narrowedPlan.drop.join(","), "400,406")
    check("and asks for no row already asked", narrowedPlan.ask.length, 0)
    check("a matched row only the filter window holds is asked for, so it gets its thumbnail", Filter.planThumbs(sparsePane, 0, 0, "media").ask.join(","), "12")
    check("a row not landed is asked for nothing", Filter.planDirSizes(sparsePane, 0, 3).join(","), "")
    sparsePane.filterRows = sparsePane.filterRows.concat([{ n: "d", d: true, r: 2340 }])
    check("a folder only the filter window holds is asked for its size once it lands, the files drawn beside it are not",
          Filter.planDirSizes(sparsePane, 0, 3).join(","), "2340")
    sparsePane.filterQuery = ""
    sparsePane.refresh()
    check("with no filter up the plan is the held window's run", Filter.planThumbs(sparsePane, 400, 406, "media").ask.join(","), "")
    check("and so is the folder sizes'", Filter.planDirSizes(sparsePane, 400, 406).join(","), "402,404")
    var hiddenInside = Thumbs.applied(Thumbs.empty(), {ask: [401, 403, 405], drop: []})
    check("a pending thumbnail between visible matches is cancelled before it decodes offscreen",
          Filter.cut({ask: [], drop: []}, sparse, hiddenInside).drop.join(","), "403")
    check("an already scheduled cancellation is not duplicated by the filter",
          Filter.cut({ask: [], drop: [403]}, sparse, hiddenInside).drop.join(","), "403")
    check("completed thumbnails stay cached when the filter hides them",
          narrowedPlan.drop.indexOf(403), -1)
    check("zero matches cancels all pending thumbnails without inventing requests",
          JSON.stringify(Filter.cut({ask: [], drop: []}, [], hiddenInside)),
          JSON.stringify({ask: [], drop: [401, 403, 405]}))
    check("clearing the filter leaves the ordinary viewport cancellation plan intact",
          JSON.stringify(Filter.cut({ask: [400], drop: [406]}, null, hiddenInside)),
          JSON.stringify({ask: [400], drop: [406]}))
    check("sparse filtered tile requests retain backend row identities",
          Filter.cut(Thumbs.plan(Thumbs.empty(), sparseRows, 400, 401, 405), sparse, Thumbs.empty()).ask.join(","),
          "401,405")
    check("directory-size requests exclude directories hidden between filtered tiles",
          Filter.keep(DirSizes.plan(DirSizes.empty(), sparseRows, 400, 401, 405), sparse).length, 0)
    sparsePane.filterQuery = "x"
    sparsePane.filterAnswer = { query: "x", list: [] }
    sparsePane.refresh()
    check("a filter matching nothing draws nothing, so no row is ever asked for", Filter.planThumbs(sparsePane, 0, 5, "media").ask.length + Filter.planDirSizes(sparsePane, 0, 5).length, 0)

    var keyLine = Fixture.pane()
    Filter.typeKey({ key: Qt.Key_Delete, text: "\u007f" }, keyLine)
    check("DEL types nothing into the filter", keyLine.filterQuery, "")
    Filter.typeKey({ key: Qt.Key_E, text: "e" }, keyLine)
    check("a letter still types into the filter", keyLine.filterQuery, "e")

    var p = Fixture.pane()
    Filter.start(p)
    check("slash gives the query line the keyboard", p.filterTyping, true)
    check("and narrows nothing until something is typed", p.filterQuery, "")

    Fixture.typeInto(p, "screens")
    check("typing narrows in place, with no round trip to wait for", p.filterQuery, "screens")
    check("and the rows it leaves are the matches", p.shown.join(","), "0,6")

    Filter.backspace(p); p.refresh()
    check("backspace shortens the query", p.filterQuery, "screen")
    check("and the rows it had hidden come back with it", p.shown.join(","), "0,5,6")

    Filter.commit(p)
    check("enter hands the keyboard back to the list", p.filterTyping, false)
    check("and the filter stands, so every action key works over the narrowed rows", p.filterQuery, "screen")

    Filter.close(p); p.refresh()
    check("esc clears the query", p.filterQuery, "")
    check("and puts the whole listing back", p.shown, null)

    var empty = Fixture.pane()
    Filter.start(empty)
    Filter.commit(empty)
    check("enter on an empty query closes the strip rather than standing an empty filter", empty.filterTyping, false)

    var walk = Fixture.pane("2026")
    walk.cursorIndex = 5
    Filter.moveCursor(walk, 1)
    check("j steps to the next row the filter shows, not the next listing row", walk.cursorIndex, 6)
    check("and the view scrolls to that row's view position, not its listing row", walk.scrolled, 1)
    Filter.moveCursor(walk, 1)
    check("the last row shown is where down stops", walk.cursorIndex, 6)
    Filter.setCursorView(walk, 0)
    check("g goes to the first row shown, which is not the listing's first row", walk.cursorIndex, 5)
    Filter.moveCursor(walk, -1)
    check("and up stops there too", walk.cursorIndex, 5)

    // The two index spaces have to really diverge, or the clamp into view space hides a step taken
    // in listing space: "o" keeps rows 2, 3, 5 and 6, so a step from either end lands differently.
    var apart = Fixture.pane("o")
    apart.cursorIndex = 2
    Filter.moveCursor(apart, 1)
    check("a step down lands on the next row drawn, not on cursor plus one", apart.cursorIndex, 3)
    check("and scrolls to that row's view position", apart.scrolled, 1)
    apart.cursorIndex = 6
    Filter.moveCursor(apart, -1)
    check("a step up lands on the previous row drawn, not on cursor minus one", apart.cursorIndex, 5)
    check("and scrolls to that row's view position too", apart.scrolled, 2)
    Filter.setCursorView(apart, 3)
    check("the last view position is the last row drawn", apart.cursorIndex, 6)

    var plain = Fixture.pane()
    plain.cursorIndex = 3
    Filter.moveCursor(plain, 1)
    check("with no filter the cursor still steps one listing row", plain.cursorIndex, 4)

    var none = Fixture.pane("zzz")
    Filter.moveCursor(none, 1)
    check("a filter matching nothing has no row to move to", none.cursorIndex, 0)

    // The wheel moves the viewport and the cursor follows it, in view positions on both ends.
    var wheel = Fixture.pane("2026")
    wheel.cursorIndex = 5
    Filter.clampCursor(wheel, 1, 1)
    check("a scrolled viewport pulls the cursor onto a row it can see", wheel.cursorIndex, 6)
    Filter.clampCursor(wheel, 0, 1)
    check("and leaves it alone once it is inside", wheel.cursorIndex, 6)

    // A selection you cannot see is one you can act on by accident, and trash, cut and copy take the
    // selection with no confirmation, so what the filter hides it drops from the selection too.
    var sel = Fixture.pane()
    sel.selection.toggle(3)
    sel.selection.toggle(5)
    sel.selection.toggle(6)
    Fixture.typeInto(sel, "scr")
    check("a row the filter hides drops out of the selection with it", Fixture.picks(sel), "5,6")
    check("and the version moves, so the status bar's count re-reads", sel.selectionVersion > 0, true)
    Filter.close(sel); sel.refresh()
    check("clearing the filter keeps what survived it and restores nothing", Fixture.picks(sel), "5,6")

    // A sort is a backend reorder: the same rows arrive in another order and the query is untouched.
    var sortFirst = Filter.shown(Fixture.reversed(), 0, "scr")
    check("sorting first and filtering after keeps the sorted order", sortFirst.join(","), "1,2,3,4")
    var filterFirst = Fixture.pane("scr")
    filterFirst.rows = Fixture.reversed()
    filterFirst.refresh()
    check("filtering first and sorting after keeps the filter, over the new order",
          filterFirst.shown.join(","), "1,2,3,4")
    check("and the reorder leaves the query alone", filterFirst.filterQuery, "scr")
    // View position 2 held screenrecording before the reverse and screenshot after, so this fails if
    // the match list were ever cached across a sort instead of recomputed from the new rows.
    check("the matches name the reordered rows, not the ones that used to be there",
          mixed.map(Fixture.nameIn(Fixture.listing())).join(" ") + " then "
          + sortFirst.map(Fixture.nameIn(Fixture.reversed())).join(" "),
          "Screens scripts screenrecording-2026-08-21.mp4 screenshot-2026-08-30.png then "
          + "scripts Screens screenshot-2026-08-30.png screenrecording-2026-08-21.mp4")
    check("and directories still lead the filtered view after a reverse",
          sortFirst.map(function (i) { return Fixture.reversed()[i].d }).join(","), "true,true,false,false")
    // States' no-match tile: the query named back, and the count of rows the filter tested.
    check("a filter that matches none says how many rows it tested", Filter.noMatch(11), "No match among 11 items")
    check("and a single item keeps the sentence readable", Filter.noMatch(1), "No match among 1 item")
    // Issue 315: viewOf is a binary search over the ascending match list, so a 100,000-row answer costs each delegate nothing.
    check("viewOf finds a shown row's view position at either end and in the middle",
          [Filter.viewOf([3, 8, 9, 20], 3), Filter.viewOf([3, 8, 9, 20], 9), Filter.viewOf([3, 8, 9, 20], 20)].join(","), "0,2,3")
    check("and answers -1 for a hidden row between, before or past the matches, and in an empty list",
          [Filter.viewOf([3, 8, 9, 20], 5), Filter.viewOf([3, 8, 9, 20], 1), Filter.viewOf([3, 8, 9, 20], 21), Filter.viewOf([], 0)].join(","), "-1,-1,-1,-1")

    var pointed = Fixture.pane()
    Filter.setCursor(pointed, 4, 0)
    check("a pointer setCursor hands context 0 to the view", pointed.contexts.join(","), "0")

    // Issue 287: Ctrl+V and Shift+Insert paste the clipboard text into the query as if typed.
    function pasteEvent(key, mods) {
        return { key: key, text: "", modifiers: mods }
    }
    function clipped(clip) {
        var p = Fixture.pane()
        p.clipboardText = function () { return clip }
        return p
    }
    var pasted = clipped("repo\nsecond line")
    Filter.start(pasted)
    Filter.typeKey(pasteEvent(Qt.Key_V, Qt.ControlModifier), pasted)
    pasted.refresh()
    check("ctrl+v pastes the clipboard's first line into the filter", pasted.filterQuery, "repo")
    check("and the paste narrows, so it stands an empty match list instead of no filter",
          pasted.shown === null, false)
    check("and that narrowed list holds no row", (pasted.shown || []).length, 0)
    var shifted = clipped("repo\nsecond line")
    Filter.start(shifted)
    Filter.typeKey(pasteEvent(Qt.Key_Insert, Qt.ShiftModifier), shifted)
    check("shift+insert pastes the same way", shifted.filterQuery, "repo")
    var dirty = clipped("a\tb\u0001c")
    Filter.start(dirty)
    Filter.typeKey(pasteEvent(Qt.Key_V, Qt.ControlModifier), dirty)
    check("a tab or control character in the clipboard types nothing", dirty.filterQuery, "abc")
    var blank = clipped("")
    Filter.start(blank)
    Filter.typeKey(pasteEvent(Qt.Key_V, Qt.ControlModifier), blank)
    check("an empty clipboard leaves the query as it was", blank.filterQuery, "")
    var missing = clipped(undefined)
    Filter.start(missing)
    Filter.typeKey(pasteEvent(Qt.Key_V, Qt.ControlModifier), missing)
    check("and a clipboard holding no text does too", missing.filterQuery, "")
    var paired = clipped("a\ud83d\ude00b")
    Filter.start(paired)
    Filter.typeKey(pasteEvent(Qt.Key_V, Qt.ControlModifier), paired)
    check("a whole surrogate pair in the clipboard is kept", paired.filterQuery, "a\ud83d\ude00b")
    var halved = clipped("a\ud83db\ude00c")
    Filter.start(halved)
    Filter.typeKey(pasteEvent(Qt.Key_V, Qt.ControlModifier), halved)
    check("and a lone half of one is dropped", halved.filterQuery, "abc")
}
