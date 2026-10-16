pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "." as Flea

Column {
    id: root
    property string path: ""
    property var face: null
    property bool installed: false
    property bool expanded: false
    property bool busy: false
    property var targets: []
    property string message: ""
    property string reply: ""
    property string requestedPath: ""
    width: parent ? parent.width : 0
    spacing: Theme.spacing.gap
    function run(verb, target) {
        if (root.busy) return
        root.busy = true
        root.reply = ""
        root.requestedPath = root.path
        var bin = Quickshell.env("FLEA_BIN") || "flea"
        child.command = verb === "defaults" ? [bin, "--font", "defaults"]
            : verb === "restore" ? [bin, "--font", "restore", target]
            : [bin, "--font", "apply", root.path, String(root.face.index), target]
        child.running = true
    }
    function complete(code) {
        if (!root.busy) return
        root.busy = false
        try {
            var data = JSON.parse(root.reply)
            if (root.path !== root.requestedPath) { root.run("defaults", ""); return }
            if (data.error) root.message = data.error
            else { root.targets = data.targets || []; root.message = data.notice || "" }
        } catch (e) { root.message = "Could not read font preferences." }
    }
    onExpandedChanged: if (expanded) root.run("defaults", "")
    Flea.DialogButton {
        objectName: "font-default-toggle"
        label: root.expanded ? "Hide default font settings" : "Use as default…"
        tabStop: true
        onActivated: root.expanded = !root.expanded
    }
    Text {
        width: parent.width
        visible: root.expanded
        text: root.message || (!root.installed ? "Install this font to use it as a default." : "Choose a destination. Terminals require a monospaced font. Changes apply to your user.")
        wrapMode: Text.WordWrap
        textFormat: Text.PlainText
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
    }
    Repeater {
        model: root.expanded ? root.targets : []
        delegate: Column {
            id: targetRow
            required property var modelData
            width: root.width
            visible: modelData.available || modelData.canRestore
            spacing: Theme.spacing.gap
            Text {
                width: parent.width
                text: targetRow.modelData.label + (targetRow.modelData.family ? " · " + targetRow.modelData.family : "")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
            }
            Flow {
                width: parent.width
                spacing: Theme.spacing.gap
                Flea.DialogButton {
                    objectName: "font-apply-" + targetRow.modelData.id
                    label: "Apply"
                    tabStop: true
                    available: !root.busy && root.installed && root.face !== null && targetRow.modelData.available
                        && (!targetRow.modelData.monospace || root.face.monospace === true)
                    onActivated: root.run("apply", targetRow.modelData.id)
                }
                Flea.DialogButton {
                    objectName: "font-restore-" + targetRow.modelData.id
                    label: "Restore previous"
                    tabStop: true
                    available: !root.busy && targetRow.modelData.canRestore
                    onActivated: root.run("restore", targetRow.modelData.id)
                }
            }
        }
    }
    Process {
        id: child
        stdout: StdioCollector { onStreamFinished: root.reply = this.text }
        onExited: function(code) { root.complete(code) }
        onRunningChanged: if (!running && root.busy) Qt.callLater(function() { if (!child.running) root.complete(2) })
    }
}
