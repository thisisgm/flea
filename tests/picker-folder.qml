//@ pragma ShellId flea-picker-folder-test
//@ pragma NativeTextRendering
import QtQuick
import Quickshell
import "flea" as Flea

ShellRoot {
    id: root
    property var shell: null
    property var win: null
    property int stage: 0
    property int checks: 0
    property var failures: []
    property string base: ""
    property int request: 0
    property int ticks: 0
    function check(ok, label) { root.checks++; if (!ok) root.failures.push(label) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (var i = 0; i < item.children.length; i++) { var found = root.find(item.children[i], name); if (found) return found }
        return null
    }
    function name(value) { root.win.folderCreation.name = value; root.win.folderCreation.fieldItem.text = value }
    function done() {
        clock.stop()
        for (var i = 0; i < root.failures.length; i++) console.log("PICKERFOLDER FAIL " + root.failures[i])
        console.log("PICKERFOLDER DONE checks=" + root.checks + " failed=" + root.failures.length)
        if (root.win) root.win.cancel()
        else Qt.quit()
    }
    Timer {
        id: clock
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (++root.ticks > 150) { root.check(false, "timeout stage " + root.stage); root.done(); return }
            if (root.stage === 0) {
                var component = Qt.createComponent("flea/PickerWindow.qml")
                if (component.status !== Component.Ready) { root.check(false, component.errorString()); root.done(); return }
                root.shell = component.createObject(root)
                root.win = root.shell.pickerWin
                root.base = root.win.path
                root.stage++
                return
            }
            var win = root.win
            var folder = win.folderCreation
            if (root.stage === 1) {
                if (win.listingState === "loading" || win.saveRequest || !win.canCreateFolder) return
                root.check(win.saving, "save mode")
                root.check(win.saveName === "download.zip", "caller filename")
                var button = root.find(win.contentItem, "pickerNewFolder")
                var glyph = root.find(button, "pickerFolderGlyph")
                root.check(glyph && glyph.text === "\ueec7", "canonical Nerd Font folder-plus glyph")
                root.check(!button.ringItem.visible && !button.markItem.visible, "icon-only action has no button border")
                root.check(button.accessName === "New folder", "icon retains accessible action name")
                win.startNewFolder()
                root.stage++
            } else if (root.stage === 2) {
                if (!folder.ready || !folder.fieldItem) return
                root.check(folder.active, "modal opened")
                root.check(folder.folder === root.base, "parent captured")
                root.check(!win.canAccept, "save unavailable under modal")
                root.check(!win.viewItem().enabled, "listing input blocked")
                root.check(folder.fieldItem.activeFocus, "folder field focus")
                root.name("existing")
                folder.submit()
                root.request = folder.pending
                folder.submit()
                root.check(folder.pending === root.request, "duplicate submission blocked")
                root.check(!folder.close(), "pending creation cannot be dismissed")
                root.stage++
            } else if (root.stage === 3) {
                if (folder.pending) return
                root.check(folder.active && folder.error.indexOf("already exists") >= 0, "collision stays in card")
                root.check(folder.name === "existing" && folder.fieldItem.text === "existing", "failed name retained")
                root.check(folder.fieldItem.activeFocus, "failure returns field focus")
                root.check(win.path === root.base && win.saveName === "download.zip", "failure preserves save intent")
                root.name("Reports 雪")
                folder.submit()
                root.stage++
            } else if (root.stage === 4) {
                if (folder.active || win.listingState === "loading" || win.saveRequest || !win.canAccept || !win.canCreateFolder) return
                root.check(win.path === root.base + "/Reports 雪", "success enters created folder")
                root.check(win.saveName === "download.zip", "success preserves filename")
                root.check(!win.answered, "folder creation does not submit save")
                root.check(win.canAccept && win.canCreateFolder, "save and next creation re-enabled")
                root.check(win.contentItem.Window.window.activeFocusItem.text === "download.zip", "filename focused after success")
                win.startNewFolder()
                root.stage++
            } else if (root.stage === 5) {
                if (!folder.ready || !folder.fieldItem) return
                root.name("Never created")
                win.cancel()
                root.check(!folder.active && !win.answered, "Escape/cancel dismisses card only")
                folder.received({id: root.request, ok: true, path: root.base + "/stale"})
                root.check(win.path === root.base + "/Reports 雪", "late response cannot navigate after close")
                win.setView("grid")
                root.stage++
            } else if (root.stage === 6) {
                root.check(win.canCreateFolder, "available in grid save view")
                win.startNewFolder()
                root.stage++
            } else if (root.stage === 7) {
                if (!folder.ready || !folder.fieldItem) return
                root.check(folder.fieldItem.activeFocus, "grid modal owns field focus")
                folder.close()
                root.done()
            }
        }
    }
}
