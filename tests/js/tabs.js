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
