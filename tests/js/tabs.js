.import "../../ui/js/Selection.js" as Selection
.import "../../ui/js/Tabs.js" as Tabs

function pane(path) {
    var p = {
        path: path || "/home/gm/Work",
        home: "/home/gm",
        history: ["/home/gm"],
        cursorIndex: 4,
        viewMode: "list",
        showHidden: false,
        searchMode: "",
        searchFrom: "",
        searchQuery: "",
        searchRunning: false,
        searchCancelled: false,
        searchScanned: 0,
        filterQuery: "",
        filterTyping: false,
        listInFlight: false,
        tabs: null,
        total: 20,
        windowSize: 40,
        said: [],
        listed: [],
        sorted: [],
        windows: [],
        preview: { active: false, closed: 0, close: function () { this.active = false; this.closed += 1 } },
        selection: Selection.create(),
        selectionVersion: 0
    }
    p.selectedIndices = function () { return p.selection.indices() }
    p.clearSelection = function () { p.selection.clear(); p.selectionVersion++ }
    p.setCursor = function (i) { p.cursorIndex = i }
    p.message = function (text) { p.said.push(text) }
    p.openWithoutHistory = function (next) { p.listed.push(next); p.path = next; p.cursorIndex = 0 }
    p.backend = {
        sortBy: "name",
        sortDesc: false,
        sort: function (by, desc) { p.sorted.push(by + ":" + desc); this.sortBy = by; this.sortDesc = desc },
        window: function (start, count) { p.windows.push(start + ":" + count) },
        searchcancel: function () { p.searchRunning = false }
    }
    return p
}

function run(check) {
    // A search sets pane.path to the scope it walks and keeps the origin in searchFrom, which
    // dropOverlay clears. openNew is driven whole rather than restingPath alone, because the defect
    // was the ORDER of those two and a test on restingPath by itself passes either way.
    var searching = pane("/")
    searching.searchMode = "results"
    searching.searchFrom = "/home/gm/Work"
    Tabs.openNew(searching)
    check("a tab opened from search results remembers where the user was, not the scope",
          searching.tabs.items[1].path, "/home/gm/Work")
    check("and so does the tab it was opened from", searching.tabs.items[0].path, "/home/gm/Work")
    check("and the search is dropped once the paths are taken", searching.searchMode, "")
    check("and the pane lands on the path the tab records", searching.path, "/home/gm/Work")
    check("carrying no cursor from the search's own listing", searching.tabs.items[1].cursorIndex, 0)
    check("and no selection from it either", searching.tabs.items[1].selected.length, 0)

    var plain = pane("/tmp/here")
    Tabs.openNew(plain)
    check("an ordinary listing opens its tab on its own path", plain.tabs.items[1].path, "/tmp/here")

    // The cursor is clamped when a listing arrives and the selection was not, so a row past the end
    // of a directory that shrank while the tab was hidden was selected anyway.
    var shrunk = { total: 3, selectionVersion: 0, toggled: [], clearSelection: function () {} }
    shrunk.selection = { toggle: function (i) { shrunk.toggled.push(i) } }
    Tabs.restoreSelection(shrunk, [0, 2, 7, 40])
    check("a selection restored into a shrunken directory keeps only rows that still exist",
          shrunk.toggled.join(","), "0,2")
    var none = { total: 0, selectionVersion: 5, toggled: [], clearSelection: function () {} }
    none.selection = { toggle: function (i) { none.toggled.push(i) } }
    Tabs.restoreSelection(none, [1, 2])
    check("and a restore that kept nothing does not announce a selection change",
          none.toggled.length + "|" + none.selectionVersion, "0|5")

    // The clamp alone was not enough: a row deleted BELOW a kept index leaves that index in range
    // and naming a different file, which trash would then act on. A switch that re-lists carries no
    // selection at all now, and the re-list's own reset is what clears it.
    var moved = pane("/tmp/a")
    moved.tabs = { items: [{ path: "/tmp/a" }, { path: "/tmp/b", history: [], cursorIndex: 1,
                            viewMode: "list", showHidden: false, selected: [0, 1, 2],
                            sortBy: "name", sortDesc: false }],
                   index: 0, pendingCursor: -1, pendingSortBy: "", pendingSortDesc: false }
    Tabs.selectAt(moved, 1)
    check("a switch that re-lists asks for no selection to be restored",
          moved.tabs.pendingSelected === undefined, true)
    check("and it did re-list, which is what clears the selection", moved.listed.join(","), "/tmp/b")

    // F3 and F4: a refusal and a background close must each cost the user nothing else.
    var full = pane("/tmp/full")
    var nine = []
    for (var t = 0; t < 9; t++) nine.push({ path: "/tmp/" + t })
    full.tabs = { items: nine, index: 0, pendingCursor: -1, pendingSelected: null,
                  pendingSortBy: "", pendingSortDesc: false }
    full.preview.active = true
    full.searchMode = "results"
    full.searchRunning = true
    Tabs.openNew(full)
    check("a refused tenth tab says so", full.said[full.said.length - 1], "Nine tabs is the most.")
    check("and leaves the preview open", full.preview.active, true)
    check("and does not cancel the running search", full.searchRunning, true)
    check("and the search itself is still standing", full.searchMode, "results")

    // selectAt got the same read-before-dropOverlay hoist, and nothing drove it from a search.
    var leaving = pane("/")
    leaving.searchMode = "results"
    leaving.searchFrom = "/home/gm/Work"
    var other = { path: "/tmp/other", history: [], cursorIndex: 0, viewMode: "list",
                  showHidden: false, selected: [], sortBy: "name", sortDesc: false }
    leaving.tabs = { items: [other, { path: "/" }], index: 1,
                     pendingCursor: -1, pendingSortBy: "", pendingSortDesc: false }
    Tabs.selectAt(leaving, 0)
    check("the tab left behind during a search records where the user was",
          leaving.tabs.items[1].path, "/home/gm/Work")

    var many = pane("/tmp/one")
    many.tabs = { items: [{ path: "/tmp/one" }, { path: "/tmp/two" }, { path: "/tmp/three" }],
                  index: 0, pendingCursor: -1, pendingSelected: null,
                  pendingSortBy: "", pendingSortDesc: false }
    many.preview.active = true
    Tabs.closeAt(many, 2)
    check("closing a background tab leaves the current tab's preview alone", many.preview.active, true)
    check("and still removes it", Tabs.count(many), 2)

    // The strip's binding reads these four by name rather than through the pane, so a comma
    // expression is not needed to make a label re-read when a tab opens; qmllint flagged that.
    var items = [{ path: "/home/gm" }, { path: "/tmp/one" }]
    check("the current tab draws the pane's live path, not its snapshot",
          Tabs.pathAt({ items: items, index: 1 }, 1, 1, "/tmp/moved"), "/tmp/moved")
    check("a hidden tab draws its own snapshot",
          Tabs.pathAt({ items: items, index: 1 }, 1, 0, "/tmp/moved"), "/home/gm")
    check("no tabs at all still answers a string",
          Tabs.pathAt(null, 0, 3, "/home/gm"), "")

    check("a root tab is labelled with its separator", Tabs.label("/", ""), "/")
    check("the home directory uses the rail's own Home label", Tabs.label("/home/gm", "/home/gm"), "Home")
    check("a home child is labelled with its leaf, not the tilde form",
          Tabs.label("/home/gm/Work", "/home/gm"), "Work")
    check("one pane with no tab state still counts as one tab", Tabs.count(pane()), 1)

    var born = pane()
    Tabs.act("tabNew", born)
    check("t seeds the current folder and opens a second tab on it", Tabs.count(born), 2)
    check("and lands on the new tab", Tabs.currentIndex(born), 1)
    check("and does not re-list, because both tabs name the same directory", born.listed.length, 0)

    born.path = "/home/gm/Downloads"
    check("a navigate updates the current tab's label without a switch",
          Tabs.labelAt(born, 1), "Downloads")
    check("and leaves the other tab's snapshot alone", Tabs.labelAt(born, 0), "Work")

    Tabs.act("tab1", born)
    check("1 switches to the first tab", Tabs.currentIndex(born), 0)
    check("and lists the snapshot path, because it is a different directory",
          born.listed.join(","), "/home/gm/Work")
    check("and keeps the cursor to restore after rows arrive", born.tabs.pendingCursor, 4)

    Tabs.applyPending(born)
    check("the pending cursor lands once rows arrive", born.cursorIndex, 4)

    var missing = pane()
    Tabs.act("tab3", missing)
    check("a digit with no such tab says so in words", missing.said.join(""), "No tab 3.")

    var last = pane()
    Tabs.act("tabClose", last)
    check("w on the only tab refuses rather than closing the window",
          last.said.join(""), "Can't close the last tab.")

    var pair = pane("/home/gm/a")
    Tabs.act("tabNew", pair)
    pair.path = "/home/gm/b"
    Tabs.act("tabClose", pair)
    check("w on a second tab leaves one", Tabs.count(pair), 1)
    check("and lists the tab that remains", pair.listed.join(","), "/home/gm/a")

    var capped = pane()
    var n
    for (n = 0; n < 12; n++)
        Tabs.act("tabNew", capped)
    check("the ninth tab is the last one t will open", Tabs.count(capped), 9)
    check("and the tenth says so", capped.said[capped.said.length - 1], "Nine tabs is the most.")

    var loading = pane()
    loading.listInFlight = true
    Tabs.act("tabNew", loading)
    check("t while a listing is in flight uses the same sentence navigation does",
          loading.said.join(""), "A directory is already loading.")

    var previewing = pane()
    previewing.preview.active = true
    Tabs.act("tabNew", previewing)
    check("t closes an open preview, so the new tab is not sitting under one",
          previewing.preview.closed, 1)

    var pending = pane("/home/gm/a")
    pending.tabs = {
        items: [],
        index: 0,
        pendingCursor: 9,
        pendingSelected: null,
        pendingSortBy: "size",
        pendingSortDesc: true
    }
    Tabs.applyPending(pending)
    check("a pending size order is asked for before the cursor is restored",
          pending.sorted.join(",") + "|" + pending.cursorIndex, "size:true|4")
    Tabs.applyPending(pending)
    check("the next rows reply restores the cursor", pending.cursorIndex, 9)
}
