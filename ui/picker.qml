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
import "js/Sort.js" as Sort

// One portal request, one window: the org.freedesktop.impl.portal.FileChooser dialog every caller on
// the box gets, opened by flea --pick and answered through the reply file tools/flea-portal reads.
// The same Backend, Row, Theme and places the browser window draws with, and none of its operations:
// a chooser that can rename or delete is a file manager wearing a dialog's clothes.
ShellRoot {
    FloatingWindow {
        id: win

        title: Picker.title(state.req)
        implicitWidth: Theme.space(640)
        implicitHeight: Theme.space(440)
        color: Theme.color.background

        // ui/PickerState.qml's finish writes the answer through this; the window only holds it.
        readonly property alias replyFile: replyFile

        // The chooser's state and every move on it, see ui/PickerState.qml. Every child below
        // takes it as its picker; the window itself only draws and holds the reply file.
        Flea.PickerState {
            id: state
            window: win
            backend: backend
            recents: recents
            footer: status
            navigate: navigate
            fetcher: fetcher
            list: list
            places: places
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
                if (state.answered)
                    return
                state.finish(Picker.RESPONSE_CANCELLED, [])
            }
        }

        // Quickshell 0.3.1 has no exit API and Qt.quit() is a no-op, so the window signals itself,
        // exactly as ui/shell.qml does, once the backend says it has drained.
        Connections { target: backend; function onQuitReady() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) } }

        Flea.Backend {
            id: backend

            onListed: function (n, readMs, sortMs) {
                state.total = n
                state.listingState = n === 0 ? "empty" : "ready"
            }
            onRows: function (start, items, ms, kinds) {
                state.held = start
                state.rows = items
                state.kindNames = kinds
                navigate.rowsArrived()
            }
            onPeeked: function (path, hidden, total, rows, readFailed) { navigate.peeked(path, hidden, total, rows, readFailed) }
        }

        // The operation replies, and the failures: trashed, renamed, made and the rest, see ui/PickerWire.qml.
        Flea.PickerWire { picker: state }

        // The history the Recent location lists, read only when that location is opened. The listing
        // is the client's own order, so the backend is asked for these paths and never to sort them.
        Flea.PickerRecent {
            id: recents
            onRefreshed: if (state.recent) backend.listPaths(recents.paths, state.windowSize)
        }

        // What a typed line does: a folder opens, a file is selected in its parent, and the rest is
        // refused in the footer. It asks the backend before it opens anything; see ui/PickerNavigate.qml.
        Flea.PickerNavigate {
            id: navigate
            picker: state
            backend: backend
            entry: entryField
            list: list
            onRemoteEntered: function (answer) { fetcher.enter(answer) }
        }

        // A typed URL is downloaded to the picker cache and answered as that file, see ui/PickerFetch.qml.
        Flea.PickerFetch {
            id: fetcher
            picker: state
            backend: backend
        }

        // A typed share URL: mounted at its root, then walked on its FUSE path through navigate.
        Flea.PickerShare { picker: state; navigate: navigate }

        Rectangle {
            anchors.fill: parent
            color: Theme.color.background
            focus: true
            // The save field takes the keyboard from the list, and Escape has to refuse from there too;
            // a fetch in flight takes the key first, so Escape then cancels the download, not the dialog.
            Keys.onEscapePressed: if (!fetcher.takeEscape()) state.cancel()

            Flea.PickerChrome {
                id: chrome
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                picker: state
                onCancelRequested: state.cancel()
                onAcceptRequested: state.accept()
                onBackRequested: state.goBack()
                onUpRequested: state.goUp()
                onChipChosen: function (index) { state.filterIndex = index }
                onCrumbChosen: function (path) { state.open(path); state.focusList() }
            }

            Flea.PickerPlaces {
                id: places
                anchors.left: parent.left
                anchors.top: chrome.bottom
                anchors.bottom: entryField.top
                home: state.home
                current: state.path
                edge: state.edge
                offerRecent: !state.saving
                focused: state.focusView === "rail"
                onChosen: function (path) { state.open(path); state.focusList() }
            }

            // The window's own column header at the picker's column set, over the same width the
            // rows take, so the two resolve one set. A click sorts through ui/js/Sort.js as the
            // window's does; Recent is the history's own order, so a click there asks for nothing.
            Flea.Header {
                id: header
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: chrome.bottom
                sortBy: backend.sortBy
                sortDesc: backend.sortDesc
                hiddenCols: Picker.HIDDEN_COLS
                dateWidth: Theme.column.pickerDate
                leadingSlot: list.checkSize + Theme.spacing.gap
                onSortRequested: function (key) { if (!state.recent) Sort.column(state, key) }
            }

            // The browser's own query line, under the header as the window stacks it: it reads
            // state as its pane and collapses to nothing while no filter is up.
            Flea.FilterStrip {
                id: filterStrip
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: header.bottom
                pane: state
            }

            Flea.PickerList {
                id: list
                anchors.left: places.right
                anchors.right: parent.right
                anchors.top: filterStrip.bottom
                anchors.bottom: entryField.top
                picker: state
                backend: backend
                // ":" takes the keyboard to whichever field the mode draws.
                entry: state.saving ? save : entryField
                clip: true
                focus: true
            }

            // The same empty hero the browser window draws, over the list area alone.
            Flea.EmptyState {
                x: list.x
                y: list.y
                width: list.width
                height: list.height
                visible: state.listingState === "empty"
            }

            // The location field, above the footer in the open modes.
            Flea.PickerEntry {
                id: entryField
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: save.top
                picker: state
                onEntered: function (text) { navigate.enter(text) }
                onDismissed: { fetcher.takeEscape(); list.forceActiveFocus() }
            }

            Flea.PickerSave {
                id: save
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: status.top
                picker: state
                onNameEdited: function (text) { state.saveName = text }
                onAccepted: state.accept()
            }

            Flea.PickerFooter {
                id: status
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                picker: state
                fetch: fetcher
            }
        }

        Component.onCompleted: {
            // Only an absolute path is a folder, so a caller cannot name the Recent token, or any
            // other text, as the directory this window opens on.
            var start = state.req.folder.charAt(0) === "/" ? state.req.folder : state.home
            state.openWithoutHistory(start)
            // Measured on the box: without this the window has the keyboard but the list does not,
            // so Escape reached the surface below and every other key was dropped.
            list.forceActiveFocus()
        }

        // The seam tests/picker.sh drives, see ui/PickerIpc.qml.
        Flea.PickerIpc {
            state: state
            chrome: chrome
            places: places
            footer: status
            fetcher: fetcher
            entry: entryField
            save: save
            list: list
            header: header
        }
    }
}
