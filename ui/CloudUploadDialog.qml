import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format

FocusScope {
    id: root
    anchors.fill: parent
    property bool opened: false
    property string sourcePath: ""
    property Item focusHolder: null
    property int targetIndex: 0
    readonly property var state: job.snapshot
    readonly property var acceptance: ({state: state.state, message: state.message, bytes: state.bytes, total: state.total, speed: state.speed, traffic: trafficLine.text, opened: opened, busy: job.busy, source: sourcePath, folder: folder.text, target: job.targets.length ? job.targets[targetIndex].id : ""})
    visible: opened
    function open(path, holder) {
        if (job.busy) return
        sourcePath = path; focusHolder = holder; targetIndex = 0; folder.text = ""; opened = true
        job.snapshot = {state: "ready", message: "Originals are kept. Existing different files are refused.", bytes: 0, total: 0, speed: 0}
        job.loadTargets()
        folder.takeFocus()
    }
    function close() {
        if (job.busy) { job.cancel(); return }
        opened = false
        if (focusHolder) focusHolder.forceActiveFocus()
    }
    function start() {
        if (job.targets.length > targetIndex) job.start(job.targets[targetIndex].id, folder.text.trim(), sourcePath)
    }
    function stepFocus(back) {
        var items = [targetFocus, folder.input, cancelFocus, startFocus].filter(function(i) { return i.visible && i.enabled })
        var index = items.findIndex(function(i) { return i.activeFocus })
        items[(index + (back ? -1 : 1) + items.length) % items.length].forceActiveFocus()
    }
    Keys.onEscapePressed: root.close()
    Keys.onTabPressed: function(event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: root.stepFocus(true)
    Keys.onPressed: function(event) { event.accepted = true }
    Flea.CloudUploadJob { id: job }
    Rectangle {
        anchors.fill: parent; color: Theme.color.background; opacity: 0.5
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onClicked: root.close(); onWheel: function(wheel) { wheel.accepted = true } }
    }
    Rectangle {
        anchors.centerIn: parent
        width: Math.max(0, Math.min(Theme.space(420) * Theme.dialogWidthRatio, root.width - 2 * Theme.spacing.gap))
        height: Math.min(body.wanted + 2 * Theme.spacing.rowPaddingX, root.height - 2 * Theme.spacing.gap)
        color: Theme.color.surface; border.color: Theme.color.muted; border.width: Theme.spacing.hairline; radius: Style.cornerRadius
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onWheel: function(wheel) { wheel.accepted = true } }
        Flea.CardScroll {
            id: body
            anchors.fill: parent; anchors.margins: Theme.spacing.rowPaddingX
            Column {
                width: body.width; spacing: Theme.spacing.gap
                Flea.DialogTitle { width: parent.width; text: "Upload to cloud" }
                Text {
                    width: parent.width; text: root.sourcePath; textFormat: Text.PlainText; elide: Text.ElideMiddle
                    color: Theme.color.muted; font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                FocusScope {
                    id: targetFocus; width: parent.width; height: targetButton.implicitHeight; enabled: !job.busy && job.targets.length > 0
                    Keys.onReturnPressed: root.targetIndex = (root.targetIndex + 1) % job.targets.length
                    Keys.onSpacePressed: root.targetIndex = (root.targetIndex + 1) % job.targets.length
                    Flea.DialogButton {
                        id: targetButton; width: parent.width; primary: parent.activeFocus; available: parent.enabled
                        label: job.loading ? "Loading targets…" : job.targets.length ? job.targets[root.targetIndex].label : "No configured targets"
                        onActivated: root.targetIndex = (root.targetIndex + 1) % job.targets.length
                    }
                }
                Text {
                    width: parent.width; text: job.targets.length ? job.targets[root.targetIndex].destination : ""
                    textFormat: Text.PlainText; elide: Text.ElideMiddle; color: Theme.color.muted
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Flea.DialogField {
                    id: folder; width: parent.width; enabled: !job.busy; label: "Folder within target"; placeholder: "Leave empty for target root"
                    onAccepted: root.start(); onTabbed: function(from, back) { root.stepFocus(back) }
                }
                Text {
                    width: parent.width; text: job.targetError || job.snapshot.message; textFormat: Text.PlainText; wrapMode: Text.Wrap
                    color: job.targetError || job.snapshot.state === "error" ? Theme.color.error : Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.body }
                }
                Text {
                    id: trafficLine
                    width: parent.width; visible: job.busy; textFormat: Text.PlainText; wrapMode: Text.Wrap
                    text: job.snapshot.state.charAt(0).toUpperCase() + job.snapshot.state.slice(1) + (job.snapshot.total > 0 ? "\nTransfer traffic (including retries): " + Format.size(job.snapshot.bytes) + " / " + Format.size(job.snapshot.total) + " · " + Format.size(job.snapshot.speed) + "/s" : "")
                    color: Theme.color.muted; font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Text {
                    width: parent.width; text: "Files and hidden files are copied. Empty folders, symlinks and special files are not supported. Originals are never removed. Closing Flea cancels this job."
                    textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.color.muted
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
                Row {
                    anchors.right: parent.right; spacing: Theme.spacing.gap
                    FocusScope {
                        id: cancelFocus; width: closeButton.implicitWidth; height: closeButton.implicitHeight
                        Keys.onReturnPressed: root.close(); Keys.onSpacePressed: root.close()
                        Flea.DialogButton { id: closeButton; label: job.busy ? "Cancel upload" : "Close"; primary: parent.activeFocus; available: !job.cancelling; onActivated: root.close() }
                    }
                    FocusScope {
                        id: startFocus; width: startButton.implicitWidth; height: startButton.implicitHeight; enabled: !job.busy && !job.loading && job.targets.length > 0
                        Keys.onReturnPressed: root.start(); Keys.onSpacePressed: root.start()
                        Flea.DialogButton { id: startButton; label: ["error", "cancelled"].indexOf(job.snapshot.state) >= 0 ? "Retry" : "Upload copy"; primary: parent.activeFocus; available: parent.enabled; onActivated: root.start() }
                    }
                }
            }
        }
    }
}
