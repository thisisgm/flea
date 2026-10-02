// A click on the folder already shown opens nothing: the rail hands the pane its own path, which Nav.openPlace leaves alone.
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/RailKeys.js" as RailKeys
.import "sourcefixture.js" as Source

// A settled pane whose open() is the real Nav.open, so a rail click lists as the app does.
function settled(path) {
    var p = {
        path: path, listingPath: "", listInFlight: false, listedSeen: true,
        searchMode: "", searchQuery: "", searchRunning: false,
        trash: { opened: false }, listingState: "ready",
        history: [], forwardHistory: [], sent: [], said: [],
        focusView: "rail", filterTyping: false, filterQuery: "",
        listingPreferences: {}, appliedListingPreferences: {},
        windowSize: 350, showHidden: false, storageClass: "", storageKnown: true,
        total: 150, held: 0, rows: [], kindNames: [], thumbState: {}, dirSizeState: {},
        cursorIndex: 40, trashArmedAt: 0, renamingIndex: -1,
        stateMessage: "", lockedMode: 0, pendingSelect: "", pendingMenu: false,
        collide: { opened: false }, menuVisible: false
    }
    p.clearSelection = function () {}
    p.message = function (text, isError) { p.said.push(text) }
    p.listArea = { primeSettle: function () {} }
    p.swap = { hold: function () { return false } }
    p.join = function (base, name) { return base + "/" + name }
    p.rowFor = function (index) { return p.rows[index - p.held] || null }
    p.backend = {
        list: function (target, first, hidden) { p.sent.push("list " + target); p.path = target; p.listInFlight = false },
        askFsInfo: function () {}
    }
    p.open = function (target) { Nav.open(p, target) }
    p.openWithoutHistory = function (target, options) { Nav.openWithoutHistory(p, target, options) }
    return p
}

function run(check) {
    var shown = settled("/home/gm/Work")
    var rail = { focusOnOpen: true }
    RailKeys.openFrom(shown, "/home/gm/Work", rail)
    check("a rail click on the folder already shown asks for no listing", shown.sent.join("|"), "")
    check("and focus still follows into the list", shown.focusView + "|" + rail.focusOnOpen, "list|false")
    var slashed = settled("/home/gm/Work")
    RailKeys.openFrom(slashed, "/home/gm/Work/", { focusOnOpen: false })
    check("a rail click with one trailing slash is the same folder", slashed.sent.join("|"), "")
    var elsewhere = settled("/home/gm/Work")
    RailKeys.openFrom(elsewhere, "/home/gm/Photos", { focusOnOpen: false })
    check("a rail click on another folder still lists", elsewhere.sent.join("|"), "list /home/gm/Photos")
    // The columns view taps the lit parent row, which is the pane's own path and lists nothing.
    var lit = settled("/home/gm/Work")
    check("a tap on the lit parent row lists nothing",
        Nav.openPlace(lit, "/home/gm/Work") + "|" + lit.sent.join("|"), "false|")
    var sibling = settled("/home/gm/Work")
    check("a tap on a sibling lists once",
        Nav.openPlace(sibling, "/home/gm/Photos") + "|" + sibling.sent.join("|"), "true|list /home/gm/Photos")
    // A locked or failed listing is retried by the same click, while search and Trash are left by it.
    var locked = settled("/home/gm/Work")
    locked.listingState = "locked"
    Nav.openPlace(locked, "/home/gm/Work")
    check("a locked listing is retried by the same click", locked.sent.join("|"), "list /home/gm/Work")
    var failed = settled("/home/gm/Work")
    failed.listingState = "error"
    Nav.openPlace(failed, "/home/gm/Work")
    check("a failed listing is retried by the same click", failed.sent.join("|"), "list /home/gm/Work")
    var searching = settled("/home/gm/Work")
    searching.searchMode = "results"
    Nav.openPlace(searching, "/home/gm/Work")
    check("search results are left by the same click", searching.sent.join("|"), "list /home/gm/Work")
    var trashing = settled("/home/gm/Work")
    trashing.trash.opened = true
    Nav.openPlace(trashing, "/home/gm/Work")
    check("the Trash view is left by the same click", trashing.sent.join("|"), "list /home/gm/Work")
    var empty = settled("/home/gm/Work")
    empty.listingState = "empty"
    Nav.openPlace(empty, "/home/gm/Work")
    check("an empty folder shown is kept", empty.sent.join("|"), "")
    var area = Source.source("ui/ColumnsArea.qml")
    var actBody = Source.slice(area, "function activateNeighbour", "function askThumb")
    check("a neighbour folder opens through the no-op route", actBody.indexOf("Nav.openPlace(root.pane, target)") >= 0, true)
}
