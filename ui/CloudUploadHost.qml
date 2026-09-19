import QtQuick

// Separate from the other menu dialogs: they must never unload an owned transfer.
Loader {
    id: root
    anchors.fill: parent
    z: 3
    active: false
    source: "CloudUploadDialog.qml"
    readonly property bool opened: item !== null && item.opened
    readonly property var snapshot: item ? item.acceptance : ({state: "unloaded"})
    function open(paths, holder) {
        if (!paths || paths.length !== 1) return false
        active = true
        item.open(paths[0], holder)
        return true
    }
}
