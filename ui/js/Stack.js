.pragma library

.import "Format.js" as Format

// The Stack is a place, not a directory. The token cannot collide with a listing path: every path
// this window holds is absolute, and this one does not start with "/".
var TOKEN = "flea:stack"
var LABEL = "The Stack"
var DENY = "The Stack is a list of files, not a folder."
var SYSTEM_SCRIPT = "/usr/lib/flea/the-stack.py"

function isStack(path) {
    return String(path || "") === TOKEN
}

function userScript(home) {
    return String(home || "") + "/.local/share/flea/the-stack.py"
}

// The packaged helper wins. The copy under the home directory is what a package update leaves behind.
function command(home, extra) {
    var args = ["python3", "-c",
        "import os,sys\n"
        + "p=next((c for c in (sys.argv[1], sys.argv[2]) if c and os.path.isfile(c)), '')\n"
        + "sys.exit(2) if not p else os.execv(sys.executable,[sys.executable,p]+sys.argv[3:])\n",
        SYSTEM_SCRIPT, userScript(home)]
    var more = extra || []
    for (var i = 0; i < more.length; i++)
        args.push(String(more[i]))
    return args
}

function historyPath(dataHome, home) {
    var root = String(dataHome || "")
    if (root.charAt(0) !== "/")
        root = String(home || "") + "/.local/share"
    return root.replace(/\/+$/, "") + "/recently-used.xbel"
}

function touchPath(home) {
    return String(home || "") + "/.local/state/flea/the-stack.json"
}

// listpaths names each row as its absolute path with the leading slash removed, and the listing
// base is "/". Joining the token onto that name would invent flea:stack/home/...
function rowPath(name) {
    var text = String(name || "")
    if (text.length === 0)
        return ""
    return text.charAt(0) === "/" ? text : "/" + text
}

function place(name, home) {
    var abs = rowPath(name)
    var cut = abs.lastIndexOf("/")
    var dir = cut <= 0 ? "/" : abs.substring(0, cut)
    return Format.tilde(dir, String(home || ""))
}

function leaf(name) {
    var abs = rowPath(name)
    var cut = abs.lastIndexOf("/")
    return cut < 0 || cut === abs.length - 1 ? abs : abs.substring(cut + 1)
}

function same(a, b) {
    if (!a || !b || a.length !== b.length)
        return false
    for (var i = 0; i < a.length; i++) {
        if (a[i] !== b[i])
            return false
    }
    return true
}

// A signal can hand the list back as a variant rather than a JS array. Both have a length.
function copyList(paths) {
    var list = []
    if (paths && typeof paths.length === "number") {
        for (var i = 0; i < paths.length; i++)
            list.push(String(paths[i]))
    }
    return list
}

// listing: this reply belongs to the open that asked for the place.
// A quiet re-read reloads only when the order changed and the pane is idle.
function readyAction(listing, probe, onStack, inFlight, unchanged) {
    if (listing)
        return "list"
    if (!probe || !onStack || inFlight || unchanged)
        return "keep"
    return "reload"
}

function busy(pane) {
    return !pane || pane.listInFlight || pane.renamingIndex >= 0 || pane.selectionCount() > 0
        || pane.menuVisible || (pane.filterQuery || "").length > 0 || pane.filterTyping
        || (pane.searchMode || "").length > 0
}

function shouldTouch(pane, path) {
    return !!pane && isStack(pane.path) && String(path || "").charAt(0) === "/"
}

// A probe that already ranked the files hands that list to listpaths. Otherwise the ranker runs.
function begin(host, model) {
    var pane = host.pane
    if (host.stackReadyPaths) {
        var ready = host.stackReadyPaths
        host.stackReadyPaths = null
        host.stackProbe = false
        host.stackShown = ready
        pane.backend.listPaths(ready, pane.windowSize)
        return
    }
    host.stackProbe = false
    model.refresh()
}

function ready(host, paths) {
    var pane = host.pane
    var list = copyList(paths)
    var listing = isStack(pane.listingPath) && pane.listInFlight
    var action = readyAction(listing, host.stackProbe, isStack(pane.path), pane.listInFlight, same(list, host.stackShown))
    host.stackProbe = false
    if (action === "list") {
        host.stackReadyPaths = null
        host.stackShown = list
        pane.backend.listPaths(list, pane.windowSize)
        return
    }
    if (action !== "reload")
        return
    var row = pane.rowFor(pane.cursorIndex)
    pane.pendingSelect = row ? pane.join(pane.path, row.n) : ""
    host.stackReadyPaths = list
    pane.openWithoutHistory(TOKEN)
}

function failed(host, text) {
    var pane = host.pane
    host.stackProbe = false
    host.stackReadyPaths = null
    if (!(isStack(pane.listingPath) && pane.listInFlight))
        return
    host.drop()
    pane.listInFlight = false
    pane.listedSeen = false
    pane.listingState = "error"
    pane.stateMessage = text || "The Stack could not be read."
    pane.message(pane.stateMessage, true)
}

// Quiet while a gesture owns the rows. The hold tries again once that gesture is finished.
function note(host, model, hold) {
    var pane = host.pane
    if (!pane || !isStack(pane.path))
        return
    if (busy(pane)) {
        hold.restart()
        return
    }
    host.stackProbe = true
    model.refresh()
}
