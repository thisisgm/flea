.import "../../ui/js/OpenWith.js" as OpenWith

// ui/js/OpenWith.js's own two functions, driven over a fake wire the way tests/js/thumbs.js drives
// Thumbs.js. The state is the wire's and the pane is what the wire holds, so the fake is that shape
// exactly; the slot is one row's answer, so every check here is about what one slot holds.

function wire() {
    return {
        pane: {
            cursorIndex: 2,
            cursorRow: { n: "photo.png", d: false },
            listInFlight: false,
            backend: { asked: [], askHandlers: function (row) { this.asked.push(row) } }
        },
        openWithRow: -1,
        openWithApps: []
    }
}

function run(check) {
    var w = wire()
    OpenWith.ask(w)
    check("a file row asks the backend for its own row", w.pane.backend.asked.join(",") + "|" + w.openWithRow, "2|2")

    OpenWith.answered(w, 2, [{ name: "Viewer", path: "/usr/share/applications/v.desktop" }])
    check("the answer fills the slot for the row it was asked for",
          w.openWithApps.length + "|" + w.openWithApps[0].name, "1|Viewer")

    OpenWith.ask(w)
    check("a second menu over the same row asks nothing, the answer already cached", w.pane.backend.asked.length, 1)

    OpenWith.ask(w)
    check("the ask is idempotent while the answer stands", w.openWithApps.length, 1)

    var moved = wire()
    moved.openWithRow = 2
    moved.openWithApps = [{ name: "Stale", path: "/x.desktop" }]
    moved.pane.cursorIndex = 9
    moved.pane.cursorRow = { n: "notes.txt", d: false }
    OpenWith.ask(moved)
    check("a menu over another row asks afresh and empties the slot meanwhile",
          moved.pane.backend.asked.join(",") + "|" + moved.openWithApps.length, "9|0")

    var dir = wire()
    dir.pane.cursorRow = { n: "photos", d: true }
    OpenWith.ask(dir)
    check("a directory row clears the slot and asks nothing", dir.openWithRow + "|" + dir.openWithApps.length + "|" + dir.pane.backend.asked.length, "-1|0|0")

    var loading = wire()
    loading.pane.cursorRow = null
    OpenWith.ask(loading)
    check("a listing with no row under the cursor clears the slot too", loading.openWithRow + "|" + loading.openWithApps.length, "-1|0")

    var w2 = wire()
    w2.openWithRow = 2
    OpenWith.answered(w2, 9, [{ name: "Late", path: "/x.desktop" }])
    check("an answer for a row the menu has since left is dropped", w2.openWithApps.length, 0)

    var w3 = wire()
    w3.openWithRow = 2
    w3.pane.listInFlight = true
    OpenWith.answered(w3, 2, [{ name: "Old listing", path: "/x.desktop" }])
    check("an answer riding a superseded listing is dropped", w3.openWithApps.length, 0)

    var w4 = wire()
    w4.openWithRow = 2
    OpenWith.answered(w4, 2, [])
    check("an empty answer is a real answer, and a second menu over that row asks nothing",
          w4.openWithApps.length + "|" + (function () { OpenWith.ask(w4); return w4.pane.backend.asked.length })(), "0|0")
}
