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
        implicitWidth: Theme.space(640)
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
        property string message: ""

        // The checked identities, each a path and its size, so Back and Parent cannot rebind one.
        property var marks: []
        // Which chip is active: an index into the caller's filters, or -1 for All files.
        property int filterIndex: Picker.currentChip(win.req)
        readonly property var filter: win.filterIndex >= 0 ? win.req.filters[win.filterIndex] : null
        readonly property var shown: Picker.shownRows(win.rows, win.held, win.filter)
        readonly property int shownTotal: win.shown === null ? win.total : win.shown.length

        // Where Back goes, and it only ever goes back: Parent is its own button and pushes here too.
        property var history: []
        // The save mode's own name, which starts as the caller's suggestion only when that
        // suggestion is a filename: tools/flea-portal passes current_name through verbatim, so a
        // separator in it would put a path outside this folder in the field before anyone typed.
        property string saveName: Picker.validName(win.req.name) ? win.req.name : ""

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
            if (next === win.path)
                return
            if (win.path.length > 0)
                win.history = win.history.concat([win.path])
            win.openWithoutHistory(next)
        }

        function openWithoutHistory(next) {
            win.path = next
            win.total = 0
            win.held = 0
            win.rows = []
            win.cursorIndex = 0
            win.listingState = "loading"
            if (Picker.isRecent(next)) {
                recents.refresh()
                return
            }
            backend.list(next, win.windowSize, false)
        }

        // A property var does not notify on an in-place mutation, so history is reassigned, never popped.
        function goBack() {
            if (win.history.length === 0)
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
            var row = win.rowFor(index)
            if (!row || row.d !== win.folderMode)
                return
            win.marks = Picker.toggle(win.marks, Picker.rowPath(win.path, row.n), row.s, win.req.multiple)
        }

        // Enter. A directory is always walked into, even in the folder request the board draws it
        // marked in, and a file submits what is checked: nothing checked is nothing to submit, which
        // is the board's own rule and what keeps a stray Enter from sending.
        function activate(index) {
            var row = win.rowFor(index)
            if (!row)
                return
            if (row.d) {
                win.open(Picker.rowPath(win.path, row.n))
                return
            }
            win.accept()
        }

        function accept() {
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
                win.finish(Picker.RESPONSE_OK, [Picker.join(win.path, win.saveName)])
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
                win.finish(Picker.RESPONSE_OK, [win.path])
                return
            }
            if (win.marks.length === 0) {
                win.say("Press Space to select a file first")
                return
            }
            win.finish(Picker.RESPONSE_OK, Picker.paths(win.marks))
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
            var text = Picker.reply(response, list)
            win.answered = true
            replyFile.setText(text)
        }

        function say(text) {
            win.message = text
            messageLife.restart()
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
                win.total = n
                win.listingState = n === 0 ? "empty" : "ready"
            }
            onRows: function (start, items, ms, kinds) {
                win.held = start
                win.rows = items
                win.kindNames = kinds
            }
            onFailed: function (where, input, msg, mode) {
                win.listingState = "empty"
                win.say(msg)
            }
        }

        // The history the Recent location lists, read only when that location is opened. The listing
        // is the client's own order, so the backend is asked for these paths and never to sort them.
        Flea.PickerRecent {
            id: recents
            onRefreshed: if (win.recent) backend.listPaths(recents.paths, win.windowSize)
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
                onBackRequested: win.goBack()
                onUpRequested: win.goUp()
                onChipChosen: function (index) { win.filterIndex = index }
            }

            Flea.PickerPlaces {
                id: places
                anchors.left: parent.left
                anchors.top: chrome.bottom
                anchors.bottom: save.top
                home: win.home
                current: win.path
                edge: win.edge
                offerRecent: !win.saving
                onChosen: function (path) { win.open(path); list.forceActiveFocus() }
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
            }

            // The same empty hero the browser window draws, over the list area alone.
            Flea.EmptyState {
                x: list.x
                y: list.y
                width: list.width
                height: list.height
                visible: win.listingState === "empty"
            }

            Flea.PickerSave {
                id: save
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: status.top
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
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    text: win.message.length > 0 ? win.message : Picker.statusLine(win.marks.length, Picker.totalBytes(win.marks))
                    color: win.message.length > 0 ? Theme.color.accent : Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }

                Text {
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    text: Picker.hints(win.req)
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }
            }
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
        }
    }
}
