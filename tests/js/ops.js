.import "menuhunt.js" as MenuHunt
.import "../../ui/js/Clipboard.js" as Clipboard
.import "../../ui/js/Ops.js" as Ops
.import "../../ui/js/Transfer.js" as Transfer

function run(check) {
    var sent = [], messages = []
    var sender = {reason: "Peer went offline", send: function(id, paths) { return false }, labelFor: function() { return "Laptop" }}
    var pane = {cursorIndex: 2, path: "/fixture", rowFor: function() { return {n: "source", d: false} },
        join: function(base, name) { return base + "/" + name }, message: function(text, failed) { messages.push([text, failed]) }}
    Ops.sendTaildrop(pane, sender, "peer", "/captured/source")
    // The reason is a shared token both front ends read; the bar names its subject, as the TUI does.
    check("a refused Taildrop dispatch names the subject and its cause", JSON.stringify(messages),
          JSON.stringify([["Taildrop \u00b7 Peer went offline", true]]))
    messages = []
    sender.send = function(id, paths) { sent.push([id, paths]); return true }
    Ops.sendTaildrop(pane, sender, "peer", "/captured/source")
    check("Taildrop uses the backend-validated cursor path", JSON.stringify(sent), JSON.stringify([["peer", ["/captured/source"]]]))
    check("accepted Taildrop dispatch is identified as sending", JSON.stringify(messages), JSON.stringify([["Sending source to Laptop.", false]]))
    messages = []
    Ops.sendTaildrop(pane, sender, "peer")
    check("an absent validated cursor path is refused", JSON.stringify(messages), JSON.stringify([["Cursor source was not validated; reopen the menu.", true]]))
    check("missing validation cannot fall back to a live cursor file", sent.length, 1)
    // Two kinds of success, and the operator has to be able to tell them apart.
    check("a verified extract says so plainly", Ops.archiveDoneLine(true), "Archive written.")
    check("and an unverified one says what was not checked",
          Ops.archiveDoneLine(false),
          "Extracted. The archive index could not be read, so this was not verified.")

    check("one item is singular and two are not",
          Ops.items(1) + " / " + Ops.items(2) + " / " + Ops.items(0),
          "1 item / 2 items / 0 items")

    // The Operations board separates its live item name with a middle dot.
    check("a copy in flight reads the way the canvas draws it",
          Ops.progressLine({ moving: false, n: 5, index: 1, name: "photo.heic" }),
          "Copying 2 of 5 · photo.heic")
    check("a move says so instead",
          Ops.progressLine({ moving: true, n: 5, index: 1, name: "photo.heic" }),
          "Moving 2 of 5 · photo.heic")
    // A directory item reports no name until its first line arrives, and the count still reads.
    check("an item with no name yet still counts",
          Ops.progressLine({ moving: false, n: 2, index: 0, name: "" }),
          "Copying 1 of 2")

    check("a finished copy names its own reversal",
          Ops.transferDone({ moving: false, n: 2 }, 2, 0, 0, false),
          "Copied 2 items · z undoes")
    check("a finished move says moved",
          Ops.transferDone({ moving: true, n: 1 }, 1, 0, 0, false),
          "Moved 1 item · z undoes")
    check("a partial failure reports both halves and is still undoable",
          Ops.transferDone({ moving: false, n: 2 }, 1, 1, 0, false),
          "Copied 1 of 2 · 1 failed · z undoes")
    // Nothing landed, so there is nothing for z to reverse and the line does not offer it.
    check("a transfer where every item failed does not offer an undo",
          Ops.transferDone({ moving: false, n: 2 }, 0, 2, 0, false),
          "Copied 0 of 2 · 2 failed")
    check("a cancel that copied nothing reports skipped sources",
          Ops.transferDone({ moving: false, n: 2 }, 0, 0, 2, true),
          "Copied 0 of 2 · 2 skipped · cancelled")
    check("a cancel that copied something still offers the undo",
          Ops.transferDone({ moving: false, n: 5 }, 3, 0, 2, true),
          "Copied 3 of 5 · 2 skipped · cancelled · z undoes")
    check("mixed outcomes retain the board's complete count inventory",
          Ops.transferDone({ moving: false, n: 5 }, 2, 1, 2, false),
          "Copied 2 of 5 · 1 failed · 2 skipped · z undoes")
    check("a named failure retains the backend cause",
          Ops.transferFailure({ moving: true }, "photo.heic", "disk full"),
          "Move failed: photo.heic · disk full")
    check("no located retry match makes no selection claim", Ops.retrySelectionLine([]), "")
    check("one verified retry match uses the board's filename wording",
          Ops.retrySelectionLine([{path: "/source/c.txt", index: 2}]), "c.txt selected for retry")
    check("multiple verified retry matches report their actual selected count",
          Ops.retrySelectionLine([{path: "/source/c.txt", index: 2}, {path: "/source/d.txt", index: 3}]),
          "2 items selected for retry")

    check("undo names the operation it reversed",
          Ops.undone("rename") + " / " + Ops.undone("move"),
          "Undid the rename. / Undid the move.")
    // src/backend/undo.rs reverses a mkdir with remove_dir, so the line says what left the disk
    // rather than repeating the wire's own verb at an operator who never typed it.
    check("undoing a new folder says what came off the disk",
          Ops.undone("mkdir"),
          "Removed the new folder.")

    check("the clipboard says what it took and how to use it",
          Ops.copied(2, true) + " / " + Ops.copied(1, false),
          "Cut 2 items · p pastes / Copied 1 item · p pastes")

    check("a leaf is the part after the last separator",
          Ops.leaf("/home/gm/photo copy.jpg"),
          "photo copy.jpg")
    check("a name with no separator is already its own leaf",
          Ops.leaf("bare.txt"),
          "bare.txt")

    var fresh = Ops.emptyClipboard()
    check("a fresh clipboard holds nothing and is not a move",
          fresh.paths.length + "/" + fresh.moving,
          "0/false")
    // A selection can be wider than the window the client holds: Ctrl-a takes root.total, while
    // Pane.rowFor answers null outside [held, held + rows.length). A stub pane holding five rows of
    // the forty selected is that shape, and it is what silently truncated a move to Dropbox.
    function windowedPane(sent) {
        var rows = []
        for (var w = 0; w < 5; w++) {
            rows.push({ n: "f" + w })
        }
        var picked = []
        for (var s = 0; s < 40; s++) {
            picked.push(s)
        }
        // What the collision card is asked, kept apart from sent, so a transfer is seen to ask rather than go straight out.
        var asked = []
        return {
            path: "/d",
            cursorIndex: 0,
            rows: rows, shown: null, asked: asked,
            selectedIndices: function () { return picked },
            rowFor: function (i) { return (i < 0 || i >= rows.length) ? null : rows[i] },
            join: function (a, b) { return a + "/" + b },
            sticky: function () {}, collide: { ask: function (msg) { asked.push(msg); return true } },
            backend: {
                heldListing: 1,
                send: function (msg) { sent.push(msg) },
                askPaths: function (rows) { sent.push({ c: "paths", rows: rows }) },
                mkdir: function (path) { sent.push({ c: "mkdir", path: path }) },
                compress: function (paths, dest, format, menuId) {
                    sent.push({ c: "archive", paths: paths, dest: dest, format: format, menuId: menuId })
                }
            },
            message: function () {},
            clipPending: null,
            clipQueue: [], clipSequence: 0,
            clipboardState: Clipboard.state(), clipboardWatchFailed: false, listInFlight: false,
            pathsPending: null,
            clipboard: null
        }
    }

    // targetPaths is where they go: it pushes only when rowFor answered, so thirty-five indices
    // leave no path and no trace. Asserted so the shape cannot come back quietly.
    var dropped = windowedPane([])
    check("targetPaths keeps only the rows the window happens to hold",
          Ops.targetPaths(dropped, dropped.selectedIndices()).length + " of 40",
          "5 of 40")

    // So a move must hand the backend INDICES. transfer takes a rows array, protocol.md says so in
    // bold and run.rs resolves it, and sending paths instead relocates five files and abandons
    // thirty-five with no error, no count and no message.
    var sentMove = []
    var dropboxPane = windowedPane(sentMove)
    Ops.moveToDropbox(dropboxPane, "/dropbox")
    var move = dropboxPane.asked.length === 1 ? dropboxPane.asked[0] : null
    check("a move to Dropbox carries every selected row, not the five the window held",
          move && move.rows ? String(move.rows.length)
                            : "truncated to " + (move && move.paths ? move.paths.length : 0),
          "40")
    check("and it asks the collision card rather than going straight to the backend", sentMove.length, 0)

    // Compress has the same defect through the same function, and the archive request has no rows
    // form, so it must resolve the indices first rather than name the handful it can see.
    var sentZip = []
    Ops.compress(windowedPane(sentZip), "zip")
    var zip = sentZip.length === 1 ? sentZip[0] : null
    check("compress asks for every selected row to be resolved, it does not name the five it holds",
          zip && zip.c === "paths" ? String(zip.rows.length)
                                   : "built an archive of " + (zip && zip.paths ? zip.paths.length : 0),
          "40")

    // The condition on opening Pane.qml: one paths reply now has two possible askers, and the
    // clipboard is the one that already worked. An unclaimed reply must still land where it always did.
    var clipPane = windowedPane([])
    Ops.clip(clipPane, true)
    Ops.pathsResolved(clipPane, ["/d/a", "/d/b"])
    check("a paths reply with nothing pending still reaches the clipboard",
          clipPane.clipboard ? clipPane.clipboard.paths.length + "/" + clipPane.clipboard.moving : "lost",
          "2/true")

    // And a reply the compress asked for builds the archive from the whole resolved list.
    var sentResolved = []
    var zipPane = windowedPane(sentResolved)
    zipPane.pathsPending = { kind: "compress", format: "zip" }
    var whole = []
    for (var z = 0; z < 40; z++) {
        whole.push("/d/f" + z)
    }
    Ops.pathsResolved(zipPane, whole)
    var built = sentResolved.length === 1 ? sentResolved[0] : null
    check("the archive is built from every resolved path, not from the window",
          built && built.c === "archive" ? String(built.paths.length) : "no archive request",
          "40")
    check("and the reply is consumed, so a later clipboard answer is not stolen by it",
          String(zipPane.pathsPending),
          "null")

    // A paths round trip in flight refuses every other asker out loud, so none steals another's reply.
    function pathsPane(sent) {
        var p = windowedPane(sent)
        p.said = []
        p.message = function (text, failed) { p.said.push([text, failed]) }
        p.opener = { copied: [], copyText: function (text) { this.copied.push(text) } }
        p.openPermissionsWith = function (list) { p.permitted = list }
        return p
    }
    var copySent = []
    var copyPane = pathsPane(copySent)
    Ops.copyAs(copyPane, "path", ["/d/a", "/d/b"])
    check("Copy as with paths copies at once in the asked format",
          copyPane.opener.copied.join(";") + "|" + copySent.length + "|" + String(copyPane.pathsPending),
          "/d/a\n/d/b|0|null")
    var relPane = pathsPane([])
    Ops.copyAs(relPane, "relative", ["/d/a", "/d/sub/b"])
    check("Copy as relative names each file from the listed directory",
          relPane.opener.copied.join(";"), "a\nsub/b")
    // Recent stands on "/", so a relative path there would read as relative to the terminal.
    var recentSent = []
    var recentPane = pathsPane(recentSent)
    recentPane.path = "/"
    recentPane.recentMode = "recent"
    Ops.copyAs(recentPane, "relative", ["/home/gm/a.txt"])
    Ops.copyAs(recentPane, "relative")
    check("Recent refuses Relative path with or without paths, copying and asking nothing",
          recentPane.opener.copied.length + "|" + recentSent.length + "|" + String(recentPane.pathsPending),
          "0|0|null")
    check("and says why, twice, as information",
          JSON.stringify(recentPane.said), JSON.stringify([[Ops.RECENT_RELATIVE, false], [Ops.RECENT_RELATIVE, false]]))
    Ops.copyAs(recentPane, "path", ["/home/gm/a.txt"])
    check("Recent still copies the full Path", recentPane.opener.copied.join(";"), "/home/gm/a.txt")
    var askSent = []
    var askPane = pathsPane(askSent)
    Ops.copyAs(askPane, "quoted")
    check("Copy as without paths asks under a copyAs claim",
          askSent.length + "|" + askPane.pathsPending.kind + "|" + askPane.pathsPending.format,
          "1|copyAs|quoted")
    var heldSent = []
    var heldPane = pathsPane(heldSent)
    heldPane.pathsPending = { kind: "drag" }
    Ops.copyAs(heldPane, "path")
    check("Copy as refuses while a drag claim is waiting",
          heldSent.length + "|" + heldPane.pathsPending.kind + "|" + JSON.stringify(heldPane.said),
          "0|drag|[[\"Still resolving the last selection; try again.\",false]]")
    MenuHunt.clipboard(check, pathsPane)
    var zipSent = []
    var zipBusy = pathsPane(zipSent)
    zipBusy.clipPending = { sequence: 1, moving: true }
    Ops.compress(zipBusy, "zip")
    check("a compress refuses out loud while a cut is waiting",
          zipSent.length + "|" + String(zipBusy.pathsPending) + "|" + JSON.stringify(zipBusy.said),
          "0|null|[[\"Still resolving the last selection; try again.\",false]]")
    var answeredPane = pathsPane([])
    answeredPane.pathsPending = { kind: "copyAs", format: "quoted" }
    Ops.pathsResolved(answeredPane, ["/d/a b"])
    check("a copyAs reply copies the resolved list in the claimed format",
          answeredPane.opener.copied.join(";") + "|" + String(answeredPane.pathsPending),
          "'/d/a b'|null")
    var relAnswer = pathsPane([])
    relAnswer.pathsPending = { kind: "copyAs", format: "relative", base: "/d" }
    Ops.pathsResolved(relAnswer, ["/d/a", "/d/sub/b"])
    check("a relative copyAs reply is from the stored listing",
          relAnswer.opener.copied.join(";"), "a\nsub/b")
    var allowedPane = pathsPane([])
    allowedPane.pathsPending = { kind: "permissions" }
    Ops.pathsResolved(allowedPane, ["/d/x"])
    check("a permissions reply opens the dialog over the resolved list",
          (allowedPane.permitted || []).join(",") + "|" + String(allowedPane.pathsPending),
          "/d/x|null")

    var captured = ["/d/captured.txt", "/d/second.txt"]
    var capturedRequests = []
    var capturedPane = windowedPane(capturedRequests)
    Ops.clip(capturedPane, true, captured)
    check("a menu clipboard action retains captured paths without resolving the current listing",
          capturedPane.clipboard.paths.join(",") + "|" + capturedPane.clipboard.moving + "|" + capturedRequests.length,
          "/d/captured.txt,/d/second.txt|true|1")
    check("captured clipboard paths reach the system without another row lookup",
          capturedRequests[0].c + "|" + capturedRequests[0].paths.join(","),
          "clipSet|/d/captured.txt,/d/second.txt")
    Ops.compressResolved(capturedPane, captured, "zip", 31)
    check("compression carries the captured paths and menu identity into the worker",
          capturedRequests[1].paths.join(",") + "|" + capturedRequests[1].menuId,
          "/d/captured.txt,/d/second.txt|31")
    Ops.moveToDropbox(capturedPane, "/dropbox", 32)
    check("Dropbox transfer retains the menu identity alongside the full selection",
          capturedPane.asked.length === 1 ? capturedPane.asked[0].menuId + "|" + capturedPane.asked[0].rows.length + "|" + capturedRequests.length
                                          : "not asked", "32|40|2")

    var t = Ops.started(12, true, 3)
    check("a started transfer carries its id, its direction and its count",
          t.id + "/" + t.moving + "/" + t.n + "/" + t.running,
          "12/true/3/true")
    var redo = Object.assign({}, t, { redo: "move" })
    var redoSample = Transfer.sampled(redo, 1, "photo.heic", 50, 100)
    var redoDone = Transfer.itemDone(redoSample, 1, "photo.heic")
    check("redo progress keeps its operation identity through samples and completions",
          Transfer.head(redoSample) + "/" + redoDone.redo + "/" + redoDone.done,
          "Redoing move 2 of 3/move/2")
    check("progress creates a new sample without changing the prior state", redo.index, 0)

    // Rename keeps its captured source and editor until the backend accepts the write.
    function renamePane(cursor, renaming, renamed, viewMode) {
        var p = windowedPane([])
        p.cursorIndex = cursor
        p.renamingIndex = renaming
        p.renameRequest = null
        Object.defineProperty(p, "renamePending", {get: function() { return p.renameRequest !== null }})
        p.renameError = ""
        p.setCursor = function(index) { p.cursorIndex = index }
        p.viewMode = viewMode ? viewMode : "list"
        p.said = ""
        p.message = function (text) { p.said = text }
        p.backend.rename = function (from, to) { renamed.push(from + ">" + to) }
        return p
    }
    var held = renamePane(2, -1, [])
    Ops.startRename(held)
    check("r on a held row opens the editor over it", held.renamingIndex, 2)
    var unheld = renamePane(9, -1, [])
    Ops.startRename(unheld)
    check("r on a row outside the held window opens nothing", unheld.renamingIndex, -1)
    var gridded = renamePane(2, -1, [], "grid")
    Ops.startRename(gridded)
    check("r in the grid opens the same editor", gridded.renamingIndex, 2)
    // The columns view draws the editor over its active column, see ui/ColumnPane.qml's own corner.
    var columned = renamePane(2, -1, [], "columns")
    Ops.startRename(columned)
    check("r in the columns view opens the same editor", columned.renamingIndex + "|" + columned.said, "2|")
    var renamed = []
    var committing = renamePane(0, 3, renamed)
    Ops.commitRename(committing, "g3")
    check("a commit retains the editor while the write is pending",
          renamed.join(",") + "|" + committing.renamingIndex + "|" + committing.renamePending, "/d/f3>g3|3|true")
    Ops.commitRename(committing, "again")
    check("a repeated commit sends no second write", renamed.length, 1)
    var stale = []
    var gone = renamePane(0, 7, stale)
    Ops.commitRename(gone, "x")
    check("a missing source cannot send a write or discard the draft",
          stale.length + "|" + gone.renamingIndex + "|" + gone.renameError, "0|7|Item changed; reopen Rename.")
    for (var invalid of ["", ".", "..", "a/b", "a\u0000b"]) {
        var refusedWrites = [], refused = renamePane(2, 2, refusedWrites)
        Ops.commitRename(refused, invalid)
        check("invalid basename retains editor and explains refusal: " + JSON.stringify(invalid),
              refusedWrites.length + "|" + refused.renamingIndex + "|" + refused.renamePending + "|" + (refused.renameError.length > 0), "0|2|false|true")
    }
    var menuRename = renamePane(2, -1, [])
    var renameIdentity = 0
    menuRename.backend.rename = function (from, to, menuId) { renameIdentity = menuId }
    Ops.startRename(menuRename, 33)
    check("menu rename retains its identity for the editor lifetime", menuRename.renameMenuId, 33)
    Ops.commitRename(menuRename, "renamed.txt")
    check("committing retains the captured menu identity", renameIdentity, 33)

    // Issue #170: a ready snapshot over the unchanged identity opens now, one in flight takes F2, a moved identity refuses.
    check("the menu rename decision lives in Ops.js", typeof Ops.menuRenameRoute, "function")
    if (typeof Ops.menuRenameRoute === "function") {
        check("a ready snapshot over the current selection opens at once",
              Ops.menuRenameRoute(true, "sel-3", "sel-3"), "now")
        check("a snapshot still in flight over it takes the F2 route",
              Ops.menuRenameRoute(false, "sel-3", "sel-3"), "f2")
        check("a ready snapshot over a moved selection refuses",
              Ops.menuRenameRoute(true, "sel-3", "sel-4"), "stale")
        check("a snapshot still in flight over a moved selection refuses too",
              Ops.menuRenameRoute(false, "sel-3", "sel-4"), "stale")
        check("an empty identity never opens and never takes the F2 route",
              Ops.menuRenameRoute(true, "", ""), "stale")
    }

    // Grid closing review G1: with the tile's Loader retired and the rename
    // refused, no live editor exists to show it, so the refusal goes to the
    // status bar as an error and the edit closes; otherwise the editor shows it.
    check("the refusal route lives in Ops.js", typeof Ops.refuseRename, "function")
    if (typeof Ops.refuseRename === "function") {
        function refusalPane(editor) {
            return { renamingIndex: 3, renameError: "",
                     renameEditor: function () { return editor },
                     said: [], message: function (text, isError) { this.said.push([text, isError]) } }
        }
        var live = refusalPane({})
        check("a live editor shows the refusal itself",
              Ops.refuseRename(live, "b-existing.md already exists.") + "|" + live.renameError + "|" + live.said.length + "|" + live.renamingIndex,
              "editor|b-existing.md already exists.|0|3")
        var retired = refusalPane(null)
        check("no live editor sends the same sentence to the status bar and closes the edit",
              Ops.refuseRename(retired, "b-existing.md already exists.") + "|" + retired.renameError + "|" + JSON.stringify(retired.said) + "|" + retired.renamingIndex,
              "status||[[\"b-existing.md already exists.\",true]]|-1")
        var settled = refusalPane(null)
        settled.renamingIndex = -1
        check("a settled edit still says the refusal once",
              Ops.refuseRename(settled, "gone.") + "|" + JSON.stringify(settled.said),
              "status|[[\"gone.\",true]]")
    }

    var operationIds = []
    var convertedArguments = null
    var menuPane = windowedPane([])
    menuPane.backend.duplicate = function (path, id) { operationIds.push("duplicate:" + id) }
    menuPane.backend.trash = function (rows, id) { operationIds.push("trash:" + id) }
    menuPane.backend.extract = function (path, dest, id) { operationIds.push("extract:" + id) }
    menuPane.backend.convertImage = function (path, dest, strip, id, requestId) {
        operationIds.push("convert:" + id)
        convertedArguments = {path: path, dest: dest, requestId: requestId}
    }
    menuPane.convertRequested = function () {}
    Ops.duplicate(menuPane, 34)
    Ops.trash(menuPane, 35)
    Ops.extract(menuPane, 36)
    Ops.openConvert(menuPane, 37)
    var conversion = menuPane.convertSource
    var convertedSource = conversion.path
    menuPane.path = "/different-directory"
    Ops.convert(menuPane, conversion, "png", false, 73)
    check("menu mutations forward the selected identity to their operation workers",
          operationIds.join(","), "duplicate:34,trash:35,extract:36,convert:37")
    check("conversion sends its original source after navigation", convertedArguments.path, convertedSource)
    check("conversion sends its captured output directory", convertedArguments.dest.indexOf("/different-directory/"), -1)
    check("conversion keeps its menu identity for the complete dialog lifetime", menuPane.convertSource.menuId, 37)
    check("conversion retains the caller's operation identity", menuPane.convertSource.requestId, 73)

    // ---- the new folder, the one operation whose name the backend chooses ----

    // No name field at all: src/backend/ops.rs takes the first free "New Folder" when one is
    // omitted, which is what spares this UI a retry loop and any collision handling of its own.
    var madeSent = []
    Ops.newFolder(windowedPane(madeSent))
    var mk = madeSent.length === 1 ? madeSent[0] : null
    check("a new folder names the parent and leaves the name to the backend",
          mk ? mk.c + " " + mk.path + " " + (mk.name === undefined) : "nothing was sent",
          "mkdir /d true")
    check("a created folder names its own reversal, the way the transfer and trash lines do",
          Ops.made("/d/New Folder"),
          "Created New Folder \u00b7 z undoes")

    // ---- the transfer card, ui/TransferCard.qml's own model ----

    // The operator's design artifact draws the count with no name in it, because the file under way
    // gets a row of its own underneath.
    check("the card headline is the count alone",
          Transfer.head({ moving: false, n: 23, index: 8 }),
          "Copying 9 of 23")
    check("and a move headline says moving",
          Transfer.head({ moving: true, n: 23, index: 8 }),
          "Moving 9 of 23")
    // The status bar's own line is built from the same headline, so the two cannot drift apart.
    check("the status line is that headline plus the name",
          Ops.progressLine({ moving: false, n: 23, index: 8, name: "panel-demo.mp4" }),
          "Copying 9 of 23 · panel-demo.mp4")

    check("the card names the file under way and how big it is",
          Transfer.fileLine({ name: "panel-demo.mp4", total: 48000000 }),
          "panel-demo.mp4 \u00b7 48.0 MB")
    // total is 0 for a directory, whose size is not known without a sweep this codebase never does.
    check("a directory with nothing copied yet claims no size", Transfer.fileLine({ name: "photos", total: 0, bytes: 0 }), "photos")
    // A tree's own running count is the byte line's under the bar, so the file line never says it twice.
    check("a directory under way still claims no size", Transfer.fileLine({ name: "photos", total: 0, bytes: 1500000000 }), "photos")
    check("nothing in flight yet draws no second row at all",
          Transfer.fileLine({ name: "", total: 0, bytes: 0 }),
          "")

    // The bar is the whole transfer, never the one file: one large file is then its own byte bar.
    check("one file half copied fills half the bar",
          Transfer.fraction({ n: 1, done: 0, bytes: 24000000, total: 48000000 }),
          0.5)
    // And thirty thousand small ones step it once each instead of restarting it thirty thousand times.
    check("eight of twenty-three finished sits eight steps along",
          Transfer.fraction({ n: 23, done: 8, bytes: 0, total: 0 }),
          8 / 23)
    check("a byte sample fills in only the item in flight",
          Transfer.fraction({ n: 23, done: 8, bytes: 24000000, total: 48000000 }),
          8.5 / 23)
    check("a transfer of nothing reports no progress rather than dividing by zero",
          Transfer.fraction({ n: 0, done: 0, bytes: 0, total: 0 }),
          0)
    // A count that overran its own promise must not draw past the end of the bar.
    check("the bar cannot overfill",
          Transfer.fraction({ n: 1, done: 0, bytes: 99, total: 10 }),
          1)

    // The fold the wire drives: a progress line fills in the item in flight, an item line ends it.
    var flight = Transfer.sampled(Ops.started(12, false, 23), 8, "panel-demo.mp4", 24000000, 48000000)
    check("a sample leaves the finished count where it was",
          flight.done + " " + flight.index + " " + flight.name,
          "8 8 panel-demo.mp4")
    check("and it keeps the id the cancel button has to name",
          flight.id + " " + flight.running,
          "12 true")
    var landed = Transfer.itemDone(flight, 8, "panel-demo.mp4")
    check("an item's own terminal line counts it whole and spends its byte sample",
          landed.done + " " + landed.bytes + " " + landed.total,
          "9 0 0")
    check("an idle transfer is not running and has nothing to draw",
          Ops.emptyTransfer().running + " " + Ops.emptyTransfer().done + " " + Ops.emptyTransfer().total,
          "false 0 0")

    // CloudMounts: a copy onto an rclone mount lands in rclone's cache first, so the done
    // line names the background upload and never claims the drive confirmed it.
    check("a copy onto rclone says it uploads in the background",
          Ops.transferDone({ moving: false, n: 5 }, 5, 0, 0, false, false, "rclone uploads them in the background"),
          "Copied 5 items · rclone uploads them in the background · z undoes")
    check("an unconfirmed folder says so the same way",
          Ops.transferDone({ moving: false, n: 2 }, 2, 0, 0, false, false, "copied, but the drive did not confirm the folder"),
          "Copied 2 items · copied, but the drive did not confirm the folder · z undoes")
    check("a verdict note with nothing landed stays unsaid",
          Ops.transferDone({ moving: false, n: 2 }, 0, 2, 0, false, false, "rclone uploads them in the background"),
          "Copied 0 of 2 · 2 failed")
    check("no verdict note leaves the shipped line alone",
          Ops.transferDone({ moving: false, n: 2 }, 2, 0, 0, false, false, ""),
          "Copied 2 items · z undoes")

    // A slow click renames with context 0 so the list stays still; F2 and r keep the three-row context.
    function contextPane() {
        return { cursorIndex: 5, path: "/d", renamingIndex: -1, renamePending: false,
            renameError: "", renameSource: "", renameMenuId: 0,
            rowFor: function () { return { n: "f.txt" } },
            join: function (base, name) { return base + "/" + name },
            message: function () {},
            seen: "unset",
            setCursor: function (index, context) { this.seen = context } }
    }
    var pointerRename = contextPane()
    Ops.startRename(pointerRename, 0, 5, 0)
    check("a pointer rename sets the cursor with context 0", pointerRename.seen, 0)
    var keyboardRename = contextPane()
    Ops.startRename(keyboardRename, 0, 5)
    check("a keyboard rename keeps no pointer context", keyboardRename.seen, undefined)
    // A stranded replace rides ahead of the undo hint, and silence stays silent.
    check("a stranded link is named on the linked line",
          Ops.linkedLine(1, 1, 0, "the link left at /d/b.txt could not be removed (stale); the replaced item stays in the trash"),
          "Linked 1 item · 1 failed · the link left at /d/b.txt could not be removed (stale); the replaced item stays in the trash · z undoes")
    check("no stranded note leaves the shipped line alone",
          Ops.linkedLine(2, 0, 0, ""),
          "Linked 2 items · z undoes")
}
