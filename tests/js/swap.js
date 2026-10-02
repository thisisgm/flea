.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/Swap.js" as Swap

// The listing swap, AGENTS.md "The listing swap": its decisions, the Nav route over a swap stub, and the key gate.

// A pane showing a settled listing, with every member the reset writes set to something it forgets.
function pane(holds) {
    var p = {
        listInFlight: false,
        listedSeen: true,
        path: "/home/gm/Work",
        listingPath: "",
        total: 40,
        held: 10,
        rows: [{ n: "a" }],
        kindNames: ["Plain text document"],
        thumbState: "stale",
        dirSizeState: "stale",
        cursorIndex: 7,
        renamingIndex: 4,
        trashArmedAt: 12345,
        listingState: "ready",
        stateMessage: "something",
        lockedMode: 0o40750,
        filterQuery: "scr",
        filterTyping: true,
        cleared: 0,
        holds: 0,
        said: [],
        sent: [],
        gitRepo: false,
        gitBranch: "",
        gitRoot: "",
        gitStatusId: 0,
    }
    p.clearSelection = function () { p.cleared += 1 }
    p.message = function (text, isError) { p.said.push(text) }
    p.listArea = { primeSettle: function () {} }
    p.backend = {
        list: function (path, first, hidden) { p.sent.push("list " + path) },
        askFsInfo: function () { p.sent.push("fsinfo") }, askGitStatus: function (id, path) { p.sent.push("gitstatus") }
    }
    // ui/PaneSwap.qml hold(): how often it was asked, with what, and whether the rows on screen stay up.
    p.swap = { holding: false, hold: function (ask) { p.holds += 1; p.asked = ask; p.swap.holding = holds; return holds } }
    return p
}

// The whole reset, one field per member ui/js/Nav.js forget() writes, so a half-done one shows.
function snapshot(p) {
    return [p.total, p.held, p.rows.length, p.kindNames.length, p.thumbState === "stale", p.dirSizeState === "stale",
            p.cursorIndex, p.trashArmedAt, p.renamingIndex, p.filterQuery, p.filterTyping, p.listingState,
            p.stateMessage, p.lockedMode, p.cleared].join("|")
}

var UNTOUCHED = "40|10|1|1|true|true|7|12345|4|scr|true|ready|something|16872|0"
var HELD = "40|10|1|1|true|true|7|12345|4|scr|false|ready|something|16872|0"
var FORGOTTEN = "0|0|0|0|false|false|0|0|-1||false|loading||0|1"

// Keys through Focus.handleKey and Focus.act over a recording backend; rows is what is still drawn, and F2 records as a rename.
function keyPane(inFlight, rows) {
    var p = {
        focusView: "list", viewMode: "list", searchMode: "", filterTyping: false, filterQuery: "", shown: null, path: "/home/gm/Work",
        listInFlight: inFlight, listingState: inFlight && rows.length === 0 ? "loading" : "ready",
        total: rows.length, held: 0, rows: rows, cursorIndex: 0, selectionVersion: 0, selectionAnchor: 0,
        keySequence: "", keySequenceIdentity: "", inputAt: 0, rowsAt: 0, trashArmedAt: 0, trashedFirst: -1,
        acted: [], sent: [], said: [],
        preview: { active: false, isMedia: false, isPdf: false },
        shareBrowser: { active: false }
    }
    p.rowFor = function (index) { return index >= 0 && index < p.rows.length ? p.rows[index] : null }
    p.join = function (base, name) { return base + "/" + name }
    p.selectedIndices = function () { return [] }
    p.renameEditor = function () { return null }
    p.message = function (text, isError) { p.said.push(text) }
    p.clearSelection = function () {}
    p.listArea = { primeSettle: function () {} }
    p.backend = {
        trash: function (rows) { p.sent.push("trash " + rows.join(",")) },
        askPaths: function (rows) { p.sent.push("paths " + rows.join(",")) },
        send: function (request) { p.sent.push(request.c) },
        list: function (path) { p.sent.push("list " + path) },
        askFsInfo: function () {}, askGitStatus: function () {}
    }
    p.openCursor = function () { Nav.openCursor(p, { open: function (path) { p.sent.push("open " + path) } }) }
    p.open = function (path) { p.sent.push("list " + path) }
    p.openParent = function () { Nav.parent(p) }
    p.act = function (action) {
        p.acted.push(action)
        if (action === "rename") p.sent.push("rename")
        else Focus.act(action, p)
    }
    return p
}

function key(code, text) {
    return { key: code, text: text, modifiers: Qt.NoModifier }
}

// A run of keys under the Default preset through one pane: what reached the pane, what reached the backend, and what was said.
function pressedOn(p, keys) {
    var wasPreset = Keymap.preset
    Keymap.setPreset("default")
    for (var i = 0; i < keys.length; i++)
        Focus.handleKey(keys[i], p, { renameEditor: function () { return null } })
    Keymap.setPreset(wasPreset)
    return p
}

function pressed(inFlight, rows, keys) {
    return pressedOn(keyPane(inFlight, rows), keys)
}

var FILE = [{ n: "a.txt", d: false, i: "text-x-generic" }]
var DD = [key(0, "d"), key(0, "d")]
var DELETE = [key(Qt.Key_Delete, "")]
var F2 = [key(Qt.Key_F2, "")]
var RETURN = [key(Qt.Key_Return, "\r")]

function run(check) {
    // What is held: a listing the pane has settled on, in the view and the mode it was listed in.
    var settled = Swap.begin(Swap.idle(), "ready", "", {}, 1000)
    check("a settled listing stays drawn while the next one is out", settled.holding, true)
    check("an empty folder's hero and a refusal's message are what their folders look like, so they are held too",
          Swap.begin(Swap.idle(), "empty", "", {}, 0).holding + "|" + Swap.begin(Swap.idle(), "locked", "", {}, 0).holding,
          "true|true")
    check("a pane still loading has nothing to hold", Swap.begin(Swap.idle(), "loading", "", {}, 0).holding, false)
    check("a query line or a walk owns the header, so nothing is held under it",
          Swap.begin(Swap.idle(), "ready", "typing", {}, 0).holding + "|" + Swap.begin(Swap.idle(), "ready", "results", {}, 0).holding,
          "false|false")
    var walked = Swap.heard(Swap.idle(), true)
    check("a walk's matches still on screen once its search closed are not held under the directory's header",
          Swap.begin(walked, "ready", "", {}, 0).holding, false)
    check("and the directory listing that follows makes the next listing holdable again",
          Swap.begin(Swap.heard(walked, false), "ready", "", {}, 0).holding, true)
    check("a caller whose rows were never listed in the view now showing clears at once",
          Swap.begin(Swap.idle(), "ready", "", { clearAtOnce: true }, 0).holding, false)
    check("the query a re-read hands back rides with its own hold and no other",
          Swap.begin(Swap.idle(), "ready", "", { keptQuery: "scr" }, 0).query + "|" + Swap.begin(settled, "ready", "", {}, 0).query,
          "scr|")

    // Which listed line is kept: the one naming the directory asked for, while rows are held or fallen back.
    var inner = "/home/gm/Work/inner"
    check("the list's own listed line is kept for the held rows", Swap.onListed(settled, true, inner, inner), Swap.KEEP)
    var stashed = Swap.kept(settled, { total: 300, path: inner })
    check("and a later one naming the same directory replaces it", Swap.onListed(stashed, true, inner, inner), Swap.KEEP)
    check("a pane holding nothing keeps nothing, so its own listing's listed line is applied as it arrives",
          Swap.onListed(Swap.idle(), true, inner, inner), Swap.APPLY)
    check("and the rows behind it are applied too, never landed", Swap.onRows(Swap.idle(), true, true), Swap.APPLY)
    check("with no listing out a line naming another folder than the last one asked for is applied, a sort at rest among them",
          Swap.onListed(Swap.idle(), false, "/home/gm/Work", inner), Swap.APPLY)
    check("while the same line with a listing out is dropped, so only the listing-out gate tells them apart",
          Swap.onListed(Swap.idle(), true, "/home/gm/Work", inner), Swap.DROP)
    check("and the state it was kept from is never written in place", settled.listed, null)
    check("a line naming no path is the listing's own, as it always was", Swap.onListed(settled, true, "", inner), Swap.KEEP)
    // s on /home/gm/Work, then Enter on inner before the sort answered: the sort's listed and rows come first.
    check("a sort's reply naming the folder being left is dropped", Swap.onListed(settled, true, "/home/gm/Work", inner), Swap.DROP)
    check("and so are its rows, which arrive with no listed line kept", Swap.onRows(settled, true, false), Swap.DROP)
    check("then the list's rows land with its kept line", Swap.onRows(stashed, true, true), Swap.LAND)
    check("a search's opening line for its scope, after its escape re-listed where it began, is dropped too",
          Swap.onListed(Swap.idle(), true, "/home/gm", "/home/gm/Downloads"), Swap.DROP)
    check("while one answered with no listing out is applied", Swap.onListed(Swap.idle(), false, "/home/gm", inner), Swap.APPLY)
    check("and a window at rest lands alone", Swap.onRows(Swap.idle(), false, false), Swap.APPLY)

    // How a hold ends: the swap, the cap, a failure, or the listing ending some other way.
    var landed = Swap.landed(stashed)
    check("the swap ends the hold with nothing kept and no loading state",
          landed.holding + "|" + landed.listed + "|" + landed.fellBack + "|" + landed.walk, "false|null|false|false")
    var expired = Swap.expired(stashed)
    check("the cap ends it into the loading state and keeps a listed line already kept for its rows",
          expired.holding + "|" + (expired.listed && expired.listed.total) + "|" + expired.fellBack, "false|300|true")
    check("whose rows land with it, so the mark stands until they do", Swap.onRows(expired, true, true), Swap.LAND)
    check("and a listed line arriving after the cap waits for its rows too", Swap.onListed(Swap.expired(settled), true, inner, inner), Swap.KEEP)
    check("which the listing's own end clears", Swap.ended(expired).fellBack + "|" + Swap.ended(expired).listed, "false|null")
    // tests/ui-operations-design.sh: a path-bar open with the backend stopped runs out the cap, then its queued replies land.
    var paused = Swap.expired(settled)
    check("a paused open to an empty folder keeps its listed line past the cap and lands on its rows, a sort's queued ahead dropped",
          [Swap.onListed(paused, true, "/home/gm/Work", inner), Swap.onRows(paused, true, false), Swap.onListed(paused, true, inner, inner),
           Swap.onRows(Swap.kept(paused, { total: 0, path: inner }), true, true)].join(","), "drop,drop,keep,land")
    var dropped = Swap.dropped(stashed)
    check("a failure ends it without the loading state, and the failure draws the rest",
          dropped.holding + "|" + dropped.fellBack, "false|false")
    var failing = { listInFlight: true, listedSeen: true, swap: { drop: function () { failing.droppedOut = failing.listInFlight } } }
    check("a failed listing lets held rows go while it is still out, then ends it",
          Swap.failListing(failing, "scan") + "|" + failing.droppedOut + "|" + failing.listInFlight + "|" + failing.listedSeen,
          "true|true|false|false")
    var refused = { listInFlight: true, listedSeen: true, swap: { drop: function () { refused.dropped = true } } }
    check("a stale refusal ended only its own request, so the listing out stays out and its rows stay held",
          Swap.failListing(refused, "stale") + "|" + (refused.dropped === true) + "|" + refused.listInFlight, "false|false|true")

    // What a pane answers while a listing is out, held or fallen back: nothing that acts on a row.
    var rowActions = ["cursorDown", "cursorFirst", "pageDown", "toggleSelect", "selectAll", "trashArm", "trash", "copy",
                      "cut", "paste", "menu", "preview", "rename", "duplicate", "properties", "deletePermanently",
                      "filter", "search", "sortNext", "toggleHidden", "newFolder", "undo"]
    check("a listing out swallows every row action",
          rowActions.filter(function (a) { return !Swap.swallows(true, a) }).join(","), "")
    check("but lets through the navigations, which refuse themselves, escape, and the keys that change the window",
          Swap.ANSWERED_WHILE_LISTING.filter(function (a) { return Swap.swallows(true, a) }).join(",") + "|"
          + Swap.ANSWERED_WHILE_LISTING.join(","),
          "|open,parent,historyBack,historyForward,escape,viewList,viewColumns,viewGrid,sidebar,windowNew,togglePreview")
    check("a pane with no listing out swallows nothing", Swap.swallows(false, "trash"), false)
    check("and an unbound key is left to the route that names the filter", Swap.swallows(true, ""), false)

    // The control: at rest the same keys over a row really reach the backend through the real route.
    check("at rest dd trashes the cursor row", pressed(false, FILE, DD).sent.join(","), "trash 0")
    check("and Delete does too", pressed(false, FILE, DELETE).sent.join(","), "trash 0")
    check("and F2 opens the rename", pressed(false, FILE, F2).sent.join(","), "rename")
    check("and Return opens the row", pressed(false, FILE, RETURN).sent.join(","), "open /home/gm/Work/a.txt")
    // A fallen-back loading state forgot the rows with the cursor on 0, which the backend already numbers in the new directory.
    var loadingDd = pressed(true, [], DD)
    check("in a fallen-back loading state dd sends nothing, where row 0 would be the new directory's file",
          loadingDd.sent.join(",") + "|" + loadingDd.acted.join(","), "|")
    check("and says why, once per key", loadingDd.said.join("|"), Swap.LOADING + "|" + Swap.LOADING)
    check("Delete sends nothing", pressed(true, [], DELETE).sent.length, 0)
    check("F2 opens no rename", pressed(true, [], F2).sent.length + "|" + pressed(true, [], F2).acted.length, "0|0")
    var loadingReturn = pressed(true, [], RETURN)
    check("Return reaches the pane, which refuses it with the sentence every refused navigation gives",
          loadingReturn.acted.join(",") + "|" + loadingReturn.sent.length + "|" + loadingReturn.said.join(""),
          "open|0|" + Swap.LOADING)
    // Held rows are the same case with rows still drawn, and the cursor on one of them.
    check("held rows send nothing for dd, Delete, F2 or Return either",
          [DD, DELETE, F2, RETURN].map(function (keys) { return pressed(true, FILE, keys).sent.length }).join(","), "0,0,0,0")
    var heldBack = pressed(true, FILE, [key(Qt.Key_Backspace, "\b")])
    check("while Backspace still reaches the pane, which refuses it the same way",
          heldBack.acted.join(",") + "|" + heldBack.sent.length + "|" + heldBack.said.join(""), "parent|0|" + Swap.LOADING)

    // What one frame showed while a listing was out.
    check("a listing out drawn in the loading state before the cap is the blank frame", Swap.frameKind(true, "loading", false), "blank")
    check("after the cap it is the loading state a slow folder always showed", Swap.frameKind(true, "loading", true), "loading")
    check("held rows are no blank frame, and neither is a pane at rest",
          Swap.frameKind(true, "ready", false) + "|" + Swap.frameKind(true, "empty", false) + "|" + Swap.frameKind(false, "loading", false), "||")

    // ui/js/Nav.js with a swap that holds: the request goes out and nothing on screen is touched.
    var held = pane(true)
    Nav.openWithoutHistory(held, inner)
    check("a held listing asks for the directory", held.sent.join(","), "list /home/gm/Work/inner,fsinfo,gitstatus")
    check("and is in flight, with the directory asked for recorded for a drop",
          held.listInFlight + "|" + held.listedSeen + "|" + held.listingPath, "true|false|/home/gm/Work/inner")
    check("and forgets nothing on screen but the filter line's caret: count, window, cursor, selection, filter, rename and state",
          snapshot(held), HELD)
    check("and the breadcrumb stays on the directory still drawn", held.path, "/home/gm/Work")
    Nav.forget(held, "")
    check("the forget ui/PaneSwap.qml runs when the rows go is the whole reset", snapshot(held), FORGOTTEN)
    var reread = pane(true)
    Nav.forget(reread, "scr")
    check("and a re-read that handed its query back keeps the filter, but not its caret",
          reread.filterQuery + "|" + reread.filterTyping + "|" + reread.cursorIndex, "scr|false|0")
    var asked = pane(true)
    Nav.openWithoutHistory(asked, "/tmp", { clearAtOnce: true, keptQuery: "q" })
    var handed = asked.asked || {}
    check("the caller's options reach the swap whole", handed.clearAtOnce + "|" + handed.keptQuery, "true|q")
    // A filter being typed when a refresh after a write holds: x is cut, so it must meet the gate rather than the query.
    var typing = keyPane(false, FILE)
    typing.filterTyping = true
    typing.filterQuery = "a"
    typing.swap = { hold: function () { return true } }
    Nav.openWithoutHistory(typing, "/home/gm/Work")
    pressedOn(typing, [key(0, "x")])
    check("a key typed over held rows meets the gate rather than a filter the swap then forgets",
          typing.filterQuery + "|" + typing.filterTyping + "|" + typing.said.join(""), "a|false|" + Swap.LOADING)

    // With nothing held the reset runs at the request, exactly as every listing did before the swap.
    var unheld = pane(false)
    Nav.openWithoutHistory(unheld, inner)
    check("a pane with nothing held forgets at the request", snapshot(unheld), FORGOTTEN)
    var kept = pane(false)
    Nav.openWithoutHistory(kept, "/home/gm/Work", { keptQuery: "scr" })
    check("and hands a re-read's query back there too", kept.filterQuery, "scr")

    // The in-flight guard runs before any hold, so a second listing neither holds nor forgets.
    var busy = pane(true)
    busy.listInFlight = true
    Nav.openWithoutHistory(busy, inner)
    check("a second listing while one is out asks the swap nothing and sends nothing",
          busy.holds + "|" + busy.sent.length + "|" + busy.said.join(""), "0|0|A directory is already loading.")
    check("and forgets nothing", snapshot(busy), UNTOUCHED)
    // A selection made between a sort key and its rows lands on other files, so the new numbering clears it.
    check("a new listing number clears a selection the old one named", Swap.clearsSelection(4, 5), true)
    check("a later window in the same numbering clears nothing", Swap.clearsSelection(5, 5), false)
    check("and nothing held yet means nothing to clear", Swap.clearsSelection(0, 5), false)
}
