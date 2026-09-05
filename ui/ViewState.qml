pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "js/UiState.js" as UiState

// The per-user state that outlives a window, `~/.local/state/flea/ui.json`. Read once here with a
// blocking FileView so the first paint already has it, and never written from QML: every change
// goes back out through `flea --ui-state`, the one Rust path that takes the lock, validates each
// key, merges the caller's and renames a temp into place. main() settles this file before the window,
// so whenever that settle succeeded on a document it could read, what is read here has already been
// through that same validation; one it could not read is left alone and lands in the default shape
// UiState.fromFile answers with. See AGENTS.md "The state file".
QtObject {
    id: root

    // The whole document, so a later section reads its own key without a second file read.
    property var state: ({})

    // Mirrors "columns" in src/uischema.rs, and is the only default this front end needs before the
    // first frame: it is what a first launch draws, when there is no file to settle and none to read.
    readonly property var defaultColumns: ["name", "size", "date"]

    // ui.json names what is SHOWN. ui/Header.qml, ui/Row.qml and ui/ContextMenu.qml all ask the
    // opposite question, so the inversion lives here once rather than at each of them.
    readonly property var columns: Array.isArray(root.state.columns) ? root.state.columns : root.defaultColumns
    readonly property var hiddenCols: {
        var out = []
        var optional = ["mode", "size", "date", "kind"]
        for (var i = 0; i < optional.length; i++) {
            if (root.columns.indexOf(optional[i]) < 0)
                out.push(optional[i])
        }
        return out
    }

    // The interface scale Ctrl+Shift+Plus steps, issue 9. It lives for the window and no longer
    // outlives it: 0.1.4 stores an Omarchy text-size stop and never a free multiplier.
    property real uiScale: 1

    // Flipped by ui/Pane.qml's onChosen, when a header-menu row answers "col:<key>".
    function toggleColumn(key) {
        var shown = root.columns.slice()
        var at = shown.indexOf(key)
        if (at >= 0)
            shown.splice(at, 1)
        else
            shown.push(key)
        root.state = Object.assign({}, root.state, { columns: shown })
        root.save()
    }

    // A patch flea refused, or a state file it could not write. The pane turns it into the status
    // bar's one sentence: the change is on screen and the file does not have it.
    signal saveFailed()

    // main() leaves a ui.json it cannot read as a JSON object exactly as the operator wrote it, so
    // what is drawn below is the shipped defaults and ui/PaneWire.qml is where that is said once.
    property bool unreadable: false

    // ui/js/UiState.js's book: what the state file holds, what the writer carries, what waits behind
    // it. A save that would change nothing writes nothing, because ui/shell.qml's scale step calls
    // save() too and the state file carries no scale.
    property var writeBook: UiState.book("")

    function save() {
        var next = UiState.asked(root.writeBook, JSON.stringify({ columns: root.columns }))
        root.writeBook = next
        if (next.start.length > 0)
            root.run(next.start)
    }

    function run(patch) {
        patcher.command = [Quickshell.env("FLEA_BIN") || "flea", "--ui-state", patch]
        patcher.running = true
    }

    // The read is taken here and not in the FileView's onLoaded, which was measured on the box
    // arriving after the first property read; blockLoading is what makes text() answer inside this
    // call, so the stored columns are in the first frame instead of replacing it.
    Component.onCompleted: root.load(stateFile.text())

    function load(text) {
        var read = UiState.fromFile(text)
        root.state = read.state
        root.unreadable = read.unreadable
        root.writeBook = UiState.book(JSON.stringify({ columns: root.columns }))
    }

    // blockLoading, because the first list draws from this: an async read would paint one column
    // set and correct it. printErrors off, because a missing file is what a first launch looks like.
    property var store: FileView {
        id: stateFile
        path: (Quickshell.env("XDG_STATE_HOME") && Quickshell.env("XDG_STATE_HOME").length > 0
               ? Quickshell.env("XDG_STATE_HOME") : Quickshell.env("HOME") + "/.local/state") + "/flea/ui.json"
        blockLoading: true
        watchChanges: false
        printErrors: false
    }

    // The writer answered, with its own status or with 2 for one that never started: the same refusal
    // to the pane and the same retry to the book, because neither reached the file.
    function wrote(exitCode) {
        var next = UiState.exited(root.writeBook, exitCode)
        root.writeBook = next
        if (next.failed)
            root.saveFailed()
        if (next.start.length > 0)
            root.run(next.start)
    }

    property var writer: Process {
        id: patcher
        onExited: function (exitCode, exitStatus) { root.wrote(exitCode) }
        // Measured on Quickshell 0.3.1: a Process that cannot start its program emits no exited at
        // all, only running going false, and a real exit clears the book before its own false
        // arrives, so this fires for the writer that never ran and never for one that did.
        onRunningChanged: {
            if (!patcher.running && root.writeBook.inflight.length > 0)
                root.wrote(2)
        }
    }
}
