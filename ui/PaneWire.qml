import QtQuick
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Errors.js" as Errors
import "js/Nav.js" as Nav
import "js/Ops.js" as Ops
import "js/Search.js" as Search
import "js/Tabs.js" as Tabs
import "js/Thumbs.js" as Thumbs
import "js/Transfer.js" as Transfer

// Every reply from outside the window lands here: the backend's, and those of the three foreign
// programs the pane runs (the opener, the Dropbox share link, Taildrop). Split out of ui/Pane.qml
// the way ui/List.qml was; it owns no state of its own and writes only through the pane handed in.
Item {
    id: root

    property var pane: null
    // The new folder has no row until the refresh lands, so the editor is opened on the rows reply
    // that carries it rather than on the made line that asked for it. Holds that folder's full path.
    property string renameOnArrival: ""
    // ui/Pane.qml reaches the three through these: openCursor takes the opener, the menu reads the
    // Taildrop peers, and the two share actions call the other two.
    readonly property alias opener: opener
    readonly property alias shareLink: shareLink
    readonly property alias taildrop: taildrop

    Flea.Opener {
        id: opener
        onFailed: function (path) { pane.message("That file could not be opened; check that it still exists.", true) }
        onIsDirectory: function (path) { pane.open(path) }
    }

    Flea.ShareLink {
        id: shareLink
        onCopied: pane.message("Share link copied to the clipboard.", false)
        onFailed: pane.message("Dropbox could not make a share link for that file.", true)
    }

    Flea.Taildrop {
        id: taildrop
        // Fetched once per session start, not per right click: peers change on the scale of
        // minutes, not the scale of opening a context menu, and refreshing on open would make
        // the menu's own height (and the clamp openAt applies) depend on an async reply.
        Component.onCompleted: refresh()
    }

    // Only when the cursor really landed on the folder that was made: on a listing wider than the
    // window the refresh may not hold that row at all, and ui/RenameField.qml lives in ui/Row.qml
    // alone, so the other two views would arm an editor nothing draws and never disarm it.
    function openRenameOnArrival() {
        if (root.renameOnArrival.length === 0)
            return
        var target = root.renameOnArrival
        root.renameOnArrival = ""
        var row = pane.rowFor(pane.cursorIndex)
        if (pane.viewMode === "list" && row && pane.join(pane.path, row.n) === target)
            pane.renamingIndex = pane.cursorIndex
    }

    Connections {
        target: pane.backend

        function onListed(total, readMs, sortMs) {
            if (pane.listInFlight) {
                pane.listedSeen = true
            }
            pane.total = total
            // A search's opening listed line is the walk starting, not a directory that came back empty.
            if (pane.searchMode === Search.RESULTS) {
                pane.listingState = Search.listingState(pane, total)
                pane.stateMessage = ""
                return
            }
            pane.listingState = total === 0 ? "empty" : "ready"
            pane.stateMessage = total === 0
                    ? "This directory is empty; add a file to see it here."
                    : ""
            pane.opened(pane.path)
        }

        function onRows(start, items, ms, kinds) {
            if (pane.listInFlight && !pane.listedSeen) {
                return
            }
            pane.held = start
            pane.rows = items
            pane.kindNames = kinds
            if (pane.rowsAt === 0 && pane.inputAt > 0 && pane.rowFor(pane.cursorIndex))
                pane.rowsAt = Date.now()
            pane.applyPendingSelect()
            Tabs.applyPending(pane)
            root.openRenameOnArrival()
            pane.listArea.restartSettle()
            if (pane.listInFlight) {
                pane.listInFlight = false
                pane.listedSeen = false
            }
        }

        // Sample input: {"t":"searching","n":812,"scanned":41200,"ms":300.114}
        function onSearching(total, scanned, ms) {
            if (pane.searchMode !== Search.RESULTS) {
                return
            }
            pane.total = total
            pane.searchScanned = scanned
            pane.searchMs = ms
            pane.listingState = Search.listingState(pane, total)
            // Unlike list, a search rides no first screenful along: the count grows, so the window is asked for as it does.
            pane.listArea.restartCoalesce()
            pane.listArea.restartSettle()
        }

        // Sample input: {"t":"searched","n":14673,"scanned":284446,"ms":229.008,"cancelled":false}
        function onSearched(total, scanned, ms, cancelled) {
            if (pane.searchMode !== Search.RESULTS) {
                return
            }
            pane.searchRunning = false
            pane.searchCancelled = cancelled
            pane.total = total
            pane.searchScanned = scanned
            pane.searchMs = ms
            pane.listingState = Search.listingState(pane, total)
            // The rows were ranked immediately before this line, so the window on screen is in
            // discovery order and every index in it now names another file. A coalesce would not
            // fix that: it asks for a window only on drift, and a full held one drifts on neither edge.
            Search.ranked(pane)
            pane.listArea.restartSettle()
        }

        // A thumbed line for the previous listing is still in the pipe when open() clears the map.
        function onThumbed(row, file) {
            if (!pane.listInFlight)
                pane.thumbState = Thumbs.remember(pane.thumbState, row, file, pane.thumbCap)
        }

        // A dirsized line for the previous listing is still in the pipe when open() clears the map.
        function onDirSized(row, bytes, partial) {
            if (!pane.listInFlight)
                pane.dirSizeState = DirSizes.remember(pane.dirSizeState, row, bytes, partial, pane.thumbCap)
        }

        // Sample input: {"t":"transferstarted","id":12,"n":2,"moving":true}
        // The verb comes off the wire, never off the clipboard: paste spends a cut before this line
        // arrives, and a Dropbox move never touches the clipboard at all.
        function onTransferStarted(id, n, moving) {
            pane.transfer = Ops.started(id, moving, n, pane.pendingTransferKind)
            pane.pendingTransferKind = "local"
            pane.sticky(Ops.progressLine(pane.transfer))
        }

        // Sample input: {"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000}
        function onTransferProgress(id, index, name, bytes, total) {
            if (id !== pane.transfer.id) {
                return
            }
            // Reassigned rather than mutated in place: an in-place write re-evaluates no binding,
            // so the card would never see a sample. The bytes and the total ride along with it.
            pane.transfer = Transfer.sampled(pane.transfer, index, name, bytes, total)
            pane.sticky(Ops.progressLine(pane.transfer))
        }

        // The item's own terminal line: it advances the sticky count, and a failure is data, not a dialog.
        function onTransferItem(id, index, name, ok, err) {
            if (id !== pane.transfer.id) {
                return
            }
            pane.transfer = Transfer.itemDone(pane.transfer, index, name)
            pane.sticky(Ops.progressLine(pane.transfer))
        }

        // Sample input: {"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false}
        function onTransferDone(id, ok, failed, skipped, cancelled) {
            if (id !== pane.transfer.id) {
                return
            }
            var line = Ops.transferDone(pane.transfer, ok, failed, cancelled)
            pane.transfer = Ops.emptyTransfer()
            pane.sticky("")
            pane.message(line, failed > 0 && ok === 0)
            pane.refresh("")
        }

        function onTrashed(ok, failed) {
            pane.sticky("")
            pane.message(Ops.trashed(ok, failed), ok === 0)
            pane.clearSelection()
            pane.refresh("")
        }

        // The listing is re-read with the new name selected, so the row the operator was on stays
        // under the cursor; a rename the pointer committed keeps the pointer's own row instead.
        function onRenamed(ok, path) {
            pane.refresh(Nav.renameRefreshTarget(pane, path))
        }

        // Sample input: {"t":"made","ok":true,"path":"/home/gm/Pictures/New Folder"}
        // The same refresh onRenamed does, which is also what puts the order back to name ascending
        // and drops any filter, so the new row is never sorted or filtered out of sight.
        function onMade(ok, path) {
            root.renameOnArrival = path
            pane.message(Ops.made(path), false)
            pane.refresh(path)
        }

        function onDuplicated(ok, path) {
            pane.message("Duplicated to " + Ops.leaf(path) + Ops.UNDO_HINT, false)
            pane.refresh(path)
        }

        function onUndone(op, ok) {
            pane.sticky("")
            pane.message(Ops.undone(op), false)
            pane.refresh("")
        }

        // A success nobody could check must not read as one that was checked, so the unverified
        // extract says so in the same slot rather than in a dialog.
        function onArchiveDone(id, ok, verified, err) {
            pane.sticky("")
            pane.message(ok ? Ops.archiveDoneLine(verified) : Errors.sentence("archive", err), !ok)
            pane.refresh("")
        }

        function onConvertDone(id, ok, path, err) {
            pane.sticky("")
            pane.message(ok ? "Converted to " + Ops.leaf(path) + "." : Errors.sentence("convert", err), !ok)
            pane.refresh(ok ? path : "")
        }

        // One statfs per directory, so the status bar's right half is refreshed by navigation alone.
        function onFsInfo(fs, free) {
            pane.fsName = fs
            pane.fsFree = free
        }

        // The answer to Ops.clip's askPaths; nothing reaches the clipboard until this lands.
        function onPaths(list) {
            Ops.pathsResolved(pane, list)
        }

        function onFailed(where, input, message, mode) {
            Ops.clearPendingKind(pane, where)
            var text = Errors.sentence(where, message)
            // A refused sort changes nothing in the backend, so it changes nothing here: a notice in the
            // plain role, never the error role, which is for a listing that stopped being true.
            if (where === "sort") {
                pane.message(text, false)
                return
            }
            pane.listInFlight = false
            pane.listedSeen = false
            // Neither the child nor its stream comes back, so the listing it produced stops being true.
            var terminal = where === "backend" || where === "read"
            // Only these two mean the refresh will never deliver rows. An editor left armed past that
            // would open over whatever row the cursor happens to hold in some later listing.
            if (terminal || where === "scan")
                root.renameOnArrival = ""
            if (terminal) {
                pane.total = 0
                pane.held = 0
                pane.rows = []
                pane.cursorIndex = 0
                // No transferdone is coming from a backend that is gone, and nothing else ends a
                // running transfer, so the card would crawl over a dead child until the app closed.
                pane.transfer = Ops.emptyTransfer()
                pane.sticky("")
            }
            if (terminal || where === "scan" || pane.listingState === "loading") {
                pane.listingState = Errors.listingState(where, message)
                pane.lockedMode = mode
                pane.stateMessage = text
            }
            pane.message(text, true)
        }
    }

}
