import QtQuick
import "js/Picker.js" as Picker
import "js/PickerFetch.js" as Fetch

// What the window does with a remote URL ui/PickerNavigate.qml handed on: the Windows file dialog's
// rule, which downloads the file and answers the application a local path, never the URL. The
// backend's fetch does the download, docs/protocol.md "fetch"; this holds the one fetch the dialog
// has in flight, reads every line by its id, and answers the dialog with the file the way accept()
// answers a marked one. The file stays in the picker cache: the application reads it after the
// answer, and the sweep at the next --pick is what removes it.
QtObject {
    id: root

    property var picker: null
    property var backend: null

    // The URL asked for, until its fetchdone; fetchId is -1 until fetchstarted names the fetch.
    property string url: ""
    property int fetchId: -1
    readonly property bool fetching: root.url.length > 0
    property string leaf: ""
    property real bytes: 0
    property real total: 0
    // Escape came before fetchstarted did, so the cancel goes out the moment the id is known.
    property bool cancelling: false

    readonly property string line: Fetch.line(root.leaf, root.bytes, root.total)
    readonly property real fraction: Fetch.fraction(root.bytes, root.total)

    function enter(answer) {
        // One fetch at a time: a second Return while it runs is nothing, not a queue.
        if (root.fetching) {
            return
        }
        var refused = Fetch.refusal(answer, root.picker.folderMode, root.picker.saving)
        if (refused.length > 0) {
            root.picker.say(refused)
            return
        }
        root.url = answer.url
        root.leaf = Fetch.leafOf(answer.url)
        root.bytes = 0
        root.total = 0
        root.fetchId = -1
        root.cancelling = false
        root.backend.fetch(answer.url)
    }

    // Escape. Answers whether the key was spent on a fetch, so the window's own Escape, the
    // dialog's refusal, runs only when nothing is being fetched.
    function takeEscape() {
        if (!root.fetching) {
            return false
        }
        root.cancel()
        return true
    }

    function cancel() {
        if (root.fetchId < 0) {
            root.cancelling = true
            return
        }
        root.backend.fetchCancel(root.fetchId)
    }

    // The dialog is answering, by whichever door: a fetch still running is cancelled first, so no
    // download outlives the window that asked for it. A fetch that answered has nothing to cancel.
    function drop() {
        if (root.fetching) {
            root.cancel()
        }
    }

    // The started line is matched on the URL, because the id is not known before it arrives.
    function started(id, uri) {
        if (!root.fetching || root.fetchId >= 0 || uri !== root.url) {
            return
        }
        root.fetchId = id
        if (root.cancelling) {
            root.backend.fetchCancel(id)
        }
    }

    function progressed(id, bytes, total) {
        if (id !== root.fetchId) {
            return
        }
        root.bytes = bytes
        root.total = total
    }

    // The terminal line. The fetch is over before the window is told, so a say() or a finish() that
    // reads back into this object finds nothing in flight.
    function finished(id, ok, path, err) {
        if (id !== root.fetchId) {
            return
        }
        root.url = ""
        root.fetchId = -1
        if (ok) {
            root.picker.finish(Picker.RESPONSE_OK, [path])
            return
        }
        root.picker.message(Fetch.failure(err), true)
    }

    Component.onCompleted: {
        root.backend.fetchStarted.connect(root.started)
        root.backend.fetchProgress.connect(root.progressed)
        root.backend.fetchDone.connect(root.finished)
    }
}
