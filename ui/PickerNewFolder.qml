pragma ComponentBehavior: Bound
import QtQuick
import "." as Flea
import "js/Picker.js" as Picker

// Owns a captured parent and one request at a time. The listing backend remains read-only.
Item {
    id: root
    required property var picker
    property bool active: false
    property string folder: ""
    property string name: ""
    property string error: ""
    property int preparing: 0
    property int pending: 0
    property int review: 0
    property bool committed: false
    property var previousFocus: null
    readonly property bool ready: root.review > 0 && !root.picker.backendUnavailable
    readonly property bool available: root.picker.saving && !root.active && !root.picker.backendUnavailable
        && !root.picker.submitting && !root.picker.recent && !root.picker.listingFailed
        && root.picker.listingState !== "loading" && root.picker.path.charAt(0) === "/"
    readonly property var fieldItem: card.item ? card.item.fieldItem : null
    signal created(string path)
    anchors.fill: parent
    visible: root.active
    z: 20

    function open(from) {
        if (!root.available) return
        root.folder = root.picker.path
        root.previousFocus = from
        root.name = ""
        root.error = ""
        root.review = 0
        root.committed = false
        root.active = true
        root.preparing = root.picker.check({op: "folderPrepare", folder: root.folder})
    }
    function close(restore) {
        if (!root.active || root.pending) return false
        root.active = false
        root.preparing = 0
        root.review = 0
        if (!root.committed && !root.picker.backendUnavailable) root.picker.check({op: "folderClose"})
        var before = root.previousFocus
        root.previousFocus = null
        if (restore !== false) Qt.callLater(function() {
            if (before && before.visible && before.enabled) before.forceActiveFocus()
            else root.picker.focusView()
        })
        return true
    }
    function submit() {
        if (!root.active || root.pending || root.preparing || root.committed || root.picker.backendUnavailable) return
        if (root.picker.path !== root.folder) { root.error = "The location changed; reopen New folder."; root.review = 0; return }
        if (!root.ready) {
            root.error = ""
            root.preparing = root.picker.check({op: "folderPrepare", folder: root.folder})
            return
        }
        if (!Picker.validName(root.name)) {
            root.error = root.name.length === 0 ? "Enter a folder name."
                : root.name === "." || root.name === ".." ? "Choose a name other than . or .."
                : root.name.indexOf("/") >= 0 ? "Use a single folder name, without /." : "A name cannot contain a null character."
            return
        }
        root.error = ""
        root.pending = root.picker.check({op: "folderCreate", review: root.review, name: root.name})
    }
    function received(message) {
        if (!root.active) return
        if (message.id === root.preparing) {
            root.preparing = 0
            if (!message.ok) { root.error = message.error; return }
            root.review = message.review
        } else if (message.id === root.pending) {
            root.pending = 0
            if (!message.ok) { root.error = message.error; if (card.item) card.item.focusName(); return }
            root.review = 0
            root.committed = true
            if (!message.path || root.picker.path !== root.folder) {
                root.error = message.error || "The folder was created, but the save location changed. Reopen it to find the folder."
                return
            }
            var path = message.path
            root.close(false)
            root.created(path)
        }
    }
    Connections {
        target: root.picker
        function onPathChanged() { if (root.active && !root.pending) root.close() }
        function onBackendUnavailableChanged() {
            if (!root.active || !root.picker.backendUnavailable) return
            root.pending = 0
            root.preparing = 0
            root.review = 0
            root.error = "Flea stopped responding. Cancel and reopen the save dialog."
        }
    }
    Loader {
        id: card
        anchors.fill: parent
        active: root.active
        sourceComponent: Flea.PickerFolderDialog { controller: root }
    }
}
