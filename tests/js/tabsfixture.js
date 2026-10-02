.import "../../ui/js/Selection.js" as Selection
.import "../../ui/js/History.js" as History

// The stub pane both tab suites drive, kept here so neither owns it: tests/js/tabs.js covers the
// tab list itself and tests/js/tabs-switch.js what a switch restores.
function pane(path) {
    var p = {
        path: path || "/home/gm/Work",
        home: "/home/gm",
        history: [History.entry("/home/gm", "Work", 3)],
        cursorIndex: 4,
        viewMode: "list",
        showHidden: false,
        // ViewState.state.hidden as ui/Pane.qml reads it: the standing preference, not this tab's.
        preferenceHidden: false,
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
    // A new listing forgets the selection; without ui/Pane.qml's keepHidden it takes the dotfile answer the tab being left chose.
    p.openWithoutHistory = function (next, options) {
        if (!options || options.keepHidden !== true)
            p.showHidden = p.preferenceHidden === true
        p.clearedAtOnce = !!options && options.clearAtOnce === true
        p.listed.push(next)
        p.path = next
        p.cursorIndex = 0
        p.selection.clear()
        p.selectionVersion++
        p.backend.listRequests += 1
    }
    p.backend = {
        sortBy: "name",
        sortDesc: false,
        // Every list bumps this, the watch's own re-read included: see ui/Backend.qml.
        listRequests: 0,
        sort: function (by, desc) { p.sorted.push(by + ":" + desc); this.sortBy = by; this.sortDesc = desc },
        window: function (start, count) { p.windows.push(start + ":" + count) },
        searchcancel: function () { p.searchRunning = false }
    }
    return p
}
