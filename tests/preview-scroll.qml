//@ pragma ShellId flea-preview-scroll-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/Focus.js" as Focus

// Real key events through the preview keymap and dispatcher into the shipped Quick Look panes.
ShellRoot {
    id: shell
    property int checks: 0
    property int failures: 0
    property int stage: 0
    property bool ticking: false
    // Listing ownership is stubbed; key lookup, sequence handling and preview dispatch are shipped Focus code.
    property var route: ({ preview: null, cursorIndex: 1, shown: null, shownTotal: 3,
        selectionVersion: 0, viewMode: "list", path: "fixture", searchMode: "idle",
        keySequence: "", keySequenceIdentity: "", trashArmedAt: 0,
        selection: { follows: function() { return false } },
        renameEditor: function() { return null }, showRow: function() {}, rowFor: function() { return null } })
    function check(ok, label) {
        checks++
        if (!ok) failures++
        console.log("PREVIEW_SCROLL " + (ok ? "CHECK " : "FAIL ") + label)
    }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    Component.onCompleted: { Flea.ViewState.state = { display: { textSize: { mode: 14 } } }; route.preview = preview }
    FloatingWindow {
        implicitWidth: 640
        implicitHeight: 360
        Flea.Preview {
            id: preview
            active: true
            kind: "text"
            path: Quickshell.env("FLEA_SCROLL_DOC")
            size: 1
        }
        Item {
            id: keys
            anchors.fill: parent
            focus: true
            Keys.onPressed: event => {
                event.accepted = Focus.handleKey(event, shell.route, null)
            }
            TestEvent { id: driver }
        }
    }
    function document() {
        if (preview.isMarkdown) return preview.markdownItem.bodyItem.parent
        var item = preview.surfaceItem()
        while (item && typeof item.scrollBy !== "function") item = item.parent
        return item
    }
    function scrollY() { return document().scrollY }
    function press(key, mods) { driver.keyClick(key, mods, -1) }
    function view() {
        if (preview.isMarkdown) return stage === 1 ? document().sourceItem : preview.markdownItem.bodyItem
        var item = document().bodyItem.parent
        while (item && typeof item.contentY !== "number") item = item.parent
        return item
    }
    function bounds() {
        var v = view()
        return [v.originY - v.topMargin, Math.max(v.originY - v.topMargin, v.originY + v.contentHeight - v.height + v.bottomMargin)]
    }
    function walk(key, mods) {
        // Bound both attempts and extent; force lazy list geometry between real key events.
        for (var n = 0; n < 600; n++) {
            press(key, mods)
            var v = view()
            if (typeof v.forceLayout === "function") v.forceLayout()
            var edge = bounds()[key === Qt.Key_PageDown ? 1 : 0]
            if (near(scrollY(), edge)) return
        }
    }
    function near(a, b) { return Math.abs(a - b) < 1 }
    function exercise() {
        var low = bounds()[0], half = view().height / 2
        walk(Qt.Key_PageUp, Qt.NoModifier)
        check(near(scrollY(), low), "explicit top " + stage)
        var start = scrollY()
        press(Qt.Key_D, Qt.ControlModifier)
        check(near(scrollY() - start, half), "Ctrl+D half viewport " + stage)
        press(Qt.Key_U, Qt.ControlModifier)
        check(near(scrollY(), low), "Ctrl+U returns to top " + stage)
        press(Qt.Key_PageDown, Qt.NoModifier)
        check(near(scrollY() - start, half), "PageDown half viewport " + stage)
        press(Qt.Key_PageUp, Qt.NoModifier)
        check(near(scrollY(), low), "PageUp returns to top " + stage)
        walk(Qt.Key_PageDown, Qt.NoModifier)
        var end = scrollY(), high = bounds()[1]
        check(end > low && near(end, high), "keys reach nonzero bottom " + stage)
        press(Qt.Key_D, Qt.ControlModifier)
        check(near(scrollY(), end), "Ctrl+D clamps at bottom " + stage)
        press(Qt.Key_PageDown, Qt.NoModifier)
        check(near(scrollY(), end), "PageDown clamps at bottom " + stage)
        walk(Qt.Key_U, Qt.ControlModifier)
        check(near(scrollY(), bounds()[0]), "repeated Ctrl+U reaches top " + stage)
        press(Qt.Key_PageUp, Qt.NoModifier)
        check(near(scrollY(), bounds()[0]), "PageUp clamps at top " + stage)
        var y = scrollY()
        for (var chord of [[Qt.Key_J, 2], [Qt.Key_K, 1], [Qt.Key_Down, 2], [Qt.Key_Up, 1]]) {
            press(chord[0], Qt.NoModifier)
            check(route.cursorIndex === chord[1] && near(scrollY(), y), "bare navigation keeps scrolling separate " + chord[0] + " " + stage)
        }
    }
    Timer {
        interval: 400
        running: true
        repeat: true
        onTriggered: {
            if (shell.ticking) return
            shell.ticking = true
            try { step() } finally { shell.ticking = false }
        }
        function step() {
            if (preview.status !== "ready" || (preview.isMarkdown && !preview.markdownItem.contentReady)) return
            keys.forceActiveFocus()
            shell.exercise()
            if (shell.stage === 0) preview.markdownSource = true
            else if (shell.stage === 1) preview.path = Quickshell.env("FLEA_SCROLL_TEXT")
            else {
                press(Qt.Key_Space, Qt.NoModifier)
                check(!preview.active, "Space closes through Focus")
                preview.active = true
                press(Qt.Key_Escape, Qt.NoModifier)
                check(!preview.active, "Escape closes through Focus")
                console.log("PREVIEW_SCROLL " + shell.checks + " checks, " + shell.failures + " failed")
                shell.quit()
                stop()
            }
            shell.stage++
        }
    }
    Timer {
        interval: 10000
        running: true
        onTriggered: { console.log("PREVIEW_SCROLL FAIL watchdog"); shell.quit() }
    }
}
