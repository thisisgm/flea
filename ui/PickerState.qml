import Quickshell
import QtQuick
import qs.Commons
import "." as Flea
import "js/DirSizes.js" as DirSizes
import "js/Filter.js" as Filter
import "js/Ops.js" as Ops
import "js/Picker.js" as Picker
import "js/PickerMarks.js" as Marks
import "js/PickerOps.js" as PickerOps
import "js/Thumbs.js" as Thumbs

// The chooser's state and the moves on it, lifted out of ui/picker.qml at the 400-line cap. The
// inverse of ui/PaneWire.qml: this owns the state and writes only through the collaborators the
// window hands in below. Every ui/Picker*.qml child takes this object as its picker.
QtObject {
    id: root

    // Handed in by ui/picker.qml: the Quickshell window, for itemRect and the reply file; the
    // backend the listing comes from; the Recent history; the footer that shows a message; the
    // typed-line navigator; the URL fetcher; and the list, whose height sizes the listing window.
    property var window: null
    property var backend: null
    property var recents: null
    property var footer: null
    property var navigate: null
    property var fetcher: null
    property var list: null
    // The rail, for the keys ui/js/RailKeys.js moves through it while focusView is "rail".
    property var places: null

    readonly property var req: Picker.request(Quickshell.env("FLEA_PICKER"))
    readonly property string home: Quickshell.env("HOME")

    // Where the list is standing, what it holds of the listing, and where the cursor is in it.
    property string path: ""
    property int total: 0
    property int held: 0
    property var rows: []
    // The per-response kind dictionary ui/Row.qml's Kind column indexes into.
    property var kindNames: []
    property int cursorIndex: 0
    property string listingState: "loading"

    // The checked identities, each a path and its size, so Back and Parent cannot rebind one.
    property var marks: []
    // The listing row the last Space or Ctrl+click toggled, where a Shift+click's range starts.
    property int markAnchor: -1
    // The user's own pills, from ~/.config/flea/filters.toml, drawn only for a caller that sent none.
    readonly property var config: Flea.PickerFilters {}
    readonly property var filters: Picker.filterList(root.req, root.config.filters)
    readonly property var chips: Picker.chips(root.req, root.config.filters)
    // Which chip is active: an index into filters, or -1 for All files.
    property int filterIndex: Picker.currentChip(root.req)
    readonly property var filter: root.filterIndex >= 0 ? root.filters[root.filterIndex] : null
    // The chip's rows and the typed query's rows, met in ui/js/Picker.js narrow; either alone otherwise.
    readonly property var shown: Picker.narrow(Picker.shownRows(root.rows, root.held, root.filter),
                                               Filter.shown(root.rows, root.held, root.filterQuery))
    readonly property int shownTotal: root.shown === null ? root.total : root.shown.length

    // Where Back goes, and it only ever goes back: Parent is its own button and pushes here too.
    property var history: []
    // The save mode's own name, which starts as the caller's suggestion only when that
    // suggestion is a filename: tools/flea-portal passes current_name through verbatim, so a
    // separator in it would put a path outside this folder in the field before anyone typed.
    property string saveName: Picker.validName(root.req.name) ? root.req.name : ""

    // SendPicker.html draws every rule and control frame in one ink, a lift over whatever plane
    // it sits on. Theme.color.surface is a drop on these palettes and vanishes against the chrome
    // strips, so the picker takes the OEM's own resting border alpha, which lifts on both.
    readonly property color edge: Style.hoverBorderColor

    // Recent is a location and not a directory: the rail's own row opens it and the listing it
    // builds comes from the desktop's history rather than a scan; see AGENTS.md "Recent, and why".
    readonly property bool recent: Picker.isRecent(root.path)

    readonly property bool saving: root.req.mode === "save"
    readonly property bool folderMode: root.req.directory || root.req.mode === "savefiles"
    readonly property bool fetching: root.fetcher !== null && root.fetcher.fetching
    readonly property int windowSize: (root.list !== null ? root.list.visibleRows : 0) + 60

    // Exactly one answer leaves this window, whichever way it is asked for.
    property bool answered: false

    // ---- the pane ui/js/Ops.js, Sort.js and Drag.js read, so they run here unmodified ----
    // ui/js/PickerOps.js turns the marks into listing indices and back. The rest is what a pane
    // holds that no chooser move reads yet, at the value a fresh ui/Pane.qml starts with.
    readonly property string viewMode: "list"
    // Which surface the keys reach, "list" or "rail", the two values ui/js/Focus.js names; Tab
    // swaps them and the rail's Escape and Enter write "list" back, see ui/js/PickerKeys.js.
    property string focusView: "list"
    property string filterQuery: ""
    property bool filterTyping: false
    property bool showHidden: false
    // Absolute paths for cut and copy. This clipboard belongs to this process, not the system.
    property var clipboard: Ops.emptyClipboard()
    property var clipPending: null
    property var pathsPending: null
    // The current trash request's paths. Its reply carries counts only, so cleanup uses this snapshot.
    property var trashPending: []
    property int renamingIndex: -1
    // The old identity stays until success can move a mark to the path the backend returns.
    property string renameFromPath: ""
    // The listing that sent the write. Recent is a token, so the source file's parent cannot name it.
    property string renameListingPath: ""
    // A click-away commit keeps the row the pointer chose, named across the async reply by path.
    property string renamePointerPath: ""
    // The directory that sent a pending mkdir. A late reply must not refresh another location.
    property string mkdirListingPath: ""
    property string renameOnArrival: ""
    property var transfer: Ops.emptyTransfer()
    // ui/Pane.qml's cap, seven screens of answered rows, so a policy bug costs memory slowly.
    readonly property int thumbCap: 240
    property var thumbState: Thumbs.empty()
    property var dirSizeState: DirSizes.empty()

    // Filter.apply prunes what a keystroke hides out of the selection through this, toggling each
    // row on a selection object the chooser does not have. A mark is a path, so nothing here can
    // rebind or vanish; while the query line is typed the answer is empty and the prune is a no-op.
    function selectedIndices() { return root.filterTyping ? [] : PickerOps.indicesFor(root) }
    function pathsFor() { return PickerOps.pathsFor(root) }
    function dropMarks(paths) { PickerOps.dropMarks(root, paths) }
    function selectAll() { PickerOps.selectAll(root) }
    function newFolder() { PickerOps.newFolder(root) }
    function clip(moving) { PickerOps.clip(root, moving) }
    function paste() { PickerOps.paste(root) }
    // A row's identity, which in Recent is the row's own path and never a join onto the token.
    function join(base, name) { return Picker.rowPath(base, name) }
    function message(text, isError) { root.footer.say(text, false, isError) }
    function sticky(text) { root.footer.sticky = text }
    // Sort.resort clears a selection because a reorder rebinds every index. A mark is a path and
    // survives a reorder the way it survives Back, so there is nothing here to clear.
    function clearSelection() {}
    function setCursor(index) {
        root.cursorIndex = index
        var view = Filter.viewOf(root.shown, index)
        if (view >= 0)
            root.showRow(view)
    }
    function showRow(view) { root.list.positionViewAtIndex(view, ListView.Contain) }
    // The re-read after one of the picker's own writes; see ui/js/PickerOps.js refresh.
    function refresh(selectPath) { PickerOps.refresh(root, selectPath) }
    function startRename() {
        if (root.renameFromPath.length === 0)
            Ops.startRename(root)
    }
    function commitRename(newName) {
        var row = root.rowFor(root.renamingIndex)
        root.renameFromPath = row ? root.join(root.path, row.n) : ""
        root.renameListingPath = root.path
        Ops.commitRename(root, newName)
    }

    function rowFor(index) {
        var at = index - root.held
        return at >= 0 && at < root.rows.length ? root.rows[at] : null
    }

    function open(next) {
        if (next === root.path)
            return
        if (root.path.length > 0)
            root.history = root.history.concat([root.path])
        root.openWithoutHistory(next)
    }

    function openWithoutHistory(next) {
        root.path = next
        root.total = 0
        root.held = 0
        root.rows = []
        root.cursorIndex = 0
        root.markAnchor = -1
        // The editor belongs to the listing being replaced. Its index must not name a new row.
        root.renamingIndex = -1
        // A filter narrows the rows already listed, so a new listing is what forgets it, ui/js/Nav.js's rule.
        Filter.close(root)
        root.thumbState = Thumbs.empty()
        root.listingState = "loading"
        if (Picker.isRecent(next)) {
            root.recents.refresh()
            return
        }
        root.backend.list(next, root.windowSize, root.showHidden)
    }

    // The . key, ui/Pane.qml's rule: the flag flips and the standing directory is listed again,
    // because the backend never sent the dotfiles and there is nothing here to unhide client-side.
    // The cursor goes back on its row by path through the refresh route, a mark being a path too;
    // a cursor on a dotfile the toggle hides lands on row 0, as after any other listing. Recent is
    // a history and not a scan, so there the key does nothing and the flag stands as it was.
    function toggleHidden() {
        if (root.recent)
            return
        root.showHidden = !root.showHidden
        var row = root.rowFor(root.cursorIndex)
        root.refresh(row ? Picker.rowPath(root.path, row.n) : "")
    }

    // A property var does not notify on an in-place mutation, so history is reassigned, never popped.
    function goBack() {
        if (root.history.length === 0)
            return
        var target = root.history[root.history.length - 1]
        root.history = root.history.slice(0, root.history.length - 1)
        root.openWithoutHistory(target)
    }

    function goUp() {
        // The board's own rule: Parent is unavailable in Recent, because a history has no parent.
        if (root.recent) {
            return
        }
        var up = Picker.parentOf(root.path)
        if (up !== root.path)
            root.open(up)
    }

    // Space. A directory is markable only when the request asked for one, and a file only when
    // it did not: the board draws no check at all on the rows the caller cannot receive.
    function toggleMark(index) {
        var row = root.rowFor(index)
        if (!row || row.d !== root.folderMode)
            return
        root.marks = Marks.toggle(root.marks, Picker.rowPath(root.path, row.n), row.s, root.req.multiple)
        root.markAnchor = index
    }

    // A plain click. The same markable rule as Space, but the mark is set and never cleared, so a
    // click on the row already checked in a single request leaves it checked.
    function selectMark(index) {
        var row = root.rowFor(index)
        if (!row || row.d !== root.folderMode)
            return
        root.marks = Marks.select(root.marks, Picker.rowPath(root.path, row.n), row.s, root.req.multiple)
        root.markAnchor = index
    }

    // Enter. A directory is always walked into, even in the folder request the board draws it
    // marked in, and a file submits what is checked: nothing checked is nothing to submit, which
    // is the board's own rule and what keeps a stray Enter from sending.
    function activate(index) {
        var row = root.rowFor(index)
        if (!row)
            return
        if (row.d) {
            root.open(Picker.rowPath(root.path, row.n))
            return
        }
        root.accept()
    }

    function accept() {
        if (root.fetching)
            return
        // The save box's line is a name or a path; ui/PickerNavigate.qml says, walks or answers.
        if (root.saving) {
            root.navigate.acceptSave(root.saveName)
            return
        }
        // A folder request with nothing checked takes the directory the window is standing in,
        // which is what the board's Choose folder button does with no row marked.
        if (root.marks.length === 0 && root.folderMode) {
            // Recent is not a directory, so there is nothing here to hand back unasked.
            if (root.recent) {
                root.say("Press Space to select a folder first")
                return
            }
            root.finish(Picker.RESPONSE_OK, [root.path])
            return
        }
        if (root.marks.length === 0) {
            root.say("Press Space to select a file first")
            return
        }
        root.finish(Picker.RESPONSE_OK, Picker.paths(root.marks))
    }

    function cancel() {
        root.finish(Picker.RESPONSE_CANCELLED, [])
    }

    // The one write out of this process. The window closes only once the reply file is on disk,
    // because tools/flea-portal reads it after this process exits and a lost write is a fault.
    // The active chip goes with a pick, so a caller that offered filters learns which one held.
    function finish(response, list) {
        if (root.answered)
            return
        // Built before the flag is set, so a throw here leaves the window answerable rather than shut.
        var text = Picker.reply(response, list, root.filter)
        root.fetcher.drop()
        root.answered = true
        root.window.replyFile.setText(text)
    }

    // A held message stays until the next say: the share legs can take their whole deadline.
    function say(text, hold) {
        root.footer.say(text, hold, false)
    }

    // The keyboard back on the listing, whichever surface it was in: a click on a row or a place
    // moves the focus the way the browser's does, and not only the Qt focus item.
    function focusList() {
        root.focusView = "list"
        root.list.forceActiveFocus()
    }

    // ui/PickerList.qml's rowCentre needs a drawn item's painted box, which only the window has.
    function itemRect(item) {
        return root.window.itemRect(item)
    }
}
