//@ pragma ShellId flea-picker-folder-state-test
import QtQuick
import QtQuick.Window
import Quickshell
import "flea" as Flea

ShellRoot {
    id: root
    property int stage: 0
    property int checks: 0
    property var failures: []
    property int old: 0
    function check(ok, label) { root.checks++; if (!ok) root.failures.push(label) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (var i = 0; i < item.children.length; i++) { var found = root.find(item.children[i], name); if (found) return found }
        return null
    }
    QtObject {
        id: picker
        property bool saving: false
        property bool backendUnavailable: false
        property bool submitting: false
        property bool recent: false
        property bool listingFailed: false
        property string listingState: "ready"
        property string path: "/synthetic"
        property string home: "/synthetic"
        property int serial: 0
        property var requests: []
        function check(value) { requests = requests.concat([value]); return ++serial }
        function focusView() {}
    }
    Window {
        id: host
        width: 875; height: 600; visible: true
        Flea.PickerNewFolder { id: folder; picker: picker }
    }
    Timer {
        id: clock
        interval: 150
        repeat: true
        running: true
        onTriggered: {
            if (root.stage === 0) {
                root.check(!folder.available, "open picker excludes folder creation")
                picker.saving = true
                root.check(folder.available, "save picker includes folder creation")
                picker.recent = true; root.check(!folder.available, "Recent excluded"); picker.recent = false
                picker.listingState = "loading"; root.check(!folder.available, "loading excluded"); picker.listingState = "ready"
                picker.listingFailed = true; root.check(!folder.available, "failed listing excluded"); picker.listingFailed = false
                picker.backendUnavailable = true; root.check(!folder.available, "dead backend excluded"); picker.backendUnavailable = false
                folder.open(null)
                root.old = folder.preparing
                folder.submit()
                root.check(folder.pending === 0 && picker.requests.length === 1, "typing/Enter during preparation cannot create")
                folder.close(false)
                folder.received({id: root.old, ok: true, review: root.old})
                root.check(!folder.ready && !folder.active, "late prepare after cancellation ignored")
                folder.open(null)
                folder.received({id: root.old, ok: true, review: root.old})
                root.check(!folder.ready, "old session response cannot ready a new card")
                folder.received({id: folder.preparing, ok: false, error: "Checking failed"})
                root.check(!folder.ready && folder.error === "Checking failed", "failed prepare reported")
                folder.submit()
                root.check(folder.preparing > 0 && folder.pending === 0, "retry checks parent without creating")
                folder.received({id: folder.preparing, ok: true, review: 99})
                folder.name = "new"; folder.fieldItem.text = "new"
                folder.submit()
                root.old = folder.pending
                root.check(!folder.close(), "pending request owns modal")
                picker.path = "/elsewhere"
                folder.received({id: root.old, ok: true, created: true, path: "/synthetic/new"})
                root.check(folder.active && folder.committed && folder.error.indexOf("created") >= 0, "changed context reports committed outcome, never navigates")
                var count = picker.requests.length
                folder.submit()
                root.check(picker.requests.length === count, "committed partial success cannot be retried")
                folder.close(false)
                Flea.ViewState.setTextSize({mode: 14})
                folder.open(null)
                folder.received({id: folder.preparing, ok: true, review: 100})
                folder.name = "new"; folder.fieldItem.text = "new"
                root.stage++
            } else if (root.stage === 1) {
                var card = root.find(folder, "pickerFolderCard")
                var viewport = root.find(folder, "pickerFolderViewport")
                var create = root.find(folder, "pickerFolderCreate")
                var cancel = root.find(folder, "pickerFolderCancel")
                root.check(viewport.height >= viewport.contentHeight, "normal card includes focus bleed in its height")
                for (var button of [create, cancel]) {
                    var frame = root.find(button, "buttonFrame")
                    var bottom = frame.mapToItem(viewport, 0, frame.height).y
                    root.check(bottom <= viewport.height - viewport.bleedY + 0.5, button.objectName + " bottom border remains inside viewport")
                }
                host.width = 320; host.height = 280
                Flea.ViewState.setTextSize({mode: 20})
                root.stage++
            } else if (root.stage === 2) {
                var card = root.find(folder, "pickerFolderCard")
                var create = root.find(folder, "pickerFolderCreate")
                var cancel = root.find(folder, "pickerFolderCancel")
                root.check(card && card.width <= host.width && card.height <= host.height, "card clamped at large text/narrow size")
                root.check(create && create.width <= card.width && cancel.width <= card.width, "actions individually fit card")
                root.check(create.y !== cancel.y, "actions wrap instead of overflowing")
                var scope = folder.children[0].item
                scope.step(folder.fieldItem, false)
                root.check(cancel.activeFocus, "Tab goes to modal Cancel")
                root.stage++
            } else {
                picker.backendUnavailable = true
                root.check(folder.active && !folder.ready && folder.error.indexOf("stopped responding") >= 0, "backend loss stays visible and cancellable")
                folder.close(false)
                clock.stop()
                for (var i = 0; i < root.failures.length; i++) console.log("FOLDERSTATE FAIL " + root.failures[i])
                console.log("FOLDERSTATE DONE checks=" + root.checks + " failed=" + root.failures.length)
                Qt.quit()
            }
        }
    }
}
