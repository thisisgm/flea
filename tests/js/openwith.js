.import "../../ui/js/OpenWith.js" as OpenWith

// ui/js/OpenWith.js's own two functions, driven over a fake pane the way tests/js/thumbs.js drives
// Thumbs.js. The slot is one row's answer, so every check here is about what one slot holds.

function pane() {
    return {
        cursorIndex: 2,
        cursorRow: { n: "photo.png", d: false },
        openWithRow: -1,
        openWithApps: [],
        listInFlight: false,
        backend: { asked: [], askHandlers: function (row) { this.asked.push(row) } }
    }
}

function run(check) {
    var p = pane()
    OpenWith.ask(p)
    check("a file row asks the backend for its own row", p.backend.asked.join(",") + "|" + p.openWithRow, "2|2")

    OpenWith.answered(p, 2, [{ name: "Viewer", path: "/usr/share/applications/v.desktop" }])
    check("the answer fills the slot for the row it was asked for",
          p.openWithApps.length + "|" + p.openWithApps[0].name, "1|Viewer")

    OpenWith.ask(p)
    check("a second menu over the same row asks nothing, the answer already cached", p.backend.asked.length, 1)

    OpenWith.ask(p)
    check("the ask is idempotent while the answer stands", p.openWithApps.length, 1)

    var moved = pane()
    moved.openWithRow = 2
    moved.openWithApps = [{ name: "Stale", path: "/x.desktop" }]
    moved.cursorIndex = 9
    moved.cursorRow = { n: "notes.txt", d: false }
    OpenWith.ask(moved)
    check("a menu over another row asks afresh and empties the slot meanwhile",
          moved.backend.asked.join(",") + "|" + moved.openWithApps.length, "9|0")

    var dir = pane()
    dir.cursorRow = { n: "photos", d: true }
    OpenWith.ask(dir)
    check("a directory row clears the slot and asks nothing", dir.openWithRow + "|" + dir.openWithApps.length + "|" + dir.backend.asked.length, "-1|0|0")

    var loading = pane()
    loading.cursorRow = null
    OpenWith.ask(loading)
    check("a listing with no row under the cursor clears the slot too", loading.openWithRow + "|" + loading.openWithApps.length, "-1|0")

    var p2 = pane()
    p2.openWithRow = 2
    OpenWith.answered(p2, 9, [{ name: "Late", path: "/x.desktop" }])
    check("an answer for a row the menu has since left is dropped", p2.openWithApps.length, 0)

    var p3 = pane()
    p3.openWithRow = 2
    p3.listInFlight = true
    OpenWith.answered(p3, 2, [{ name: "Old listing", path: "/x.desktop" }])
    check("an answer riding a superseded listing is dropped", p3.openWithApps.length, 0)

    var p4 = pane()
    p4.openWithRow = 2
    OpenWith.answered(p4, 2, [])
    check("an empty answer is a real answer, and a second menu over that row asks nothing",
          p4.openWithApps.length + "|" + (function () { OpenWith.ask(p4); return p4.backend.asked.length })(), "0|0")
}
