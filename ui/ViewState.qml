pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "js/Keymap.js" as Keymap
import "js/Settings.js" as Settings
import "js/TextSize.js" as TextSize
import "js/UiState.js" as UiState

// The per-user state that outlives a window, `~/.local/state/flea/ui.json`. Read once here with a
// blocking FileView so the first paint already has it, and never written from QML: every change
// goes back out through `flea --ui-state`, the one Rust path that takes the lock, validates each
// key, merges the caller's and renames a temp into place. main() settles this file before the
// window, so whenever that settle succeeded on a document it could read, what is read here has
// already been through that same validation; one it could not read is left alone and lands in the
// default shape UiState.fromFile answers with. There is no second settings file: the settings panel
// writes these same keys through the same patch. See AGENTS.md "The state file".
QtObject {
    id: root

    // The whole document, so a later section reads its own key without a second file read.
    property var state: ({})

    // Mirrors "columns" in src/uischema.rs, and is the only default this front end needs before the
    // first frame: it is what a first launch draws, when there is no file to settle and none to read.
    readonly property var defaultColumns: ["name", "size", "date"]

    // Mirrors "menu"."hidden" in src/uischema.rs, for the same first launch: six ids this release's
    // menu cannot build, plus Copy path and Open in terminal, which the SettingsMenus board ships
    // switched off. Every id here is an action ui/js/Menu.js gives a row, or will give one.
    readonly property var defaultMenuHidden: ["delete", "openwith", "openTerminal", "moveto",
                                              "copyto", "properties", "permissions", "copypath"]

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

    // The Display section's text size, `display.textSize` in src/uischema.rs: {"mode":"system"}
    // follows Omarchy and is the default, and an override pins one of TextSize.js's seven stops as
    // {"mode":N}. ui/Theme.qml is the only consumer, and the monitor scale beside it in the panel is
    // the compositor's alone.
    readonly property var display: root.state.display || ({})
    readonly property var textSize: TextSize.parse(root.display.textSize)

    // Omarchy's own size, held here so the Ctrl+Shift chords and the panel's own rows step away
    // from and back to one anchor rather than each resolving their own.
    readonly property int omarchyBase: Style.font.baseSize

    // The context-menu actions switched off in the settings panel's Menus section, by action id, and
    // the section's only state: ui/js/Menu.js applyHidden is the consumer and ui/ContextMenu.qml the
    // one caller that passes it in, and the panel's master row is derived from this set, not stored.
    readonly property var menu: root.state.menu || ({})
    readonly property var menuHidden: Array.isArray(root.menu.hidden) ? root.menu.hidden
                                                                      : root.defaultMenuHidden

    // The Menus section's "Show keyboard hints" row, `keyHints` in src/uischema.rs. It draws the key
    // beside every menu row and the tip under an empty directory, and it binds no key of its own:
    // every chord answers whether this is on or off.
    readonly property bool keyHints: root.state.keyHints === true

    // "mac" or "windows", the Keys section's two-value toggle over the one generated key table.
    readonly property string keysPreset: Settings.contains(Settings.PRESETS, root.state.keys)
                                         ? root.state.keys : Settings.PRESETS[0]

    // The generated table holds the live preset, so this is the whole wire between the stored value
    // and every lookup: it fires on load and on a settings change alike, and rebinds in this process.
    onKeysPresetChanged: Keymap.setPreset(root.keysPreset)

    // A whole top-level ui.json key this window changed: the same value goes into the document and
    // into the patch, because nothing else lives under it.
    function changeKey(key, value) {
        root.owe(key, UiState.withKey(root.state, key, value), UiState.withKey(root.unsaved, key, value))
    }

    // One leaf inside a group. The leaf merges into whatever else the group holds so the document
    // keeps a sub-key a newer Flea left there, and the patch carries the leaf ALONE: a sub-key this
    // Flea has no rule for is one src/uistate.rs refuses, and it refuses the whole patch with it.
    function changeLeaf(key, leaf) {
        root.owe(key, UiState.withGroup(root.state, key, leaf), UiState.withGroup(root.unsaved, key, leaf))
    }

    // Both writers' last move: hold the new document, and when the named key is not the value it
    // already held, owe the state file what changed and ask for a write. A setter that lands the
    // value already on screen owes nothing, so a chord clamped at the end of its range writes nothing.
    function owe(key, next, owed) {
        var before = JSON.stringify(root.state[key])
        root.state = next
        if (JSON.stringify(next[key]) === before)
            return
        root.unsaved = owed
        root.save()
    }

    // The Display section's writers. ui/shell.qml routes keys.toml's textSizeUp, textSizeDown and
    // textSizeReset into the same three, so a chord and a control cannot hold two different sizes.
    function setTextSize(next) {
        root.changeLeaf("display", { textSize: TextSize.parse(next) })
    }

    function followTextSize() {
        root.setTextSize(TextSize.follow())
    }

    function stepTextSize(direction) {
        root.setTextSize(TextSize.stepped(root.textSize, root.omarchyBase, direction))
    }

    function toggleTextFollow() {
        root.setTextSize(TextSize.following(root.textSize)
                         ? TextSize.pin(root.textSize, root.omarchyBase) : TextSize.follow())
    }

    // The Menus section's own two writers. Both write the hidden set alone, because the master row
    // over the six basic actions is Settings.masterState of that set rather than a value of its own.
    function setMenuHidden(hidden) {
        root.changeLeaf("menu", { hidden: hidden })
    }

    function toggleMenuAction(id) {
        root.setMenuHidden(Settings.toggleId(root.menuHidden, id))
    }

    function toggleMenuBasic() {
        root.setMenuHidden(Settings.toggleMaster(root.menuHidden))
    }

    function toggleKeyHints() {
        root.changeKey("keyHints", !root.keyHints)
    }

    function setKeysPreset(name) {
        root.changeKey("keys", name)
    }

    // Flipped by ui/Pane.qml's onChosen, when a header-menu row answers "col:<key>".
    function toggleColumn(key) {
        var shown = root.columns.slice()
        var at = shown.indexOf(key)
        if (at >= 0)
            shown.splice(at, 1)
        else
            shown.push(key)
        root.changeKey("columns", shown)
    }

    // A patch flea refused, or a state file it could not write. The pane turns it into the status
    // bar's one sentence: the change is on screen and the file does not have it.
    signal saveFailed()

    // The settings on screen are not the ones on disk: main() left a ui.json it cannot read as a
    // JSON object exactly as the operator wrote it, or the read below failed outright. Either way
    // what is drawn is the shipped defaults, and ui/PaneWire.qml is where that is said once. Only
    // ever set true, because the read that would clear it is the one that could not be taken.
    property bool unreadable: false

    // What this window has changed and no write has landed for yet, in the shape of a ui.json patch.
    // A write that lands takes its own settings out of it leaf by leaf, so a refused one keeps its
    // settings for the next patch to carry and a stored one is never carried again.
    property var unsaved: ({})

    // ui/js/UiState.js's book: the newest patch a writer landed, what the running writer carries,
    // what waits behind it. A save that would change nothing writes nothing.
    property var writeBook: UiState.book()

    // Only what this window has changed, because src/uistate.rs merges a patch key by key: a key
    // left out is one the file keeps, so saving a text size here cannot write this window's own read
    // of `keys` over a change another window or the CLI made after that read. It is every change no
    // writer has stored yet rather than the newest one alone, so a refused patch's settings ride out
    // again on the next one; a stored patch's are gone from it before the next one is built.
    function patch() {
        return JSON.stringify(root.unsaved)
    }

    function save() {
        var next = UiState.asked(root.writeBook, root.patch())
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
        if (read.unreadable)
            root.unreadable = true
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
        // A file that is not there is a first launch and says nothing; anything else is a file this
        // window could not read and is about to draw the defaults over, which is the unchecked-read
        // defect ui/NetworkDialog.qml already carried once and must not be repeated here.
        onLoadFailed: function (error) { if (error !== FileViewError.FileNotFound) root.unreadable = true }
    }

    // The writer answered, with its own status or with 2 for one that never started: the same refusal
    // to the pane and the same retry to the book, because neither reached the file.
    function wrote(exitCode) {
        // Taken out before the patch below is built, so a writer queued behind this one launches with
        // what is still owed and not with the settings this one has just stored.
        if (exitCode === 0)
            root.unsaved = UiState.acknowledged(root.unsaved, root.writeBook.inflight)
        var next = UiState.exited(root.writeBook, exitCode, root.patch())
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
