import QtQuick
import "js/Filter.js" as Filter
import "js/PickerEntry.js" as PickerEntry
import "js/PickerNavigate.js" as Navigate

// What the window does with a line ui/PickerEntry.qml reported: the Windows filename box's rules,
// decided in ui/js/PickerNavigate.js and carried out here against the window, its backend and its
// list. Nothing opens or selects before the backend has peeked the parent, and a stale peek for
// another parent is never read as this line's answer.
QtObject {
    id: root

    property var picker: null
    property var backend: null
    property var entry: null
    property var list: null

    // The typed path whose parent is being peeked, and the file a coming rows response should land
    // the cursor on, with the listing it belongs to and the window start it was asked for in.
    property string awaiting: ""
    property string awaitingParent: ""
    property bool awaitingDir: false
    property string pendingSelect: ""
    property string pendingParent: ""
    property int pendingStart: 0

    // A URL the later changes fetch; until then the window only says so.
    signal remoteEntered(var answer)
    // An smb, sftp or ssh URL, which ui/PickerShare.qml mounts and hands back through act.
    signal shareEntered(var answer)

    function enter(text) {
        // Recent is a history and not a directory, so a relative name resolves against home there.
        var current = root.picker.recent ? root.picker.home : root.picker.path
        root.act(PickerEntry.classify(text, current, root.picker.home))
    }

    function refusal(reason) {
        return Navigate.refusal(reason)
    }

    // A classified line, from the field or from ui/PickerShare.qml once a share's FUSE path is known.
    function act(answer) {
        var step = Navigate.plan(answer, root.picker.path, root.picker.marks, root.picker.folderMode)
        if (step.step === "say") {
            root.picker.say(step.message)
        } else if (step.step === "remote") {
            root.picker.say(Navigate.NOT_YET)
            root.remoteEntered(answer)
        } else if (step.step === "share") {
            root.shareEntered(answer)
        } else if (step.step === "accept") {
            root.picker.accept()
        } else if (step.step === "settle") {
            root.landed()
        } else if (step.step === "peek") {
            root.awaiting = step.path
            root.awaitingParent = step.parent
            root.awaitingDir = answer.wantsDir
            root.backend.peek(step.parent, Navigate.PEEK_ROWS, true)
        }
    }

    // Hidden rows are asked for so a dotfile resolves; the window's own listing never shows them.
    function peeked(path, hidden, total, rows, readFailed) {
        if (root.awaiting.length === 0 || path !== root.awaitingParent) {
            return
        }
        var target = root.awaiting
        root.awaiting = ""
        var step = Navigate.verdict(target, root.awaitingDir, root.picker.folderMode,
                                    { total: total, rows: rows, readFailed: readFailed })
        if (step.step === "say") {
            root.picker.say(step.message)
        } else if (step.step === "open") {
            root.picker.open(step.path)
            root.landed()
        } else if (step.step === "openSay") {
            root.picker.open(step.parent)
            root.landed()
            root.picker.say(step.message)
        } else if (step.step === "select") {
            root.select(step.parent, step.path, step.at)
        }
    }

    // A directory is open: the line has done its work, so the box clears and the list has the keys.
    function landed() {
        root.entry.text = ""
        root.list.forceActiveFocus()
    }

    // The file's row is landed on in the rows the window holds when it is standing in the parent
    // already, and otherwise once the parent's listing arrives. A listing holds only a window of
    // rows, so when the row lies past the first one the window around it is asked for as well, at
    // the index the peek said, and the response that starts there is the one read.
    function select(parent, path, at) {
        root.pendingSelect = path
        root.pendingParent = parent
        root.pendingStart = Navigate.windowStart(at, root.picker.windowSize)
        if (parent === root.picker.path) {
            var index = Navigate.indexOf(root.picker.rows, root.picker.held, root.picker.path, path)
            if (index >= 0) {
                root.pendingSelect = ""
                root.landOn(index, path)
                return
            }
            root.backend.window(root.pendingStart, root.picker.windowSize)
            return
        }
        root.picker.open(parent)
        if (root.pendingStart > 0) {
            root.backend.window(root.pendingStart, root.picker.windowSize)
        }
    }

    // Called by the window from every rows response. The target is found in whichever response
    // holds it, given up on when the window asked for arrives without it, and dropped when the
    // window has moved to another directory, so a later listing never re-reveals it.
    function rowsArrived() {
        if (root.pendingSelect.length === 0) {
            return
        }
        if (root.picker.path !== root.pendingParent) {
            root.pendingSelect = ""
            return
        }
        var index = Navigate.indexOf(root.picker.rows, root.picker.held, root.picker.path, root.pendingSelect)
        if (index >= 0) {
            var target = root.pendingSelect
            root.pendingSelect = ""
            root.landOn(index, target)
            return
        }
        if (root.picker.held !== root.pendingStart) {
            return
        }
        root.pendingSelect = ""
        root.picker.say(Navigate.NOT_LISTED)
    }

    // The mark is the typed file alone, the box's own rule; the text stays in the field so a second
    // Return on it accepts. A chip that hides the row gives way to All files, so the mark is seen.
    function landOn(index, path) {
        var row = root.picker.rowFor(index)
        root.picker.cursorIndex = index
        root.picker.marks = [{ path: path, bytes: row.s }]
        root.picker.markAnchor = index
        var view = Filter.viewOf(root.picker.shown, index)
        if (view < 0) {
            root.picker.filterIndex = -1
            view = index
        }
        root.list.positionViewAtIndex(view, ListView.Contain)
    }
}
