.import "../../ui/js/Filter.js" as Filter
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/Ops.js" as Ops
.import "filterfixture.js" as Fixture

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

    check("the canvas's own accounting line names the rows that dropped out",
          Filter.note(mixed, 7, "scr"), "3 rows hidden by the filter")
    check("one hidden row is a row, not rows", Filter.note([0, 1, 2, 3, 4, 5], 7, "s"), "1 row hidden by the filter")
    check("a filter hiding nothing says nothing, the OEM self-hide rule", Filter.note([0, 1, 2], 3, "e"), "")
    check("no filter draws no line at all", Filter.note(null, 7, ""), "")
    check("and nothing matching says so, in the search board's own wording",
          Filter.note([], 7, "benchz"), "Nothing matches benchz")

    // The pane holds a window around the viewport, not the directory, so on a bigger listing the
    // filter has only seen the rows it holds and the strip has to say so.
    check("a window holding the whole listing needs no caveat", Filter.scope(7, 7), "")
    check("a partial window names the rows the filter actually saw",
          Filter.scope(327, 100000), "in the 327 rows loaded")

    check("the rows shown between two ends skip the ones the filter hid",
          Filter.between(mixed, 1, 6).join(","), "1,5,6")
    check("the ends are inclusive and their order does not matter", Filter.between(mixed, 6, 1).join(","), "1,5,6")
    check("a request is cut back to the rows the filter still shows",
          Filter.keep([1, 2, 3, 5], mixed).join(","), "1,5")
    check("with no filter a request passes through untouched", Filter.keep([1, 2, 3], null).join(","), "1,2,3")
    check("a thumbnail plan is cut on its asks and keeps its drops",
          JSON.stringify(Filter.cut({ ask: [1, 3, 5], drop: [9] }, mixed)),
          JSON.stringify({ ask: [1, 5], drop: [9] }))
    // A filtered viewport covers a set, not a run, so the range-shaped planners in ui/js/Thumbs.js
    // and ui/js/DirSizes.js get the run it spans and cut() takes back what they over-asked for.
    check("a view range spans the listing rows at its ends",
          JSON.stringify(Filter.span(mixed, 1, 3)), JSON.stringify({ first: 1, last: 6 }))
    check("with no filter the range is itself",
          JSON.stringify(Filter.span(null, 3, 9)), JSON.stringify({ first: 3, last: 9 }))
    check("a filter matching nothing spans nothing, so no row is ever asked for",
          JSON.stringify(Filter.span([], 0, 5)), JSON.stringify({ first: 0, last: -1 }))

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

    // That cursor stays on a row the filter hides, and the operations that fall back to the cursor
    // row acted on it: Enter opened it, dd trashed it, y and x took it, and r set the rename guard
    // with no delegate to draw the editor or clear it. The rule at the top of prune() is that a row
    // nobody can see is not one to act on, and these hold the cursor row to it.
    var blind = Fixture.pane("zzz")
    blind.cursorIndex = 3
    check("the cursor under a filter that hides it is not a row to act on",
          Ops.targetIndices(blind).join(","), "")
    var seen = Fixture.pane("scr")
    seen.cursorIndex = 5
    check("and a cursor row the filter shows still is", Ops.targetIndices(seen).join(","), "5")
    check("with no filter the cursor row is the target it always was",
          Ops.targetIndices(Fixture.pane("")).join(","), "0")
    blind.viewMode = "list"
    blind.renamingIndex = -1
    blind.rowFor = function (i) { return blind.rows[i] }
    Ops.startRename(blind)
    check("r over a hidden cursor row opens no editor", blind.renamingIndex, -1)
    blind.listInFlight = false
    blind.said = []
    blind.message = function (text) { blind.said.push(text) }
    blind.opened = 0
    blind.open = function () { blind.opened += 1 }
    blind.join = function (dir, name) { return dir + "/" + name }
    blind.path = "/tmp"
    // Screens, a directory the filter hides: opened, it would navigate away from the filtered listing.
    blind.cursorIndex = 0
    Nav.openCursor(blind, null)
    check("enter over a hidden cursor row says so and opens nothing",
          blind.said.join("") + "|" + blind.opened, "That row is hidden by the filter.|0")

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

    var hidden = Fixture.pane()
    hidden.cursorIndex = 3
    Fixture.typeInto(hidden, "2026")
    check("a cursor the filter hides lands on the first row it left standing", hidden.cursorIndex, 5)
    check("and the view scrolls to the top of the narrowed listing", hidden.scrolled, 0)

    var kept = Fixture.pane()
    kept.cursorIndex = 6
    Fixture.typeInto(kept, "2026")
    check("a cursor the filter keeps does not move", kept.cursorIndex, 6)
    check("and nothing scrolled on its account", kept.scrolled, -1)

    var all = Fixture.pane("scr")
    Filter.selectAll(all)
    check("select all takes what is drawn, never the rows the filter hid", Fixture.picks(all), "0,1,5,6")
    var allPlain = Fixture.pane()
    Filter.selectAll(allPlain)
    check("and with no filter it still takes the whole listing", Fixture.picks(allPlain), "0,1,2,3,4,5,6")

    var span = Fixture.pane("scr")
    span.cursorIndex = 6
    Filter.extendTo(span, 1)
    check("extending over a filtered view skips the rows it hid", Fixture.picks(span), "1,5,6")
    var spanPlain = Fixture.pane()
    spanPlain.cursorIndex = 3
    Filter.extendTo(spanPlain, 1)
    check("and with no filter it is still a plain range", Fixture.picks(spanPlain), "1,2,3")

    // Shift+J whole: the anchor latches, the cursor steps through what is drawn and the selection
    // follows. "o" keeps 2, 3, 5 and 6, so two steps from row 2 reach 5 and not 4.
    var chain = Fixture.pane("o")
    chain.cursorIndex = 2
    Filter.extend(chain, 1)
    Filter.extend(chain, 1)
    check("shift J walks the selection down the rows that are drawn", Fixture.picks(chain), "2,3,5")
    check("and the cursor is on the last of them", chain.cursorIndex, 5)
    check("and the anchor stayed where the chain started", chain.selectionAnchor, 2)
    check("and the version moved once per step", chain.selectionVersion, 2)
    var chainPlain = Fixture.pane()
    chainPlain.cursorIndex = 2
    Filter.extend(chainPlain, 1)
    check("with no filter it is still a plain one-row range", Fixture.picks(chainPlain), "2,3")

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
}
