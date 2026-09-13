// Its own app id, so one Hyprland rule can give the chooser the floating treatment Omarchy already
// gives xdg-desktop-portal-gtk without touching the window; flea --picker writes that rule.
//@ pragma AppId com.thisisgm.flea.picker
//@ pragma ShellId fleapicker
//@ pragma NativeTextRendering
//@ pragma CacheDir $BASE/flea

import Quickshell
import Quickshell.Io
import QtQuick
import qs.Commons
import "." as Flea
import "js/Picker.js" as Picker
import "js/Keymap.js" as Keymap

// One portal request, one window: the org.freedesktop.impl.portal.FileChooser dialog every caller on
// the box gets, opened by flea --pick and answered through the reply file tools/flea-portal reads.
// The same Backend, Row, Theme and places the browser window draws with, and none of its operations:
// a chooser that can rename or delete is a file manager wearing a dialog's clothes.
ShellRoot {
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
        // Submission disables its initiating control; keep cancellation on the enabled focus path.
        onSubmittingChanged: if (win.submitting) win.stepFocus(null, false)
        readonly property bool canAccept: !win.backendUnavailable && !win.submitting && !win.markRequest && (win.saving
            ? win.saveReady : win.marks.length > 0 || (win.folderMode && !win.recent && win.listingState !== "loading"))

        // SendPicker.html draws every rule and control frame in one ink, a lift over whatever plane
        // it sits on. Theme.color.surface is a drop on these palettes and vanishes against the chrome
        // strips, so the picker takes the OEM's own resting border alpha, which lifts on both.
        readonly property color edge: Style.hoverBorderColor

        // Recent is a location and not a directory: the rail's own row opens it and the listing it
        // builds comes from the desktop's history rather than a scan; see AGENTS.md "Recent, and why".
        readonly property bool recent: Picker.isRecent(win.path)

        readonly property bool saving: win.req.mode === "save"
        readonly property bool folderMode: win.req.directory || win.req.mode === "savefiles"
        readonly property int windowSize: list.visibleRows + 60

        // Exactly one answer leaves this window, whichever way it is asked for.
        property bool answered: false

        function rowFor(index) {
            var at = index - win.held
            return at >= 0 && at < win.rows.length ? win.rows[at] : null
        }

        function open(next) {
            if (win.submitting || next === win.path)
                return
            if (win.path.length > 0)
                win.history = win.history.concat([win.path])
            win.openWithoutHistory(next)
        }

        function openWithoutHistory(next) {
            if (win.backendUnavailable) return
            if (shares.item) shares.item.close()
            win.path = next
            win.total = 0
            win.held = 0
            win.rows = []
            win.cursorIndex = 0
            win.listingState = "loading"
            win.receivingLatestListing = false
            win.invalidateSave()
            win.validateMarks(false)
            if (Picker.isRecent(next)) {
                recents.refresh()
                return
            }
            win.requestListing({c: "list", path: next, first: win.windowSize, hidden: false})
        }

        function requestListing(request) {
            win.pendingListings++
            backend.send(Object.assign(request, win.filterRequest()))
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

        function validateMarks(accepting) {
            if (win.backendUnavailable) return
            if (!win.marks.length && !win.markRequest) return
            if (win.markRequest) { win.marksDirty = true; return }
            win.acceptMarks = accepting
            win.markRequest = win.check({op: "validate"})
        }

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

        // A property var does not notify on an in-place mutation, so history is reassigned, never popped.
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

        // Space. A directory is markable only when the request asked for one, and a file only when
        // it did not: the board draws no check at all on the rows the caller cannot receive.
        function toggleMark(index) {
            if (win.backendUnavailable) return
            if (win.markRequest || win.submitting) { win.say("Selection is still being checked."); return }
            var row = win.rowFor(index)
            if (!row || Picker.directory(row) !== win.folderMode)
                return
            win.markRequest = win.check({op: "mark", path: Picker.rowPath(win.path, row.n), directory: win.folderMode, multiple: win.req.multiple})
        }

        // Enter. A directory is always walked into, even in the folder request the board draws it
        // marked in, and a file submits what is checked: nothing checked is nothing to submit, which
        // is the board's own rule and what keeps a stray Enter from sending.
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
                win.say("Press Space to select a file first")
                return
            }
            win.validateMarks(true)
        }

        function cancel() {
            win.finish(Picker.RESPONSE_CANCELLED, [])
        }

        // The one write out of this process. The window closes only once the reply file is on disk,
        // because tools/flea-portal reads it after this process exits and a lost write is a fault.
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
            // Sequenced on saved(), never on setText() returning: the answer has to be readable
            // before this process ends, and Quickshell writes it on its own thread. The backend is
            // told next, ui/shell.qml's own exit gate, so no listing child outlives this window.
            onSaved: backend.quit()
            onSaveFailed: {
                console.warn("the portal reply could not be written, so the request fails rather than reporting a refusal")
                backend.quit()
            }
        }

        // Closing the window is a refusal, the board's own rule, and it takes the same path a
        // pressed Cancel does. A window closed after an answer is the answer's own teardown.
        Connections {
            target: Quickshell
            function onLastWindowClosed() {
                if (win.answered)
                    return
                win.finish(Picker.RESPONSE_CANCELLED, [])
            }
        }

        // Quickshell 0.3.1 has no exit API and Qt.quit() is a no-op, so the window signals itself,
        // exactly as ui/shell.qml does, once the backend says it has drained.
        Connections { target: backend; function onQuitReady() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) } }

        Flea.Backend {
            id: backend

            onListed: function (n, readMs, sortMs) {
                if (win.backendUnavailable) return
                win.pendingListings = Math.max(0, win.pendingListings - 1)
                win.receivingLatestListing = win.pendingListings === 0
                if (!win.receivingLatestListing) return
                win.total = n
                win.listingState = n === 0 ? "empty" : "ready"
            }
            onRows: function (start, items, ms, kinds) {
                if (!win.receivingLatestListing) return
                win.held = start
                win.rows = items
                win.kindNames = kinds
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
                    win.pendingListings = Math.max(0, win.pendingListings - 1)
                    if (win.pendingListings > 0) return
                }
                win.listingState = "empty"
                win.say(msg, true)
            }
            onChanged: function (path) {
                if (path === win.path && !win.submitting) win.openWithoutHistory(win.path)
            }
            onPickerResult: function (message) {
                if (win.answered || win.backendUnavailable) return
                if (message.id === win.markRequest) {
                    win.markRequest = 0
                    var accepting = win.acceptMarks
                    win.acceptMarks = false
                    if (!message.ok) { win.say(message.error, true); return }
                    win.marks = Picker.reviewedMarks(win.marks, message.marks)
                    if (message.removed) win.say(message.removed === 1
                        ? "1 selected item moved or changed; select it again."
                        : message.removed + " selected items moved or changed; select them again.", true)
                    else if (accepting && win.marks.length) { win.finish(Picker.RESPONSE_OK, win.marks); return }
                    if (win.marksDirty) { win.marksDirty = false; win.validateMarks(false) }
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
                onBackRequested: { list.forceActiveFocus(); win.goBack() }
                onUpRequested: { list.forceActiveFocus(); win.goUp() }
                onChipChosen: function (index) { win.filterIndex = index }
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
                onChosen: function (path) { win.open(path); list.forceActiveFocus() }
                onNetworkCompleted: function(requestId, uri, success, reason) {
                    if (networkDialog.item) networkDialog.item.mountFinished(requestId, uri, success, reason)
                }
            }

            Flea.PickerList {
                id: list
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: chrome.bottom
                anchors.bottom: save.top
                picker: win
                backend: backend
                clip: true
                focus: true
                enabled: !win.submitting && !win.backendUnavailable
            }

            // The same empty hero the browser window draws, over the list area alone.
            Flea.EmptyState {
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
                    function onClosed() { list.forceActiveFocus() }
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
                    text: win.message.length > 0 ? win.message : Picker.statusLine(win.marks.length, Picker.totalBytes(win.marks))
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
                        onClosed: list.forceActiveFocus()
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
            win.openWithoutHistory(start)
            // Measured on the box: without this the window has the keyboard but the list does not,
            // so Escape reached the surface below and every other key was dropped.
            list.forceActiveFocus()
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
            function controls(): string { return JSON.stringify(chrome.controls().concat(save.controls(), places.controls())) }
            function rowCentre(index: int): string { return win.centre(list.itemAtIndex(index)) }
            function saveState(): string { return JSON.stringify({name: win.saveName, path: win.saveReview.path || "", collision: win.saveCollision, error: win.saveError, field: win.centre(save.fieldItem)}) }
            function snapshot(): string {
                return JSON.stringify({path: win.path, total: win.total, held: win.held, rows: win.rows,
                    cursor: win.cursorIndex, cursorName: win.rowFor(win.cursorIndex) ? win.rowFor(win.cursorIndex).n : "",
                    marks: win.marks, state: win.listingState, filter: win.filterIndex, history: win.history,
                    marksBusy: win.markRequest > 0, saveBusy: win.saveRequest > 0, submitting: win.submitting, backendUnavailable: win.backendUnavailable,
                    canAccept: win.canAccept, saveReady: win.saveReady, collision: win.saveCollision,
                    saveName: win.saveName, saveError: win.saveError, message: win.message, messageError: win.messageError, hints: statusHints.text,
                    controls: chrome.controls().concat(save.controls(), places.controls()), listFocus: list.activeFocus,
                    railFocus: places.focusItem.activeFocus, preset: Flea.ViewState.keysPreset,
                    bodySmall: Theme.font.bodySmall, body: Theme.font.body, width: win.width, height: win.height,
                    geometry: {chrome: chrome.height, rail: places.width, row: Theme.rowHeight, footer: status.height,
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
            var items = chrome.focusItems().concat([places.focusItem, list], save.focusItems())
                .filter(function(item) { return item.visible && item.enabled && item.activeFocusOnTab })
            if (!items.length) return
            var at = items.indexOf(from)
            var next = items[(at + (back ? -1 : 1) + items.length) % items.length]
            if (next === list && shares.item && shares.item.active) shareFocus.forceActiveFocus(Qt.TabFocusReason)
            else next.forceActiveFocus(Qt.TabFocusReason)
        }
    }
}
