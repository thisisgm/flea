pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// The per-user state that outlives a window, `~/.local/state/flea/ui.json`. Read once here with a
// blocking FileView so the first paint already has it, and never written from QML: every change
// goes back out through `flea --ui-state`, the one Rust path that takes the lock, validates each
// key, merges the caller's and renames a temp into place. See AGENTS.md "The state file".
QtObject {
    id: root

    // The whole document, so a later section reads its own key without a second file read.
    property var state: ({})

    // Mirrors "columns" in src/uischema.rs, and is the only default this front end needs before the
    // first frame; the Rust side owns every other one and answers with the whole shape.
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

    // What the state file already holds, so a save that would change nothing writes nothing:
    // ui/shell.qml's scale step calls save() too and the state file carries no scale.
    property string saved: ""
    property string pending: ""

    function save() {
        var patch = JSON.stringify({ columns: root.columns })
        if (patch === root.saved)
            return
        root.saved = patch
        // One writer at a time, and the newest patch waits rather than being dropped on the floor.
        if (patcher.running)
            root.pending = patch
        else
            root.run(patch)
    }

    function run(patch) {
        root.pending = ""
        patcher.command = [Quickshell.env("FLEA_BIN") || "flea", "--ui-state", patch]
        patcher.running = true
    }

    // The read is taken here and not in the FileView's onLoaded, which was measured on the box
    // arriving after the first property read; blockLoading is what makes text() answer inside this
    // call, so the stored columns are in the first frame instead of replacing it.
    Component.onCompleted: root.load(stateFile.text())

    function load(text) {
        try {
            var parsed = JSON.parse(text)
            if (parsed && typeof parsed === "object" && !Array.isArray(parsed))
                root.state = parsed
        } catch (e) {
            // A file this cannot read is the update path's problem, and it answers with the defaults.
        }
        root.saved = JSON.stringify({ columns: root.columns })
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

    property var writer: Process {
        id: patcher
        onExited: if (root.pending.length > 0) root.run(root.pending)
    }
}
