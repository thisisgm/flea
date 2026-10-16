pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "js/Density.js" as Density
import "js/Format.js" as Format
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

    // A key hides the listing's hover until the pointer moves, as the Omarchy menu does; a row moved under a still pointer is no move.
    property bool pointerQuiet: false
    property point pointerAt: Qt.point(-1, -1)
    // Sample input: a Qt.Key_J press quiets; a bare Shift press does not.
    function pointerKeyed(event) {
        var bare = [Qt.Key_Shift, Qt.Key_Control, Qt.Key_Alt, Qt.Key_AltGr, Qt.Key_Meta, Qt.Key_Super_L, Qt.Key_Super_R]
        if (bare.indexOf(event.key) < 0) root.pointerQuiet = true
    }
    // Sample input: the scene point a row's HoverHandler reports, within a pixel of the last one while a key holds it quiet.
    function pointerMoved(scene) {
        if (root.pointerQuiet && Math.abs(scene.x - root.pointerAt.x) <= 1 && Math.abs(scene.y - root.pointerAt.y) <= 1) return
        root.pointerAt = scene
        root.pointerQuiet = false
    }

    // Mirrors "columns" in src/uischema.rs, and is the only default this front end needs before the
    // first frame: it is what a first launch draws, when there is no file to settle and none to read.
    readonly property var defaultColumns: ["name", "size", "date"]

    // Mirrors "menu"."hidden" in src/uischema.rs, for the same first launch: six ids this release's
    // menu cannot build, plus Copy as and Open in terminal, which the SettingsMenus board ships
    // switched off. Every id here is an action ui/js/Menu.js gives a row, or will give one.
    // Mirrors src/uischema.rs DEFAULTS menu.hidden exactly; the two drifted once and a fresh
    // ui.json then hid a row the shipped schema shows.
    readonly property var defaultMenuHidden: ["delete", "openTerminal", "placeMenu", "runScript",
                                               "moveto", "copyto", "properties", "permissions", "copyAs",
                                               "pasteAs", "invertSelection", "extThumbs", "run", "openAsText"]

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
    // "Enable shelf" is one switch, so the Menus row is not a stored menu id at all.
    // It reads the shelf's own boolean, which is what src/shelfplugin.rs installs the bar plugin on.
    readonly property bool shelfEnabled: (root.state.shelf || ({})).enabled === true
    readonly property var menuHidden: (Array.isArray(root.menu.hidden) ? root.menu.hidden
                                                                       : root.defaultMenuHidden)
                                      .filter(function (id) { return id !== "shelf" })
                                      .concat(root.shelfEnabled ? [] : ["shelf"])

    // The Menus section's "Show keyboard hints" row, `keyHints` in src/uischema.rs. It draws the key
    // beside every menu row and the tip under an empty directory, and it binds no key of its own:
    // every chord answers whether this is on or off.
    // === true, not !== false: an absent key is off, which is what src/uischema.rs stores and what
    // driveSize and trashCount beside it already read. The other way round drew hints from a state
    // file that never named them.
    readonly property bool keyHints: root.state.keyHints === true

    // Code icons default on; only a saved boolean false disables them, matching schema healing.
    readonly property bool langMarks: root.state.langMarks !== false
    function toggleLangMarks() { root.changeKey("langMarks", !root.langMarks) }

    // Absent reads off, the way keyHints beside it does.
    readonly property bool highlightToday: root.state.highlightToday === true

    // Settings > View > Open folders while dragging: absent reads off, like highlightToday.
    readonly property bool openFoldersWhileDragging: root.state.openFoldersWhileDragging === true
    // Rows compare one number and never build a Date.
    property double todayStart: Format.dayStart(Date.now())

    // One timer steps to the next local midnight while the switch is on, in minute steps so a suspend still lands the day within a minute of resume; rows never tick.
    property var midnightTimer: Timer {
        repeat: true
        running: root.highlightToday
        interval: Math.min(Format.MINUTE_MS, Format.msUntilMidnight(Date.now()))
        onTriggered: {
            root.todayStart = Format.dayStart(Date.now())
            root.midnightTimer.interval = Math.min(Format.MINUTE_MS, Format.msUntilMidnight(Date.now()))
            root.midnightTimer.running = root.highlightToday
        }
    }
    onHighlightTodayChanged: {
        root.todayStart = Format.dayStart(Date.now())
        root.midnightTimer.interval = Math.min(Format.MINUTE_MS, Format.msUntilMidnight(Date.now()))
        root.midnightTimer.running = root.highlightToday
    }

    // The listing's view, "view" in src/uischema.rs, which ui/Pane.qml draws for the first frame so
    // the view the window was left on is the view the next launch opens on. A word this build cannot
    // draw reads as the list, which covers both a hand edit made while the window is up and the
    // stored "dual", a window shape and not a view a pane can be in. PR 97, DouglasdeMoura.
    readonly property string view: Settings.contains(["list", "columns", "grid"], root.state.view)
                                   ? root.state.view : "list"
    // Per-folder views stay separate from the fallback above and off until opted in, so an existing ui.json keeps its global view.
    readonly property bool rememberViewPerFolder: root.state.rememberViewPerFolder === true
    readonly property var folderViews: root.state.folderViews || ({})
    // The chooser's own last-used view (`pickerView` in src/uischema.rs); an undrawable word reads as the list.
    readonly property string pickerView: Settings.contains(["list", "grid"], root.state.pickerView)
                                         ? root.state.pickerView : "list"

    // The Keys section's four-value chooser over the one generated key table, falling back to its
    // first value, Default, which is what SettingsKeys.html says a missing or unknown name means.
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

    // Changed entries of a map key: the document draws the whole map, the patch owes those entries alone.
    function changeMapEntries(key, entries, full) {
        root.owe(key, UiState.withKey(root.state, key, full), UiState.withGroup(root.unsaved, key, entries))
    }

    // Both writers' last move: hold the new document, and when the named key is not the value it
    // already held, owe the state file what changed and ask for a write. A setter that lands the
    // value already on screen owes nothing, so a chord clamped at the end of its range writes nothing.
    function owe(key, next, owed) {
        var before = JSON.stringify(root.state[key])
        if (JSON.stringify(next[key]) === before)
            return
        root.state = next
        root.unsaved = owed
        root.saveStatus = "Saving…"
        root.save()
    }

    // RailAdditions rule 4: ctrl-b remembers the rail the way the view is remembered, so this is a
    // state and not a setting; the width rule in ui/Pane.qml hides it without writing anything.
    readonly property bool railHidden: (root.state.places || ({})).rail === "hidden"
    // Directive 74: hiding on a narrow window is its own switch and it ships off, so the rail stays
    // where it is at every width until somebody asks for the other behaviour.
    readonly property bool railAutoHide: (root.state.places || ({})).autoHide === true
    function toggleRail() { root.changeLeaf("places", { rail: root.railHidden ? "shown" : "hidden" }) }

    readonly property var preview: root.state.preview || ({})
    readonly property bool previewColumn: root.preview.column !== false
    readonly property bool previewAutomatic: root.preview.loadOn !== "manual"
    // `previewFacts` in src/uischema.rs: the tall kinds' one compact line, or the full table `i` opens.
    readonly property string previewFacts: root.state.previewFacts === "full" ? "full" : "compact"
    readonly property string thumbnailMode: root.preview.thumbnails || "media"
    readonly property string thumbnailSize: root.preview.thumbSize || "medium"
    // Huge 192 and Largest 256 decode nothing new: the shared cache already holds 256.
    readonly property int thumbnailPixels: Density.thumbPixelsFor(root.thumbnailSize)
    readonly property bool ctrlZoom: root.preview.ctrlZoom !== false
    // `preview.volume` in src/uischema.rs, 0 to 100: the level ui/MediaSound.qml plays every player at, 100 being full scale.
    readonly property int mediaVolume: root.preview.volume !== undefined ? root.preview.volume : 100
    function setMediaVolume(level) { root.changeLeaf("preview", { volume: level }) }
    // `preview.width` in src/uischema.rs: the Columns view's last column as a whole percent of the area, 0 for the default share.
    readonly property int previewWidth: root.preview.width || 0
    function setPreviewWidth(percent) { root.changeLeaf("preview", { width: percent }) }
    // Quick Look's w: long lines wrap in a code or text file; off until pressed, and remembered.
    readonly property bool previewWrap: root.preview.wrap === true
    readonly property string density: root.state.density || "compact"
    readonly property string addressBar: root.state.addressBar || "breadcrumb"
    readonly property bool hyprlandIcons: root.display.hyprlandIcons === true
    property string saveStatus: "Saved"

    // Settings > View > Opening. ui/js/Startup.js is what turns these into a path; nothing else reads
    // them, so the panel and the two openers can never disagree about where a window or a tab begins.
    readonly property string startFolder: root.state.startFolder || ""

    // Settings > Places > Trash. The sweep reads both; ui/TrashHost.qml is its only driver.
    readonly property bool trashAutoEmpty: root.state.trashAutoEmpty === true
    readonly property int trashSweptOn: root.state.trashSweptOn || 0

    // Settings > About's "Check automatically", `updates.autoCheck` in src/uischema.rs, on until switched off.
    readonly property bool updateAutoCheck: (root.state.updates || ({})).autoCheck !== false

    // Only a successful sweep records its day, so failures retry on the next launch.
    function recordTrashSweep(day) {
        root.changeKey("trashSweptOn", day)
    }

    // Choosing the panel's current folder also selects the folder start mode.
    function setStartFolder(path) {
        root.changeKey("startIn", "folder")
        root.changeKey("startFolder", String(path || ""))
    }

    // Navigation owes only changed keys, then writes the last folder and ordered tab strip together.
    function rememberNavigation(path, tabs) {
        var next = root.state
        var owed = root.unsaved
        var values = { lastPath: String(path || ""), lastTabs: tabs }
        for (var key in values) {
            if (JSON.stringify(next[key]) === JSON.stringify(values[key])) continue
            next = UiState.withKey(next, key, values[key])
            owed = UiState.withKey(owed, key, values[key])
        }
        if (next === root.state) return
        root.state = next
        root.unsaved = owed
        root.saveStatus = "Saving…"
        root.save()
    }

    // Setting ids name either one top-level key or one leaf of an existing group.
    function changeSetting(id, value) {
        // The rail is remembered as a word, so its own Places row writes that word and not a boolean.
        if (id === "places.rail") { root.changeLeaf("places", { rail: value ? "shown" : "hidden" }); return }
        var parts = id.split(".")
        if (parts.length === 1) {
            root.changeKey(id, value)
        } else {
            var leaf = {}
            leaf[parts[1]] = value
            root.changeLeaf(parts[0], leaf)
        }
    }

    // The Display controls and text-size chords share these writers.
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

    // The Menus section's own two writers. Both write the hidden set alone, because a group's master is Settings.masterState of that set rather than a value of its own.
    function setMenuHidden(hidden) {
        root.changeLeaf("menu", { hidden: hidden.filter(function (id) { return id !== "shelf" }) })
    }

    function toggleMenuAction(id) {
        if (id === "shelf") { root.changeLeaf("shelf", { enabled: !root.shelfEnabled }); return }
        root.setMenuHidden(Settings.toggleId(root.menuHidden, id))
    }

    // A master that turns its whole group on turns the shelf on with it, and the same the other way.
    function toggleMenuGroup(ids) {
        if (ids.indexOf("shelf") >= 0)
            root.changeLeaf("shelf", { enabled: Settings.masterState(root.menuHidden, ids) !== "all" })
        root.setMenuHidden(Settings.toggleMaster(root.menuHidden, ids))
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
    signal favouritesReadFailed(string message)
    property string favouritesReadError: ""

    // The settings on screen are not the ones on disk: main() left a ui.json it cannot read as a
    // JSON object exactly as the operator wrote it, or the read below failed outright. Either way
    // what is drawn is the shipped defaults, and ui/PaneWire.qml is where that is said once. Only
    // ever set true, because the read that would clear it is the one that could not be taken.
    property bool unreadable: false
    property bool initialReadComplete: false

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
        root.writeRevision += 1
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
    Component.onCompleted: { root.load(stateFile.text()); root.initialReadComplete = true }

    function load(text) {
        var read = UiState.fromFile(text)
        root.state = read.state
        if (read.unreadable)
            root.unreadable = true
    }
    function syncFavourites(text) {
        var read = UiState.refreshedFavourites(root.state, text)
        if (read.error && read.error !== root.favouritesReadError) root.favouritesReadFailed(read.error)
        root.favouritesReadError = read.error
        if (read.state !== root.state) root.state = read.state
        return read.error.length === 0
    }

    // A change another window saved: the settled document with no argument, merged and validated, writes nothing.
    property string settleMode: ""
    // Local saves and acknowledgements invalidate older snapshots, even after their owed keys clear.
    property double writeRevision: 0
    property double settleRevision: 0
    property bool settleDirty: false
    property string pruneInflight: ""
    property string pruneQueued: ""
    property var settleAnswer: ({})
    function applyShared(text) {
        if (!UiState.parsesAsObject(text))
            return
        if (UiState.settleBusy(settler.running, root.settleMode, root.settleAnswer)) {
            root.settleDirty = true
            return
        }
        root.settleStart("apply")
    }
    // One starter for both settle kinds, so a prune never inherits a stale exit or text.
    function settleStart(mode) {
        // A half-landed settle is still pending, so starting over it would lose the half already here.
        if (UiState.settleBusy(settler.running, root.settleMode, root.settleAnswer))
            return
        root.settleMode = mode
        root.settleRevision = root.writeRevision
        root.settleAnswer = {}
        settler.command = [Quickshell.env("FLEA_BIN") || "flea", "--ui-state"]
        settler.running = true
    }
    // A settler's exit and its collected text land in either order, so only a whole answer reaches settleDone.
    function settleLanded(half) {
        if (UiState.whole(root.settleAnswer))
            return
        root.settleAnswer = UiState.landed(root.settleAnswer, half)
        if (!UiState.whole(root.settleAnswer))
            return
        root.settleDone(root.settleAnswer.code, root.settleAnswer.text)
    }
    // A settle's end spends a queued prune before a dirty re-read; the dirty flag survives the prune so the re-read still runs after it.
    function settleNext() {
        // Keep the prune queued until the process is idle and both reply halves have landed.
        if (UiState.settleBusy(settler.running, root.settleMode, root.settleAnswer))
            return
        var next = UiState.settleNext(root.settleDirty, root.pruneQueued)
        if (next === "prune") {
            var queued = root.pruneQueued
            root.pruneQueued = ""
            root.pruneInflight = queued
            root.settleStart("prune")
        } else if (next === "apply") {
            root.settleDirty = false
            root.applyShared(stateFile.text())
        }
    }
    function settleDone(exitCode, out) {
        var mode = root.settleMode
        root.settleMode = ""
        if (mode === "prune") {
            if (exitCode !== 0) { root.pruneFailed(); return }
            var settled = null
            try {
                settled = JSON.parse(out)
            } catch (e) {
                root.pruneFailed()
                return
            }
            if (!UiState.parsesAsObject(stateFile.text())) { root.pruneFailed(); return }
            root.unsaved = UiState.dropInvalid(root.unsaved, root.pruneInflight)
            root.state = UiState.revertedState(root.state, root.pruneInflight, settled)
            root.finishPrune(false)
            return
        }
        if (root.settleRevision !== root.writeRevision) {
            root.settleDirty = true
            root.settleNext()
            return
        }
        if (exitCode !== 0) { root.settleNext(); return }
        if (!UiState.parsesAsObject(out)) { root.settleNext(); return }
        if (!UiState.parsesAsObject(stateFile.text())) { root.settleNext(); return }
        var merged = UiState.applyExternal(root.state, root.unsaved, out)
        if (merged.changed)
            root.state = merged.state
        root.syncFavourites(stateFile.text())
        root.settleNext()
    }
    // One tail for both prune endings: end the settle, then save what is still owed.
    function finishPrune(failed) {
        root.pruneInflight = ""
        root.writeBook = { saved: root.writeBook.saved, inflight: "", pending: "", start: "" }
        if (failed)
            root.saveFailed()
        root.settleNext()
        if (JSON.stringify(root.unsaved) !== "{}") {
            root.saveStatus = "Saving…"
            root.save()
        }
    }
    // A prune settle that never answered still drops what the schema refused, so no save strands behind a phantom writer.
    function pruneFailed() {
        root.unsaved = UiState.dropInvalid(root.unsaved, root.pruneInflight)
        root.finishPrune(true)
    }
    function refreshFavourites() {
        stateFile.reload()
        // blockLoading covers only the first read; a reload otherwise returns the previous document.
        stateFile.waitForJob()
        if (!stateFile.loaded || root.favouritesReadError.length > 0) return false
        var text = stateFile.text()
        return root.syncFavourites(text)
    }

    // blockLoading, because the first list draws from this: an async read would paint one column
    // set and correct it. printErrors off, because a missing file is what a first launch looks like.
    property var store: FileView {
        id: stateFile
        path: (Quickshell.env("XDG_STATE_HOME") && Quickshell.env("XDG_STATE_HOME").length > 0
               ? Quickshell.env("XDG_STATE_HOME") : Quickshell.env("HOME") + "/.local/state") + "/flea/ui.json"
        blockLoading: true
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            root.favouritesReadError = ""
            if (root.initialReadComplete) {
                root.applyShared(text())
            }
            root.syncFavourites(text())
        }
        printErrors: false
        // A file that is not there is a first launch and says nothing; anything else is a file this
        // window could not read and is about to draw the defaults over, which is the unchecked-read
        // defect ui/NetworkDialog.qml already carried once and must not be repeated here.
        onLoadFailed: function (error) {
            if (error === FileViewError.FileNotFound && !root.initialReadComplete) return
            if (!root.initialReadComplete) root.unreadable = true
            root.favouritesReadError = UiState.FAVOURITES_UNREAD
            root.favouritesReadFailed(root.favouritesReadError)
        }
    }

    // The writer answered, with its own status or with 2 for one that never started: the same refusal
    // to the pane and the same retry to the book, because neither reached the file. A refusal never
    // leaves a key owed forever: the owed keys the settled document disagrees with are dropped once,
    // said once in the status line, so a later valid write applies again in the same process.
    function wrote(exitCode) {
        // Taken out before the patch below is built, so a writer queued behind this one launches with
        // what is still owed and not with the settings this one has just stored.
        root.saveStatus = exitCode === 0 ? "Saved"
            : "Could not save settings · changes apply to this session only"
        if (exitCode === 0) {
            root.writeRevision += 1
            root.unsaved = UiState.acknowledged(root.unsaved, root.writeBook.inflight)
        }
        var failedPatch = root.writeBook.inflight
        var next = UiState.exited(root.writeBook, exitCode, root.patch())
        root.writeBook = next
        if (next.failed)
            root.saveFailed()
        if (exitCode !== 0 && UiState.isPatchInvalid(failedPatch)) {
            // A refused patch prunes behind a pending settle instead of hijacking that settle's mode.
            var ask = UiState.pruneAsk(UiState.settleBusy(settler.running, root.settleMode, root.settleAnswer), failedPatch)
            if (ask.queue.length > 0) {
                root.pruneQueued = ask.queue
                return
            }
            root.pruneInflight = ask.start
            root.settleStart("prune")
            return
        }
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

    property var settler: Process {
        id: settler
        stdout: StdioCollector { id: settleOut; waitForEnd: true; onStreamFinished: root.settleLanded({ text: settleOut.text }) }
        onExited: function (exitCode, exitStatus) { root.settleLanded({ code: exitCode }) }
        // A settler that never started raises no exited, only running going false; the joined halves tell it from a real exit the way the writer's own rule does.
        onRunningChanged: {
            if (root.settleMode.length > 0 && UiState.neverRan(root.settleAnswer, settler.running))
                root.settleLanded(UiState.NEVER_RAN)
            // onExited may land both halves while running is true; the idle transition resumes its queued work.
            if (!settler.running && root.settleMode.length === 0)
                root.settleNext()
        }
    }
}
