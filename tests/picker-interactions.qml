//@ pragma ShellId flea-picker-interactions-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea

// Real pointer/key/wheel delivery through the chooser, with a real read-only listing worker.
ShellRoot {
    id: root
    property var shell: null
    property var win: null
    property var chrome: null
    property var entry: null
    property var hidden: null
    property var filename: null
    property var grid: null
    property int stage: 0
    property int checks: 0
    property int failures: 0
    property double started: Date.now()
    property bool driving: false
    property string base: ""
    property real beforeHeight: 0
    property real beforeScroll: 0
    property int beforeCursor: 0
    property string beforeSelection: ""
    property var sizes: ["small", "medium", "large", "xlarge", "huge", "largest"]

    function check(label, ok) {
        root.checks++
        if (!ok) { root.failures++; console.log("PICKERINPUT FAIL " + label) }
    }
    function find(item, predicate) {
        if (predicate(item)) return item
        var children = item.children || []
        for (var i = 0; i < children.length; i++) {
            var found = root.find(children[i], predicate)
            if (found) return found
        }
        return null
    }
    function named(name) { return root.find(win.contentItem, function(item) { return item.objectName === name }) }
    function click(item, x) { driver.mouseClick(item, x === undefined ? item.width / 2 : x, item.height / 2, Qt.LeftButton, Qt.NoModifier, -1) }
    function wheel(notches, modifiers) {
        driver.mouseWheel(grid, grid.width / 2, grid.height / 2, Qt.NoButton,
            modifiers === undefined ? Qt.ControlModifier : modifiers, 0, notches * 120, 1)
    }
    function hasRow(name) { return win.rows.some(function(row) { return row.n === name }) }
    function settled() { return win.pendingListings === 0 && win.listingState === "ready" && (!win.saving || win.saveReady) }
    function done() {
        clock.stop()
        console.log("PICKERINPUT DONE checks=" + checks + " failed=" + failures)
        win.cancel()
    }

    property var actions: [
        function() {
            if (!Flea.ViewState.initialReadComplete) return false
            var component = Qt.createComponent("flea/PickerWindow.qml")
            if (component.status !== Component.Ready) { root.check(component.errorString(), false); Qt.quit(); return false }
            shell = component.createObject(root); win = shell.pickerWin; base = win.path
            chrome = root.find(win.contentItem, function(item) { return item.startEdit !== undefined && item.submissionChanged !== undefined })
            entry = root.find(chrome, function(item) { return item.pathField !== undefined })
            hidden = root.named("pickerHidden")
            filename = root.find(win.contentItem, function(item) { return item.fieldItem !== undefined && item.selectStem !== undefined }).fieldItem
            return true
        },
        function() {
            if (!root.settled()) return false
            root.check("hidden files start excluded", !win.showHidden && !root.hasRow(".secret"))
            root.check("visible hidden-files action", hidden.visible && hidden.enabled && hidden.accessName === "Show hidden files")
            root.click(entry, 2)
            return true
        },
        function() {
            root.check("clicking the path opens a focused editor", entry.editing && entry.pathField.activeFocus)
            root.check("current directory is selected", entry.pathField.selectedText === base + "/")
            entry.pathField.text = base + "/uncommitted"
            driver.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
            root.check("Escape discards a path without answering", !entry.editing && win.path === base && !win.answered)
            root.click(entry, entry.width - 2)
            return true
        },
        function() {
            root.check("path padding also opens editing", entry.editing && entry.pathField.text === base + "/")
            if (win.saving) root.click(filename)
            else root.click(win.viewItem().itemAtIndex(0))
            return true
        },
        function() {
            root.check("click-away closes the path editor", !entry.editing)
            root.check("click-away preserves target focus", win.saving ? filename.activeFocus : win.viewItem().activeFocus)
            root.click(entry)
            return true
        },
        function() {
            entry.pathField.text = base + "/sub"
            driver.keyClick(Qt.Key_Return, Qt.NoModifier, -1)
            return true
        },
        function() {
            if (win.path !== base + "/sub" || !root.settled()) return false
            root.check("typed directory navigates without submitting", !entry.editing && !win.answered)
            beforeSelection = filename.selectedText
            root.click(hidden)
            return true
        },
        function() {
            if (!root.settled()) return false
            root.check("pointer toggle includes actual hidden rows", win.showHidden && root.hasRow(".secret"))
            root.check("hidden action reflects the current state", hidden.active && hidden.accessName === "Hide hidden files")
            root.check("hidden toggle is local to this request", Flea.ViewState.state.hidden === false)
            root.check("save name and selection survive relisting", !win.saving || (win.saveName === "report.pdf" && filename.activeFocus && filename.selectedText === beforeSelection))
            root.click(hidden)
            return true
        },
        function() {
            if (!root.settled()) return false
            root.check("second toggle excludes hidden rows", !win.showHidden && !root.hasRow(".secret"))
            // A modified shortcut reaches the listing even while Filename owns unmodified text.
            driver.keyClick(Qt.Key_Period, Qt.ControlModifier | Qt.ShiftModifier, -1)
            return true
        },
        function() {
            if (!root.settled()) return false
            root.check("existing hidden shortcut still works", win.showHidden && root.hasRow(".secret"))
            win.reviewRequest = 999
            root.check("hidden toggle is disabled during submission", !hidden.enabled)
            root.click(entry); root.click(hidden)
            root.check("submission cannot open the path or change visibility", !entry.editing && win.showHidden)
            win.reviewRequest = 0
            win.setView("grid"); grid = win.viewItem()
            Flea.ViewState.changeSetting("preview.thumbSize", "small")
            return true
        },
        function() {
            beforeHeight = grid.cellHeightPx; beforeCursor = win.cursorIndex; beforeScroll = grid.contentY
            root.wheel(1)
            return true
        },
        function() {
            root.check("Ctrl+wheel increases icon size", Flea.ViewState.thumbnailSize === "medium" && grid.cellHeightPx > beforeHeight)
            root.check("zoom does not scroll or move the cursor", grid.contentY === beforeScroll && win.cursorIndex === beforeCursor)
            return true
        }
    ]

    Component.onCompleted: {
        for (var i = 2; i < sizes.length; i++) (function(index) {
            actions.push(function() { root.wheel(1); return true })
            actions.push(function() { root.check("zoom reaches " + sizes[index], Flea.ViewState.thumbnailSize === sizes[index]); return true })
        })(i)
        actions.push(function() { root.wheel(10); return true })
        actions.push(function() { root.check("zoom clamps at largest", Flea.ViewState.thumbnailSize === "largest"); root.wheel(-20); return true })
        actions.push(function() { root.check("zoom clamps at small", Flea.ViewState.thumbnailSize === "small"); return true })
        actions.push(function() {
            // The same handler also accumulates phased touchpad travel and sub-notch wheels.
            var scroll = root.find(grid, function(item) { return item.objectName === "fleaScroll" })
            function send(phase, pixels, angle) {
                return scroll.handleWheel({phase: phase, pixelDelta: {x: 0, y: pixels}, angleDelta: {x: 0, y: angle}, modifiers: Qt.ControlModifier, accepted: false})
            }
            root.check("partial notch is consumed", send(Qt.NoScrollPhase, 0, 60) && Flea.ViewState.thumbnailSize === "small")
            send(Qt.NoScrollPhase, 0, 60)
            root.check("partial notches accumulate", Flea.ViewState.thumbnailSize === "medium")
            send(Qt.ScrollUpdate, -24, 0); send(Qt.ScrollUpdate, -24, 0)
            root.check("touchpad travel zooms", Flea.ViewState.thumbnailSize === "small")
            send(Qt.NoScrollPhase, 15, 120)
            root.check("Qt notch with incidental pixels stays a notch", Flea.ViewState.thumbnailSize === "medium")
            Flea.ViewState.changeSetting("preview.ctrlZoom", false)
            root.wheel(-1)
            return true
        })
        actions.push(function() {
            root.check("disabled Ctrl+wheel does not resize", Flea.ViewState.thumbnailSize === "medium")
            Flea.ViewState.changeSetting("preview.ctrlZoom", true)
            grid.contentY = 0
            root.wheel(-1, Qt.NoModifier)
            return true
        })
        actions.push(function() {
            root.check("ordinary wheel scrolls rather than zooms", grid.contentY > 0 && Flea.ViewState.thumbnailSize === "medium")
            grid.contentY = 0
            Flea.ViewState.changeSetting("preview.thumbSize", "largest")
            return true
        })
        actions.push(function() {
            // Leave only the first held row: zoom must refetch without a scroll event.
            win.rows = win.rows.slice(0, 1)
            beforeScroll = grid.contentY
            root.wheel(-20)
            return true
        })
        actions.push(function() {
            if (win.rows.length <= 1) return false
            root.check("zoom-out refetches newly visible rows without scrolling", grid.contentY === beforeScroll && Flea.ViewState.thumbnailSize === "small")
            root.check("interactions never submit or change the save name", !win.answered && (!win.saving || win.saveName === "report.pdf"))
            root.done(); return true
        })
    }
    TestEvent { id: driver }
    Timer {
        id: clock; interval: 80; repeat: true; running: true
        onTriggered: {
            if (root.driving) return
            if (Date.now() - started > 8000) { root.check("timeout at stage " + stage, false); root.done(); return }
            root.driving = true
            if (actions[stage]()) { stage++; started = Date.now() }
            root.driving = false
        }
    }
}
