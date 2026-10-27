.import "../../ui/js/Anchor.js" as Anchor
.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/FilterWire.js" as FilterWire
.import "../../ui/js/Swap.js" as Swap
.import "filterfixture.js" as Fixture

// Issue 315: a query over a held window smaller than the folder asks the backend to test the folder's
// names, and its two answers land as the shown model and one view window of rows. The stub pane is
// filter.js's own; its backend records each call in order.

function run(check) {
    // ui/Pane.qml calls ask from onFilterQueryChanged, so a stub types by applying the query and asking.
    function type(p, text) {
        for (var i = 0; i < text.length; i++) {
            Filter.typed(p, text.charAt(i))
            p.refresh()
            FilterWire.ask(p)
            p.refresh()
        }
    }
    // The folder's answer as the backend sends it: runs over listing rows, for the newest gen and the held numbering.
    function land(p, runs) {
        var ok = FilterWire.answered(p, p.filterGen, FilterWire.expand(runs).length, runs, p.backend.heldListing)
        p.refresh()
        return ok
    }
    function rowsFrom(start, count) {
        var out = []
        for (var r = start; r < start + count; r++) out.push({ n: "file-" + r + ".txt", r: r })
        return out
    }
    function last(p) { return p.backend.asked[p.backend.asked.length - 1] }

    // Seven rows held from 400, in a folder of 5,000: the backend is asked, and the held rows answer meanwhile.
    var wide = Fixture.pane("", 400)
    wide.total = 5000
    type(wide, "20")
    check("a query over a window smaller than the folder asks the backend once per keystroke, each with its own gen, around the cursor's row, which the held rows could not test, with the buffer ahead",
          wide.backend.asked.join(" "), "filter 2:1:0:50:130 filter 20:2:0:50:130")
    check("and the numbering and the query asked for are the ones standing", wide.filterAskedFor + ":" + wide.filterAsked, "1:20")
    check("until the answer lands, the held rows' own matches are shown", wide.shown.join(","), "405,406")
    check("and the strip counts only the rows tested so far", FilterWire.tested(wide), 7)
    check("the answer for the newest gen and the held numbering is taken", land(wide, [[12, 2], [2340, 1]]), true)
    check("and expanded into the shown model, in listing order", wide.shown.join(","), "12,13,2340")
    check("which now counts as the whole folder tested", FilterWire.tested(wide), 5000)
    check("a cursor the folder's answer hides takes the first row shown and the view goes there", wide.cursorIndex + ":" + wide.scrolled, "12:0")
    wide.cursorIndex = 2340
    land(wide, [[12, 2], [2340, 1]])
    check("a cursor the answer keeps stays, and the view follows its view position", wide.cursorIndex + ":" + wide.scrolled, "2340:2")
    check("an answer for a stale gen is dropped", FilterWire.answered(wide, 1, 1, [[7, 1]], 1), false)
    check("as is one for another numbering", FilterWire.answered(wide, 2, 1, [[7, 1]], 2), false)
    check("and neither moved the shown model", wide.shown.join(","), "12,13,2340")
    // A query that moved on with no ask, which a listing out allows, must not take the ask's answer as its own.
    wide.listInFlight = true
    Filter.typed(wide, "2"); wide.refresh(); FilterWire.ask(wide); wide.refresh()
    check("a query typed while a listing is out keeps the gen asked", wide.filterGen + ":" + wide.filterAsked + ":" + wide.filterQuery, "2:20:202")
    check("so the answer for that gen is not labelled with the query it was not for", FilterWire.answered(wide, 2, 3, [[12, 2], [2340, 1]], 1), false)
    check("and the held rows answer meanwhile", wide.shown.join(","), "405,406")
    wide.listInFlight = false
    Filter.backspace(wide); wide.refresh(); FilterWire.ask(wide); wide.refresh()
    land(wide, [[12, 2], [2340, 1]])

    // One view window of the matched rows, each naming its listing row back.
    var window = [{ n: "file-0012.txt", r: 12 }, { n: "file-0013.txt", r: 13 }, { n: "file-2340.txt", r: 2340 }]
    check("a window for the standing gen lands as the filter's rows", FilterWire.windowed(wide, 0, window, ["Plain text"], 1, wide.filterGen), true)
    check("rowFor answers a matched row the held window does not hold from it", wide.rowFor(2340).n, "file-2340.txt")
    check("and a held row from the held window still", wide.rowFor(405).n, "screenrecording-2026-08-21.mp4")
    check("the window's Kind dictionary is its own", wide.filterKinds.join(","), "Plain text")
    check("a window for a stale gen is dropped", FilterWire.windowed(wide, 0, window, [], 1, 1), false)
    FilterWire.windowed(wide, 1, [{ n: "other", r: 99 }], [], 1, wide.filterGen)
    check("a row that does not name the listing row back is nothing", wide.rowFor(13), null)
    check("as is a view position past the window", wide.rowFor(2340), null)

    // Esc closes the standing query on the backend and drops the answer and its rows.
    Filter.escape(wide); wide.refresh(); FilterWire.ask(wide); wide.refresh()
    check("an empty query closes the standing query", last(wide), "close")
    check("and drops the answer and the rows", (wide.filterAnswer === null) + ":" + wide.filterRows.length + ":" + wide.shown, "true:0:null")

    // A folder the pane holds whole is answered on the keystroke and asks nothing.
    var whole = Fixture.pane()
    type(whole, "scr")
    check("a window that is the whole folder asks the backend nothing", whole.backend.asked.length, 0)
    check("and shows the held rows' own answer", whole.shown.join(","), "0,1,5,6")

    // Drift under the filter: the view-space window on the matches, asked for the way the listing's own is.
    var drifted = Fixture.pane("", 400)
    drifted.total = 5000
    type(drifted, "e")
    check("before the answer lands, a drift check asks nothing", FilterWire.drift(drifted, 0, 34) === null && drifted.backend.asked.length === 1, true)
    land(drifted, [[0, 3000]])
    check("with no filter window held, a drift check asks for one around the view", JSON.stringify(FilterWire.drift(drifted, 0, 34)) + " " + last(drifted), '{"start":0,"count":130} fwindow 0:130')
    FilterWire.windowed(drifted, 0, rowsFrom(0, 130), [], 1, drifted.filterGen)
    check("a view inside the window with margin to spare asks nothing", FilterWire.drift(drifted, 0, 34), null)
    check("a view near the window's end asks ahead of it", last(drifted) === "fwindow 0:130" && FilterWire.drift(drifted, 100, 134) !== null && last(drifted) === "fwindow 50:130", true)
    FilterWire.windowed(drifted, 500, rowsFrom(500, 130), [], 1, drifted.filterGen)
    check("and a view near its start asks behind it", FilterWire.drift(drifted, 510, 544) !== null && last(drifted) === "fwindow 460:130", true)

    // A listing still out asks nothing; the rows landing under another numbering ask once.
    var out = Fixture.pane("", 400)
    out.total = 5000
    out.listInFlight = true
    type(out, "s")
    check("a query typed while a listing is out asks nothing yet", out.backend.asked.length, 0)
    out.listInFlight = false
    check("the rows landing ask the folder", FilterWire.relisted(out, 1) && last(out) === "filter s:1:0:50:130", true)
    check("a window of the same numbering changes nothing", FilterWire.relisted(out, 1), false)
    land(out, [[1, 1]])
    FilterWire.windowed(out, 0, [{ n: "scripts", r: 1 }], [], 1, out.filterGen)
    out.backend.heldListing = 2
    // ui/PaneSwap.qml takeRows drops the old numbering's answer before its rows are read, and rowsLanded asks once they are in.
    check("a window of the same numbering landing drops nothing", (FilterWire.renumbered(out, 1), out.filterAnswer !== null && out.filterRows.length === 1), true)
    FilterWire.renumbered(out, 2)
    check("another numbering drops the answer and its window at once", (out.filterAnswer === null) + ":" + out.filterRows.length + ":" + out.shown.join(","), "true:0:1")
    check("and the rows landing ask again", FilterWire.relisted(out, 2) && last(out).indexOf("filter s:2:") === 0, true)
    check("and a pane with no query standing is left alone", FilterWire.relisted(Fixture.pane(), 3), false)

    // A filter never moves the held window while the cursor walks the folder's matches, so a re-read with the cursor
    // outside it asks the window around the cursor's row back, and its anchor waits for that one (ui/js/Anchor.js).
    var reread = Fixture.pane("s", 400)
    reread.total = 5000
    reread.path = "/d"
    reread.openWithoutHistory = function () { reread.backend.asked.push("list") }
    reread.filterAnswer = { gen: 1, listing: 1, query: "s", list: [401, 2400] }
    reread.refresh()
    reread.filterHeld = 1
    reread.filterRows = [{ n: "file-2400.txt", r: 2400 }]
    reread.cursorIndex = 2400
    var rereadAnchor = Anchor.watched(reread, false, 10)
    check("a re-read with the cursor on a row only the filter window holds asks its window back around that row",
          rereadAnchor.name + ":" + rereadAnchor.start + " " + reread.backend.asked.join(","), "file-2400.txt:2350 list,2350:130")
    reread.backend.asked = []
    reread.cursorIndex = 401
    check("and one with the cursor in the held window asks that window back", Anchor.watched(reread, false, 10).start + " " + reread.backend.asked.join(","), "400 list,400:130")

    // A re-read's anchor puts the cursor back on its file first; the folder is asked once it has, around that file.
    var anchored = Fixture.pane("s", 400)
    anchored.total = 5000
    anchored.path = "/d"
    anchored.wire = { anchor: { path: "/d" } }
    anchored.cursorIndex = 2400
    check("an anchor still out for this folder holds the ask", FilterWire.relisted(anchored, 1) + ":" + anchored.backend.asked.length, "false:0")
    anchored.wire.anchor = null
    check("and once it ends the folder is asked around the cursor it put back", FilterWire.relisted(anchored, 1) && last(anchored), "filter s:1:2400:50:130")

    // The answer's count change moves a scrolled view to its top and the view pulls the cursor onto a row there, inside
    // the write (ui/StableModel.qml resize, Filter.clampCursor): a stub whose answer write refreshes and pulls the same way.
    function pulling(p) {
        var standing = p.filterAnswer
        Object.defineProperty(p, "filterAnswer", {
            get: function () { return standing },
            set: function (a) { standing = a; p.refresh(); if (a !== null && p.shown.length > 0) { p.cursorIndex = p.shown[0]; p.cursorSeq += 1 } },
            configurable: true
        })
        return p
    }
    var pulled = pulling(Fixture.pane("", 400))
    pulled.total = 5000
    type(pulled, "e")
    pulled.cursorIndex = 2340
    land(pulled, [[12, 2], [2340, 1]])
    check("an answer that keeps the cursor puts it back after the view's pull, and the view follows it", pulled.cursorIndex + ":" + pulled.scrolled, "2340:2")

    // Where the cursor's row stood on screen when relisted asked is where its answer puts it back, measured as ui/js/AnchorHold.js does.
    var placed = pulling(Fixture.pane("s", 400))
    placed.total = 5000
    placed.viewMode = "list"
    placed.anchorRowHeight = 10
    placed.listArea = { contentY: 0, originY: 0, height: 100, contentHeight: 10000, coalesced: 0, restartCoalesce: function () { placed.listArea.coalesced += 1 } }
    placed.cursorIndex = 405
    check("relisted keeps the place of a cursor drawn in the viewport, for the gen it asks", FilterWire.relisted(placed, 1) && JSON.stringify(placed.filterPlace), '{"offset":40,"rowH":10,"gen":1}')
    land(placed, [[400, 7], [2340, 1]])
    check("its answer puts the row back there and does not scroll to it", placed.listArea.contentY + ":" + placed.scrolled + ":" + placed.listArea.coalesced, "10:-1:1")
    check("and the place is spent", placed.filterPlace, null)
    placed.cursorIndex = 4000
    placed.filterAnswer = null
    check("a cursor no row on screen draws keeps no place", FilterWire.relisted(placed, 2) && placed.filterPlace, null)

    // The selection: what the held rows hide goes on the keystroke, what they cannot test waits for the folder.
    var marked = Fixture.pane("", 400)
    marked.total = 5000
    marked.selection.toggle(1000)
    marked.selection.toggle(403)
    type(marked, "scr")
    check("a selected held row the query hides leaves the selection, a selected row outside the window stays", Fixture.picks(marked), "1000")
    land(marked, [[0, 1], [5, 2]])
    check("and goes when the folder's answer hides it", Fixture.picks(marked), "")

    check("expand turns runs back into the rows they stand for", FilterWire.expand([[0, 3], [5, 1]]).join(","), "0,1,2,5")
    var rows = Fixture.listing()
    check("standing answers the held rows' own subsequence with no answer in", FilterWire.standing(null, rows, 0, "scr").join(","), "0,1,5,6")
    check("the folder's answer while it is for the standing query", FilterWire.standing({ query: "scr", list: [9] }, rows, 0, "scr").join(","), "9")
    check("and the held rows' own again once the query moved on", FilterWire.standing({ query: "sc", list: [9] }, rows, 0, "scr").join(","), "0,1,5,6")

    // A filter window's stat on a stuck mount fails seconds later, when the user may be in another folder with its listing out.
    var filtering = { listInFlight: true, listedSeen: true, swap: { drop: function () { filtering.dropped = true } } }
    check("a filter window's failure ends only its own request, never the listing out for the folder since gone to",
          Swap.failListing(filtering, "filterwindow", "/mnt/stuck") + "|" + (filtering.dropped === true) + "|" + filtering.listInFlight, "false|false|true")
}
