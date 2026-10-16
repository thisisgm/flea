//@ pragma ShellId flea-font-preview-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/Kinds.js" as Kinds
import "flea/js/Facts.js" as Facts
import "flea/js/Nav.js" as Nav

ShellRoot {
    id: root
    property int stage: 0
    property int ticks: 0
    property int checks: 0
    property int failed: 0
    readonly property string path: Quickshell.env("FONT_FIXTURE")
    function check(ok, message) { checks++; if (!ok) { failed++; console.log("FONT FAIL " + message) } }
    function finish() {
        tick.stop()
        console.log("FONT DONE checks=" + checks + " failed=" + failed)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
    FloatingWindow {
        id: win
        implicitWidth: 1000
        implicitHeight: 800
        visible: true
        Flea.Preview { id: preview }
        Flea.PreviewColumn {
            id: column
            width: 310
            height: parent.height
            visible: false
            row: ({ n: "fixture.ttf", d: false, i: "font-x-generic", p: 33188, s: 100, m: 0 })
            kindName: "Font"
            meta: ({})
            path: root.path
        }
    }
    property var keys: null
    Component.onCompleted: root.keys = Qt.createQmlObject("import QtTest; TestEvent {}", win.contentItem)
    Timer {
        id: tick
        interval: 100
        running: true
        repeat: true
        onTriggered: {
            if (++root.ticks > 450) { root.check(false, "timed out stage " + root.stage + " status=" + preview.status); root.finish(); return }
            var item = preview.fontItem
            if (root.stage === 0) {
                root.check(item === null, "no font object before opening")
                root.check(Kinds.quickLookKind("font-x-generic", "a.TTF") === Kinds.FONT, "TTF routes internally")
                root.check(Kinds.quickLookKind("text-x-generic", "a.otf") === Kinds.FONT, "OTF overrides a generic icon")
                root.check(Facts.state({n: "FONT.TTF", p: 33188}, 1, false, "", "Data") === Kinds.FONT, "column recognizes a font with unknown MIME")
                // Enter's actual Nav route must call Quick Look and never the foreign opener.
                var opened = false
                var pane = { cursorIndex: 0, listingPath: "/fixture", path: "/fixture", filterText: "", filterIndices: [0], filterMatches: [0], filterActive: false, filtered: false, shown: null, searchMode: "", recentMode: "",
                    rowFor: function() { return {n: "a.TTF", d: false, i: "font-x-generic", s: 100, k: 0} },
                    join: function(a,b) { return a + "/" + b }, kindNames: ["Font"],
                    quickLook: function() { return {open: function() { opened = true }} },
                    message: function() {} }
                Nav.openCursor(pane, {open: function() { root.check(false, "Enter used foreign opener") }})
                root.check(opened, "Enter opens font preview")
                preview.show(root.path, "font-x-generic", 0, "Font", "", -1)
                root.check(!preview.advancedFont && preview.fontManageControl.visible, "Quick Look starts with the upstream sample and a management control")
                preview.fontManageControl.activated()
                root.stage++
            } else if (root.stage === 1 && preview.status === "font" && item && !item.busy) {
                if (Quickshell.env("FONT_CAPTURE")) item.grabToImage(function(result) { result.saveToFile(Quickshell.env("FONT_CAPTURE")) })
                root.check(item.fontInfo.family.length > 0, "real font family loaded")
                root.check(!item.fontInfo.installed, "fixture starts uninstalled")
                var size = item.sampleSize
                item.changeSize(4)
                root.check(item.sampleSize === size + 4, "size control changes sample")
                item.sampleInput.forceActiveFocus()
                item.sampleInput.text = "Test"
                item.sampleInput.cursorPosition = 4
                root.keys.keyClick(Qt.Key_Space, Qt.NoModifier, -1)
                root.keys.keyClick(Qt.Key_J, Qt.NoModifier, -1)
                root.check(item.sampleInput.text === "Test j", "Space and j type in sample: " + item.sampleInput.text)
                root.check(preview.active, "typing does not close preview")
                item.actionItem.activated()
                root.stage++
            } else if (root.stage === 2 && item && !item.busy) {
                root.check(item.fontInfo.installed, "button installs real font")
                root.check(item.actionItem.label === "Uninstall", "installed font offers uninstall")
                item.actionItem.activated()
                root.stage++
            } else if (root.stage === 3 && item && !item.busy) {
                root.check(!item.fontInfo.installed, "button uninstalls real font")
                preview.close()
                root.stage++
            } else if (root.stage === 4) {
                root.check(preview.fontItem === null, "close releases font viewer")
                preview.show(root.path, "font-x-generic", 0, "Font", "", -1)
                root.check(!preview.fontManagement, "reopening restores the isolated quick sample")
                preview.fontManageControl.activated()
                root.stage++
            } else if (root.stage === 5 && preview.status === "font" && item && !item.busy) {
                root.check(!item.fontInfo.installed, "reopen reads uninstalled state")
                item.sampleInput.forceActiveFocus()
                root.keys.keyClick(Qt.Key_Escape, Qt.NoModifier, -1)
                root.check(!preview.active, "Escape closes from text editor")
                column.visible = true
                root.stage++
            } else if (root.stage === 6 && column.fontItem && column.fontItem.status === "font" && !column.fontItem.busy) {
                root.check(column.fontItem.compact, "columns uses compact font preview")
                root.check(column.fontItem.fontInfo.family.length > 0, "columns renders real font")
                column.overlayOpen = true
                root.stage++
            } else if (root.stage === 7) {
                root.check(column.fontItem === null, "Quick Look releases the column font viewer")
                root.finish()
            }
        }
    }
}
