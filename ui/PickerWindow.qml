// The pragmas that name this window's app id and shell id live in ui/boot/picker.qml, which is
// the Quickshell entry; this document is loaded from there by file: URL so that Qt caches it.

import Quickshell
import Quickshell.Io
import QtQuick
import qs.Commons
import "." as Flea
import "js/Picker.js" as Picker
import "js/Keymap.js" as Keymap
import "js/Thumbs.js" as Thumbs

// One portal request, one window: the org.freedesktop.impl.portal.FileChooser dialog every caller on
// the box gets, opened by flea --pick and answered through the reply file tools/flea-portal reads.
// The same Backend, Row, Theme and places the browser window draws with, and none of its operations:
// a chooser that can rename or delete is a file manager wearing a dialog's clothes.
ShellRoot {
    property alias pickerWin: win
    FloatingWindow {
        id: win

        readonly property var req: Picker.request(Quickshell.env("FLEA_PICKER"))
        readonly property string home: Quickshell.env("HOME")

        title: Picker.title(win.req)
        implicitWidth: Math.round(Theme.space(640) * Theme.dialogWidthRatio)
        implicitHeight: Theme.space(440)
        color: Theme.color.background

        // Where the list is standing, what it holds of the listing, and where the cursor is in it.
        property string path: ""
        property int total: 0
        property int held: 0
        property var rows: []
        // The per-response kind dictionary ui/Row.qml's Kind column indexes into.
        property var kindNames: []
        property int cursorIndex: 0
        property string listingState: "loading"
        // A refused scan leaves the worker holding the previous folder, which "empty" alone cannot say, so only a new listing clears it.
        property bool listingFailed: false
        // The one statement of when the listing may be reordered; the header and the keys both read it.
        readonly property bool sortable: !win.backendUnavailable && !win.recent && !win.submitting && !win.listingFailed
            && win.listingState !== "loading" && !(shares.item && shares.item.active)
        // The order the listing is actually in, read through the picker because the backend id is out of scope.
        readonly property string sortBy: backend.sortBy
        readonly property bool sortDesc: backend.sortDesc
        property int pendingListings: 0
        property bool receivingLatestListing: false
        property bool backendUnavailable: false
        property string message: ""

        // The checked identities, each a path and its size, so Back and Parent cannot rebind one.
        property var marks: []
        // Which chip is active: an index into the caller's filters, or -1 for All files.
        property int filterIndex: Picker.currentChip(win.req)
        readonly property var filter: win.filterIndex >= 0 ? win.req.filters[win.filterIndex] : null
        readonly property var shown: null
        readonly property int shownTotal: win.total
        onFilterChanged: if (win.path.length) win.openWithoutHistory(win.path)

        // The view the listing draws in; a launch reads pickerView and only setView writes it.
        property string viewMode: "list"
        function setView(mode) {
            var next = Picker.viewSwitch(win.viewMode, mode)
            if (!next) { win.focusView(); return }
            win.viewMode = next
            // The user's own switch, and only that, is what the next launch reopens: a launch
            // reads, and cursor moves never owe the file anything.
            ViewState.changeKey("pickerView", next)
            // A reshow owns its window: the shown view moves to the cursor and refetches there.
            var at = Math.max(0, Math.min(win.cursorIndex, win.total - 1))
            if (next === "grid") grid.reshow(at)
            else list.reshow(at)
            win.focusView()
        }
        function viewItem() { return win.viewMode === "grid" ? grid : list }
        function focusView() { win.viewItem().forceActiveFocus() }
        // Whether the listing shows the directory's dotfiles. The worker never sends what a
        // request did not ask for, so the window re-reads the standing directory when this flips.
        property bool showHidden: false
        // The grid's visible-only thumbnail plan, owned here so both views share one map the way
        // the main pane owns its views' states; only the grid ever asks.
        property var thumbState: Thumbs.empty()
        // Seven screens of history at the picker's row height, the main pane's own bound.
        readonly property int thumbCap: 240
        // The directory's storage class beside its fsinfo line, never per row; unknown holds the
        // first settle the way the main pane holds its own, see ui/js/ExtThumbs.js.
        property string storageClass: ""
        property bool storageKnown: false

        // Where Back goes, and it only ever goes back: Parent is its own button and pushes here too.
        property var history: []
        // The save mode's own name, which starts as the caller's suggestion only when that
        // suggestion is a filename: tools/flea-portal passes current_name through verbatim, so a
        // separator in it would put a path outside this folder in the field before anyone typed.
        property string saveName: Picker.validName(win.req.name) ? win.req.name : ""
        onSaveNameChanged: win.invalidateSave()
        property int nextCheck: 0
        property int markRequest: 0
        property bool acceptMarks: false
        property bool marksDirty: false
        property int saveRequest: 0
        property int reviewRequest: 0
        property string probeKey: ""
        property var saveReview: ({})
        property string saveError: ""
        readonly property string saveKey: JSON.stringify([win.path, win.saveName])
        readonly property bool saveReady: win.saveReview.key === win.saveKey
        readonly property bool saveCollision: win.saveReady && win.saveReview.collision
        readonly property bool submitting: win.acceptMarks || win.reviewRequest > 0
        // Submission keeps Cancel reachable and restores its initiating control only on refusal.
        onSubmittingChanged: chrome.submissionChanged()
        readonly property bool marksAllowed: win.req.multiple && !win.saving
        readonly property var cursorRow: win.rowFor(win.cursorIndex)
        readonly property bool cursorFile: win.listingState === "ready" && !win.listingFailed && win.cursorRow !== null && !Picker.directory(win.cursorRow)
        readonly property bool canAccept: !win.backendUnavailable && !win.submitting && !win.markRequest && (win.saving
            ? win.saveReady : win.marks.length > 0 || (!win.folderMode && win.cursorFile) || (win.folderMode && !win.recent && win.listingState !== "loading"))

        // SendPicker.html draws every rule and control frame in one ink, a lift over whatever plane
        // it sits on. Theme.color.surface is a drop on these palettes and vanishes against the chrome
        // strips, so the picker takes the OEM's own resting border alpha, which lifts on both.
        readonly property color edge: Style.hoverBorderColor

        // Recent is a location and not a directory: the rail's own row opens it and the listing it
        // builds comes from the desktop's history rather than a scan; see AGENTS.md "Recent, and why".
        readonly property bool recent: Picker.isRecent(win.path)

        readonly property bool saving: win.req.mode === "save"
        readonly property bool folderMode: win.req.directory || win.req.mode === "savefiles"
        // Twice the wider view's screen plus slack, so the screen still fits after the quarter lead.
        readonly property int windowSize: Picker.windowSize(list.visibleRows, grid.visibleTileRows, grid.columns)
        // Both views refetch through one interval and lead, the main views' own 16 ms and quarter window.
        readonly property int coalesceMs: 16
        readonly property real windowLead: 0.25

        // Exactly one answer leaves this window, whichever way it is asked for.
        property bool answered: false

        function rowFor(index) {
            var at = index - win.held
            return at >= 0 && at < win.rows.length ? win.rows[at] : null
        }

        function open(next) {
            if (win.submitting || next === win.path) return
            if (win.path.length > 0) win.history = win.history.concat([win.path])
            win.openWithoutHistory(next)
        }

        // Issue 191: the location field, the browser's own path bar in the chooser. It navigates to
        // a typed, pasted or home-relative directory, the way ui/ChromeBar.qml does; a file path
        // lists nothing, exactly as the browser's does, so this stays navigation and adds no new
        // wire. ui/PickerChrome.qml draws the field over the path text while editingPath stands.
        property bool editingPath: false
        function startPathEdit() {
            if (win.submitting || win.backendUnavailable) return
            win.editingPath = true
        }
        function cancelPathEdit() { win.editingPath = false; list.forceActiveFocus() }
        function commitPath(typed, target) {
            win.editingPath = false
            list.forceActiveFocus()
            if (target.length > 0 && target !== win.path) win.open(target)
        }

        function openWithoutHistory(next) {
            if (win.backendUnavailable) return
            listing.clear()
            if (shares.item) shares.item.close()
            win.path = next
            win.listingFailed = false
            win.clearListing()
            win.invalidateSave()
            win.validateMarks(false)
            if (Picker.isRecent(next)) {
                recents.refresh()
                return
            }
            win.requestListing(backend.listRequest(next, win.windowSize, win.showHidden))
        }

        function requestListing(request) {
            win.pendingListings = 1
            listing.request(Object.assign(request, win.filterRequest()))
        }

        // Rows are named by index, so a replaced listing is dropped whole; emptying the model returns the viewport to the top.
        function clearListing() {
            selection.reset()
            win.total = 0
            win.held = 0
            win.rows = []
            win.cursorIndex = 0
            win.listingState = "loading"
            win.receivingLatestListing = false
            // A replaced listing renumbers every row, so no thumb answer may outlive it.
            win.thumbState = Thumbs.empty()
            win.storageClass = ""
            win.storageKnown = false
        }

        // Sort reorders the worker's filtered listing without re-reading the folder, so marks and save review stay; never written to ui.json.
        function requestSort(order) {
            if (!order || !win.sortable || (backend.sortBy === order.key && backend.sortDesc === order.desc))
                return
            backend.sortBy = order.key
            backend.sortDesc = order.desc
            win.clearListing()
            win.pendingListings = 1
            listing.sort(order.key, order.desc, ViewState.state.foldersFirst !== false, ViewState.state.groupByKind === true)
            // sort answers a listed line and no rows of its own, so the reordered window is asked for.
            listing.window(0, win.windowSize)
            win.focusView()
        }

        function filterRequest() {
            return {pickerGlobs: win.filter ? win.filter.globs : [], pickerMimes: win.filter ? win.filter.mimes : []}
        }

        function check(request) {
            request.c = "picker"
            request.id = ++win.nextCheck
            backend.send(request)
            return request.id
        }

        function validateMarks(accepting) { selection.validate(accepting) }

        function invalidateSave() {
            win.saveReview = ({})
            win.saveError = ""
            if (win.saving) Qt.callLater(win.probeSave)
        }

        function probeSave() {
            if (win.backendUnavailable || !win.saving || win.saveRequest || !win.path.length || !Picker.validName(win.saveName)) return
            win.probeKey = win.saveKey
            win.saveRequest = win.check({op: "save", folder: win.path, name: win.saveName})
        }

        // Reassign history so property var notifies.
        function goBack() {
            if (shares.item && shares.item.active) { shares.item.close(); return }
            if (win.submitting || win.history.length === 0)
                return
            var target = win.history[win.history.length - 1]
            win.history = win.history.slice(0, win.history.length - 1)
            win.openWithoutHistory(target)
        }

        function goUp() {
            // The board's own rule: Parent is unavailable in Recent, because a history has no parent.
            if (win.recent) {
                return
            }
            var up = Picker.parentOf(win.path)
            if (up !== win.path)
                win.open(up)
        }

        function toggleMark(index) { selection.toggle(index) }
        function markRange(was, index) { selection.range(was, index) }
        function selectAll() { selection.all() }
        function endRange() { selection.endRange() }

        // Enter walks folders and submits the checked files, or the cursor file when none are checked.
        function activate(index) {
            var row = win.rowFor(index)
            if (!row)
                return
            if (Picker.directory(row)) {
                win.open(Picker.rowPath(win.path, row.n))
                return
            }
            win.accept()
        }

        // File double clicks toggle marks in several-file requests; one-file requests mark when needed, then accept.
        function doubleActivate(index, rowPath, firstPath) {
            var row = win.rowFor(index)
            var choice = Picker.doubleAction(win.req, row, rowPath, firstPath, win.marks)
            if (choice === Picker.DOUBLE_OPEN) {
                win.open(Picker.rowPath(win.path, row.n))
                return
            }
            if (choice === Picker.DOUBLE_MARK) {
                win.toggleMark(index)
                return
            }
            if (choice !== Picker.DOUBLE_ACCEPT && choice !== Picker.DOUBLE_MARK_ACCEPT) {
                return
            }
            if (win.backendUnavailable || win.markRequest || win.submitting) {
                win.say("Selection is still being checked.")
                return
            }
            if (choice === Picker.DOUBLE_ACCEPT) {
                win.accept()
                return
            }
            win.acceptMarks = true
            win.markRequest = win.check({op: "mark", path: rowPath, directory: false, multiple: win.req.multiple})
        }

        function accept(reviewed) {
            if (win.backendUnavailable || win.submitting || win.markRequest) return
            if (win.saving) {
                if (win.saveName.length === 0) {
                    win.say("Name the file before saving it")
                    return
                }
                // The answer has to name the folder the user was shown, so a typed separator is
                // refused here rather than rewritten: a rewrite would send a path nobody approved.
                if (!Picker.validName(win.saveName)) {
                    win.say(Picker.NAME_REFUSED)
                    return
                }
                if (!win.saveReady) { win.probeSave(); return }
                if (win.saveCollision && reviewed !== true) { save.focusCancel(); return }
                win.reviewRequest = win.check({op: "review", review: win.saveReview.review})
                return
            }
            // A folder request with nothing checked takes the directory the window is standing in,
            // which is what the board's Choose folder button does with no row marked.
            if (win.marks.length === 0 && win.folderMode) {
                // Recent is not a directory, so there is nothing here to hand back unasked.
                if (win.recent) {
                    win.say("Press Space to select a folder first")
                    return
                }
                win.acceptMarks = true
                win.markRequest = win.check({op: "mark", path: win.path, directory: true, multiple: false})
                return
            }
            if (win.marks.length === 0) {
                if (!win.cursorFile) return
                win.acceptMarks = true
                win.markRequest = win.check({op: "mark", path: Picker.rowPath(win.path, win.cursorRow.n), directory: false, multiple: false})
                return
            }
            win.validateMarks(true)
        }

        function cancel() {
            win.finish(Picker.RESPONSE_CANCELLED, [])
        }

        // The portal reads this reply after exit, so its atomic write must finish before teardown.
        function finish(response, list) {
            if (win.answered)
                return
            // Built before the flag is set, so a throw here leaves the window answerable rather than shut.
            var text = Picker.reply(response, list, win.filterIndex)
            win.answered = true
            if (!win.backendUnavailable) backend.send({c: "picker", op: "close"})
            replyFile.setText(text)
        }

        property bool messageError: false
        function say(text, error) {
            win.message = text
            win.messageError = error === true
            messageLife.stop()
            if (!win.messageError) messageLife.restart()
        }

        Timer {
            id: messageLife
            interval: 4000
            onTriggered: win.message = ""
        }

        FileView {
            id: replyFile
            path: Quickshell.env("FLEA_PICKER_REPLY")
            atomicWrites: true
            // The reply file does not exist until this window writes it, and a preload read of a
            // path that is not there is not an error worth a line; onSaveFailed below is.
            printErrors: false
            // Sequenced on saved(), never on setText() returning: the reply is on disk before both children are reaped.
            onSaved: lifecycle.quit()
            onSaveFailed: {
                console.warn("the portal reply could not be written, so the request fails rather than reporting a refusal")
                lifecycle.quit()
            }
        }

        // A window-manager close takes the same refusal path as Cancel.
        Connections {
            target: Quickshell
            function onLastWindowClosed() {
                if (win.answered)
                    return
                win.finish(Picker.RESPONSE_CANCELLED, [])
            }
        }

        // The saved reply can leave once both chooser-owned read processes have exited.
        Flea.PickerLifecycle {
            id: lifecycle; checks: backend; listing: listing
            onStopped: Quickshell.execDetached(["kill", String(Quickshell.processId)])
        }
        Flea.PickerListing {
            id: listing
            onFailed: function(reason) { backend.failed("scan", win.path, reason, 0) }
            onMessage: function(message) { backend.receive(JSON.stringify(message)) }
        }

        Flea.Backend {
            id: backend
            pickerOnly: true
            // The saved order seeds the first listing only: a window re-sorting later would move the mark over rows that never moved.
            preserveSort: true

            onListed: function (n, readMs, sortMs) {
                if (win.backendUnavailable) return
                win.pendingListings = 0
                win.receivingLatestListing = true
                win.total = n
                win.listingState = n === 0 ? "empty" : "ready"
                // Grid needs the storage class, so ask fsinfo except on Recent.
                if (!win.recent)
                    listing.fsinfo()
            }
            onRows: function (start, items, ms, kinds) {
                if (!win.receivingLatestListing) return
                win.held = start
                win.rows = items
                win.kindNames = kinds
            }
            // A thumbed line for the previous listing is still in the pipe when clearListing
            // emptied the map, so only the live listing's answers land.
            onThumbed: function (row, file) {
                if (!win.receivingLatestListing) return
                win.thumbState = Thumbs.remember(win.thumbState, row, file, win.thumbCap)
            }
            // The worker stats its own base after the rows, so the class lands after them; a
            // settle fired in between held on unknown, and restarts now that it is named.
            onFsInfo: function (fs, free, path, storageClass) {
                if (path.length > 0 && path !== win.path) return
                win.storageClass = storageClass || ""
                win.storageKnown = true
                grid.restartSettle()
            }
            onFailed: function (where, input, msg, mode) {
                if (where === "backend") {
                    win.backendUnavailable = true
                    win.pendingListings = 0
                    win.receivingLatestListing = false
                    win.markRequest = 0
                    win.saveRequest = 0
                    win.reviewRequest = 0
                    win.acceptMarks = false
                    win.marksDirty = false
                    win.saveReview = ({})
                    win.saveError = msg
                    win.say(msg + "; cancel and reopen this request.", true)
                    win.stepFocus(null, false)
                    return
                }
                if (where === "scan" || where === "sort") {
                    selection.reset()
                    win.pendingListings = 0
                    win.listingFailed = true
                }
                // A worker lost over held rows keeps them and the footer error; only a listing that holds nothing reads as empty.
                if (win.total === 0) win.listingState = "empty"
                win.say(msg, true)
            }
            onChanged: function (path) {
                if (path === win.path && !win.submitting) win.openWithoutHistory(win.path)
            }
            onPickerResult: function (message) {
                if (win.answered || win.backendUnavailable) return
                if (message.id === win.markRequest) {
                    selection.received(message)
                } else if (message.id === win.saveRequest) {
                    win.saveRequest = 0
                    if (win.probeKey !== win.saveKey) { win.probeSave(); return }
                    if (!message.ok) { win.saveError = message.error; win.say(message.error, true); return }
                    win.saveReview = Object.assign({key: win.saveKey}, message)
                } else if (message.id === win.reviewRequest) {
                    win.reviewRequest = 0
                    if (!message.ok || !win.saveReady) {
                        win.say(message.error || "The save location changed; review it again.", true)
                        win.invalidateSave()
                        return
                    }
                    win.finish(Picker.RESPONSE_OK, [message.path])
                }
            }
        }

        Flea.PickerSelection {
            id: selection
            picker: win
            listing: listing
            backend: backend
        }

        // The history the Recent location lists, read only when that location is opened. The listing
        // is the client's own order, so the backend is asked for these paths and never to sort them.
        Flea.PickerRecent {
            id: recents
            onRefreshed: if (win.recent) win.requestListing({c: "listpaths", paths: recents.paths, first: win.windowSize})
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.color.background
            focus: true
            // The save field takes the keyboard from the list, and Escape has to refuse from there too.
            Keys.onEscapePressed: win.cancel()

            Flea.PickerChrome {
                id: chrome
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                picker: win
                onCancelRequested: win.cancel()
                onAcceptRequested: win.accept()
                onBackRequested: { win.focusView(); win.goBack() }
                onUpRequested: { win.focusView(); win.goUp() }
                onChipChosen: function (index) { win.filterIndex = index }
                onViewChosen: function (mode) { win.setView(mode) }
            }

            Flea.PickerPlaces {
                id: places
                anchors.left: parent.left
                anchors.top: chrome.bottom
                anchors.bottom: save.top
                home: win.home
                picker: win
                current: win.path
                edge: win.edge
                offerRecent: !win.saving
                onChosen: function (path) { win.open(path); win.focusView() }
                onNetworkCompleted: function(requestId, uri, success, reason) {
                    if (networkDialog.item) networkDialog.item.mountFinished(requestId, uri, success, reason)
                }
            }

            Flea.PickerHeader {
                id: header
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: chrome.bottom
                picker: win
                backend: backend
                leadingSlot: win.marksAllowed ? list.checkSize + Theme.spacing.gap : 0
                // Only the list heads columns; the save form keeps its old room, so the header yields when it and one row do not fit.
                visible: win.viewMode === "list" && save.y - chrome.height >= implicitHeight + Theme.rowHeight
                height: visible ? implicitHeight : 0
            }

            Flea.PickerList {
                id: list
                visible: win.viewMode === "list"
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: header.bottom
                anchors.bottom: save.top
                picker: win
                backend: listing
                clip: true
                focus: win.viewMode === "list"
                enabled: !win.submitting && !win.backendUnavailable
            }

            // The grid draws the same rows through the main window's tiles, with its own
            // visible-only thumbnail plan; see ui/PickerGrid.qml.
            Flea.PickerGrid {
                id: grid
                visible: win.viewMode === "grid"
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: header.bottom
                anchors.bottom: save.top
                picker: win
                backend: listing
                clip: true
                focus: win.viewMode === "grid"
                enabled: !win.submitting && !win.backendUnavailable
            }

            Connections {
                target: grid
                function onThumbsApplied(work) { win.thumbState = Thumbs.applied(win.thumbState, work) }
            }

            // The same empty hero the browser window draws, over the list area alone.
            Flea.EmptyState {
                id: hero
                x: list.x
                y: list.y
                width: list.width
                height: list.height
                visible: win.listingState === "empty"
            }

            FocusScope {
                id: shareFocus
                x: list.x
                y: list.y
                width: list.width
                height: list.height
                visible: shares.item !== null && shares.item.visible
                Keys.onPressed: function(event) {
                    var action = Keymap.lookup(event.key, event.text, event.modifiers, "rail")
                    event.accepted = true
                    if (event.key === Qt.Key_Escape || action === "parent" || action === "historyBack") shares.item.close()
                    else if (action === "cursorDown") shares.item.moveCursor(1)
                    else if (action === "cursorUp") shares.item.moveCursor(-1)
                    else if (action === "open" || event.key === Qt.Key_Space) shares.item.activateCursor()
                    else if (event.key === Qt.Key_Tab) win.stepFocus(list, (event.modifiers & Qt.ShiftModifier) !== 0)
                    else if (event.key === Qt.Key_Backtab) win.stepFocus(list, true)
                }
                Loader {
                    id: shares
                    anchors.fill: parent
                    active: false
                    source: "ShareBrowser.qml"
                }
                Connections {
                    target: shares.item
                    function onClosed() { win.focusView() }
                    function onActivated(uri, label) { places.openChild(uri, label) }
                }
            }

            Flea.PickerSave {
                id: save
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: status.top
                maximumHeight: Math.max(0, status.y - chrome.height - Theme.rowHeight)
                picker: win
                onNameEdited: function (text) { win.saveName = text }
                onAccepted: win.accept()
            }

            // The footer: what is checked on the left, the keys that act on it on the right.
            Item {
                id: status
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: Theme.chromeHeight

                // The footer takes the chrome plane, the same strip the ask above it stands on.
                Rectangle {
                    anchors.fill: parent
                    color: Theme.color.surface
                }

                Rectangle {
                    anchors.top: parent.top
                    width: parent.width
                    height: Theme.spacing.hairline
                    color: win.edge
                }

                Text {
                    id: statusMessage
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.right: statusHints.left
                    anchors.rightMargin: Theme.spacing.gap
                    anchors.verticalCenter: parent.verticalCenter
                    text: win.message.length > 0 ? win.message : !win.marksAllowed ? "" : Picker.statusLine(win.marks.length, Picker.totalBytes(win.marks))
                    color: win.messageError ? Theme.color.error : Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }

                Text {
                    id: statusHints
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacing.rowPaddingX
                    width: Math.min(implicitWidth, Math.max(0, parent.width - 2 * Theme.spacing.rowPaddingX
                        - Theme.spacing.gap - Math.min(statusMessage.implicitWidth, parent.width / 2)))
                    anchors.verticalCenter: parent.verticalCenter
                    text: win.backendUnavailable ? "Esc cancel" : Picker.hints(win.req)
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
            }

            Loader {
                id: networkDialog
                anchors.fill: parent
                active: false
                sourceComponent: Component {
                    Flea.NetworkDialog {
                        onClosed: win.focusView()
                        onMountRequested: function(requestId, uri, label, password) { places.retry(requestId, uri, label, password) }
                        onCancelRequested: function(requestId) { places.cancelNetwork(requestId) }
                    }
                }
            }
        }

        function showShares(uri, label, names) {
            shares.active = true
            shares.item.open(uri, label, names)
            shareFocus.forceActiveFocus()
        }
        function retryNetwork(uri, label, password, reason, failed) {
            networkDialog.active = true
            networkDialog.item.openLocation(uri, label, password, reason, failed)
        }

        Component.onCompleted: {
            // Only an absolute path is a folder, so a caller cannot name the Recent token, or any
            // other text, as the directory this window opens on.
            var start = win.req.folder.charAt(0) === "/" ? win.req.folder : win.home
            win.viewMode = Picker.rememberedView(ViewState.pickerView)
            win.openWithoutHistory(start)
            // Measured on the box: without this the window has the keyboard but the list does not,
            // so Escape reached the surface below and every other key was dropped.
            win.focusView()
        }

        // The seam tests/picker.sh drives, the same read-only shape ui/Ipc.qml has for the window.
        IpcHandler {
            target: "fleapicker"
            function ready(): bool { return true }
            function path(): string { return win.path }
            function total(): int { return win.total }
            function shownTotal(): int { return win.shownTotal }
            function cursor(): int { return win.cursorIndex }
            function marks(): string { return Picker.paths(win.marks).join(",") }
            function rowAt(index: int): string { var row = win.rowFor(index); return row ? row.n : "" }
            function cursorName(): string { return win.rowFor(win.cursorIndex) ? win.rowFor(win.cursorIndex).n : "" }
            function state(): string { return win.listingState }
            function recent(): bool { return win.recent }
            function accept(): string { return Picker.acceptLabel(win.req, win.marks.length) }
            function chip(): int { return win.filterIndex }
            function saveName(): string { return win.saveName }
            function message(): string { return win.message }
            function checks(): string { return JSON.stringify({marksBusy: win.markRequest > 0, saveBusy: win.saveRequest > 0, submitting: win.submitting, canAccept: win.canAccept, saveReady: win.saveReady, collision: win.saveCollision, review: win.saveReview.review || 0}) }
            function markedUris(): string { return JSON.stringify(win.marks.map(function(mark) { return mark.uri })) }
            function controls(): string { return JSON.stringify(chrome.controls().concat(header.controls(), save.controls(), places.controls())) }
            function rowCentre(index: int): string { return win.centre(win.viewItem().itemAtIndex(index)) }
            function saveState(): string { return JSON.stringify({name: win.saveName, path: win.saveReview.path || "", collision: win.saveCollision, error: win.saveError, field: win.centre(save.fieldItem)}) }
            function snapshot(): string {
                return JSON.stringify({path: win.path, total: win.total, held: win.held, rows: win.rows,
                    cursor: win.cursorIndex, cursorName: win.rowFor(win.cursorIndex) ? win.rowFor(win.cursorIndex).n : "",
                    marks: win.marks, state: win.listingState, emptyHero: hero.visible, listingFailed: win.listingFailed, sortBy: backend.sortBy, sortDesc: backend.sortDesc, sortable: win.sortable, filter: win.filterIndex, history: win.history,
                    view: win.viewMode, thumbPending: Object.keys(win.thumbState.file).filter(function(index) { return win.thumbState.file[index] === null || win.thumbState.file[index] === "cache-asked" }).length,
                    marksBusy: win.markRequest > 0, saveBusy: win.saveRequest > 0, submitting: win.submitting, backendUnavailable: win.backendUnavailable,
                    canAccept: win.canAccept, saveReady: win.saveReady, collision: win.saveCollision,
                    saveName: win.saveName, saveError: win.saveError, message: win.message, messageError: win.messageError, hints: statusHints.text,
                    controls: chrome.controls().concat(header.controls(), save.controls(), places.controls()), headerMark: header.sortBy, listFocus: list.activeFocus, gridFocus: grid.activeFocus,
                    railFocus: places.focusItem.activeFocus, preset: Flea.ViewState.keysPreset,
                    bodySmall: Theme.font.bodySmall, body: Theme.font.body, width: win.width, height: win.height,
                    themeLoaded: Theme.ready, themeForeground: String(Theme.color.foreground),
                    geometry: {chrome: chrome.height, header: header.height, rail: places.width, row: Theme.rowHeight, footer: status.height,
                        save: save.height, list: list.height, saveViewport: win.bounds(save.scrollItem), saveScroll: save.scrollItem.contentY},
                    outputUri: {text: save.uri, offset: save.uriItem.contentX,
                        maximum: Math.max(0, save.uriItem.contentWidth - save.uriItem.width)}, title: win.title, app: win.req.app})
            }
        }

        function centre(item) {
            if (!item) return ""
            var point = item.mapToItem(win.contentItem, item.width / 2, item.height / 2)
            return Math.round(point.x) + " " + Math.round(point.y)
        }
        function control(name, item, available) {
            return {name: name, visible: item.visible, enabled: available, focused: item.activeFocus, centre: win.centre(item), bounds: win.bounds(item)}
        }
        function bounds(item) {
            if (!item) return []
            var point = item.mapToItem(win.contentItem, 0, 0)
            return [point.x, point.y, item.width, item.height]
        }
        function stepFocus(from, back) {
            var items = chrome.focusItems().concat([places.focusItem, win.viewItem()], save.focusItems())
                .filter(function(item) { return item.visible && item.enabled && item.activeFocusOnTab })
            if (!items.length) return
            var at = items.indexOf(from)
            var next = items[(at + (back ? -1 : 1) + items.length) % items.length]
            if (next === list && shares.item && shares.item.active) shareFocus.forceActiveFocus(Qt.TabFocusReason)
            else next.forceActiveFocus(Qt.TabFocusReason)
        }
    }
}
