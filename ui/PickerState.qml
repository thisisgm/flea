import Quickshell
import QtQuick
import qs.Commons
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
    // Which chip is active: an index into the caller's filters, or -1 for All files.
    property int filterIndex: Picker.currentChip(root.req)
    readonly property var filter: root.filterIndex >= 0 ? root.req.filters[root.filterIndex] : null
    readonly property var shown: Picker.shownRows(root.rows, root.held, root.filter)
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
    readonly property string focusView: "list"
    property string filterQuery: ""
    property bool filterTyping: false
    property bool showHidden: false
    property var clipboard: Ops.emptyClipboard()
    property var clipPending: null
    property var pathsPending: null
    property int renamingIndex: -1
    property string renameOnArrival: ""
    property var transfer: Ops.emptyTransfer()
    property var thumbState: Thumbs.empty()
    property var dirSizeState: DirSizes.empty()

    function selectedIndices() { return PickerOps.indicesFor(root) }
    function pathsFor() { return PickerOps.pathsFor(root) }
    function dropMarks(paths) { PickerOps.dropMarks(root, paths) }
    function selectAll() { PickerOps.selectAll(root) }
    // A row's identity, which in Recent is the row's own path and never a join onto the token.
    function join(base, name) { return Picker.rowPath(base, name) }
    function message(text, isError) { root.say(text) }
    function sticky(text) { root.say(text, true) }
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
        root.listingState = "loading"
        if (Picker.isRecent(next)) {
            root.recents.refresh()
            return
        }
        root.backend.list(next, root.windowSize, false)
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
    function finish(response, list) {
        if (root.answered)
            return
        // Built before the flag is set, so a throw here leaves the window answerable rather than shut.
        var text = Picker.reply(response, list)
        root.fetcher.drop()
        root.answered = true
        root.window.replyFile.setText(text)
    }

    // A held message stays until the next say: the share legs can take their whole deadline.
    function say(text, hold) {
        root.footer.say(text, hold)
    }

    // ui/PickerList.qml's rowCentre needs a drawn item's painted box, which only the window has.
    function itemRect(item) {
        return root.window.itemRect(item)
    }
}
