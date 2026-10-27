.import "../../ui/js/Filter.js" as Filter
.import "filterfixture.js" as Fixture

// Issue 315: the query leaving, by Esc or by backspace, parks the whole listing on the cursor's row, and a
// keystroke settles the cursor before the query is written. Both against the view's own pull: the count
// changing moves the view to its top and the view pulls the cursor onto a row there, inside the write.

function run(check) {
    // Issue 315 part 2: Esc with the window held far down the folder left the view at the top, drawing
    // rows the pane did not hold, so Esc parks the listing on the cursor's own row.
    var parked = Fixture.pane("2026", 1800)
    parked.total = 5000
    parked.cursorIndex = 1805
    Filter.escape(parked); parked.refresh()
    check("esc from a filter clears the query", parked.filterQuery, "")
    check("and scrolls to the cursor's row, which is its view position with no filter up", parked.scrolled, 1805)
    // The count growing on the write of the query moves the view to its top and the view pulls the cursor onto a row there
    // (ui/StableModel.qml resize, clampCursor), all inside the write: a stub whose query write does the same pull.
    function pulling(query, held, pullTo) {
        var p = Fixture.pane(query, held)
        p.total = 5000
        var standing = p.filterQuery
        Object.defineProperty(p, "filterQuery", {
            get: function () { return standing },
            set: function (q) { if (q !== standing) { standing = q; p.refresh(); p.cursorIndex = pullTo; p.cursorSeq += 1 } },
            configurable: true
        })
        return p
    }
    var pulled = pulling("2026", 1800, 7)
    pulled.cursorIndex = 1805
    Filter.escape(pulled); pulled.refresh()
    check("esc puts the cursor back where it stood under the filter after the view's pull", pulled.cursorIndex + ":" + pulled.scrolled, "1805:1805")
    var emptied = pulling("6", 1800, 7)
    emptied.cursorIndex = 1805
    Filter.backspace(emptied); emptied.refresh()
    check("backspace to an empty query parks the listing on the cursor's row the same way", emptied.filterQuery + "|" + emptied.cursorIndex + ":" + emptied.scrolled, "|1805:1805")
    // Issue 315: a cursor on a row the pane holds nowhere is untested by the keystroke, so it stays for the folder's answer.
    var afar = pulling("20", 1800, 1805)
    afar.cursorIndex = 4000
    Filter.typed(afar, "2"); afar.refresh()
    check("a cursor outside every window stays where it was, put back after the view's pull, and nothing scrolled on its account",
          afar.cursorIndex + ":" + afar.scrolled, "4000:-1")
    // A cursor on a row only the filter window holds is tested by that row's name.
    var windowed = pulling("20", 1800, 1805)
    windowed.filterAnswer = { gen: 1, listing: 1, query: "20", list: [1805, 1806, 4000, 4001] }
    windowed.refresh()
    windowed.filterHeld = 2
    windowed.filterRows = [{ n: "video-2026.mp4", r: 4000 }, { n: "notes-2020.txt", r: 4001 }]
    windowed.cursorIndex = 4000
    Filter.typed(windowed, "2"); windowed.refresh()
    check("a cursor on a filter window row whose name still matches stays", windowed.cursorIndex, 4000)
    windowed.filterAnswer = { gen: 1, listing: 1, query: "202", list: [1805, 1806, 4000, 4001] }
    windowed.refresh()
    windowed.cursorIndex = 4001
    Filter.typed(windowed, "6"); windowed.refresh()
    check("and one whose name fails the query takes the first held match and the view goes to the top", windowed.cursorIndex + ":" + windowed.scrolled, "1805:0")
}
