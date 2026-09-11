//@ pragma NativeTextRendering
import QtQuick
import Quickshell
import qs.Commons
import "." as Flea
import "js/Thumbs.js" as Thumbs

// Copied beside the real UI by typography.sh. No stub Theme: these are the production components
// and Omarchy singletons. Read actual Text/TextInput fonts so a smaller delegate binding fails even
// when Theme.tokens() is right. All settings and file reads belong to the runner's sandbox HOME.
Item {
    id: root
    width: 1400
    height: 1000
    property int failures: 0
    property int checks: 0
    property int step: -1
    property var stops: [9, 10, 11, 12, 14, 16, 20]
    property var cases: []
    property var sample: ({ n: "readable.txt", d: false, s: 12345, m: 1780000000,
                            p: 33188, i: "text-x-generic", k: 0, t: true })

    Flea.Row { id: listRow; width: root.width; row: root.sample; kindNames: ["Text document"]; hiddenCols: [] }
    Flea.Row { id: pickerRow; width: root.width; row: root.sample; compactDate: true; leadingSlot: 20; hiddenCols: ["mode", "kind"] }
    Flea.ColumnRow { id: columnRow; width: 300; row: root.sample }
    Flea.SidebarRow { id: railRow; index: 0; modelData: ({label: "Documents", group: "places", kind: "favorite", glyph: "folder"}) }
    Flea.RenameField { id: editor; width: 250; height: Flea.Theme.rowHeight; name: "readable.txt" }
    Flea.Header { id: header; width: root.width }
    Flea.MenuRow { id: menu; width: Flea.Theme.menuWidth; entry: ({label: "Send with Taildrop", glyph: "file", action: "open"}) }
    Flea.DialogField { id: field; width: 300; label: "Filename"; text: "readable.txt" }
    Flea.DialogButton { id: button; label: "Open" }
    Flea.FactsTable { id: facts; width: 400; rows: [{label: "Kind", value: "Text document"}] }
    Flea.SettingsPanel { id: settings; width: root.width; height: root.height; opened: true }
    Flea.SettingsRow { id: setting; width: Flea.Theme.settings.paneWidth; row: ({kind: "fact", label: "Reading text", value: Flea.Theme.font.body + "px"}) }
    Flea.SettingsSegment { id: segment; options: ["Follow Omarchy", "Override"]; value: "Follow Omarchy" }
    QtObject {
        id: fakePane
        property int total: 100
        property int held: 0
        property int cursorIndex: 0
        property var rows: Array(total).fill(root.sample)
        property var backend: fakeBackend
        property var thumbState: Thumbs.empty()
        property bool listInFlight: true
        property int coalesceMs: 16
        property int settleMs: 120
        function rowFor(index) { return root.sample }
        function isSelected(index) { return false }
    }
    QtObject {
        id: fakeBackend
        property var asked: []
        function thumb(rows) { asked = rows }
        function thumbcancel(rows) {}
    }
    QtObject { id: fakeMenu; function close() {} }
    Flea.GridArea { id: grid; width: 600; height: 400; pane: fakePane; menu: fakeMenu }
    Flea.GridTile {
        id: longTile
        width: Flea.Theme.grid.minCellWidth
        height: grid.cellHeight
        row: ({n: "A file with a long name needing two lines.txt", d: false, i: "text-x-generic", p: 33188})
    }

    function check(label, actual, expected) {
        root.checks++
        if (actual !== expected) {
            root.failures++
            console.log("FAIL", label, "expected", expected, "got", actual)
        }
    }
    function texts(item, result) {
        if (item.font !== undefined && item.text !== undefined && item.visible && String(item.text).length)
            result.push(item)
        var children = item.children || []
        for (var i = 0; i < children.length; i++) texts(children[i], result)
        return result
    }
    function normal(label, item, minimum) {
        var found = texts(item, [])
        check(label + " has real text", found.length >= minimum, true)
        for (var i = 0; i < found.length; i++) {
            check(label + " " + found[i].text, found[i].font.pixelSize, root.cases[root.step].body)
            check(label + " family", found[i].font.family, Style.font.family)
        }
        return found
    }
    function checkCase() {
        var c = root.cases[root.step]
        check(c.label + " body", Flea.Theme.font.body, c.body)
        check(c.label + " base", Flea.Theme.baseSize, c.mode === "system" ? c.base : c.mode)
        normal("list", listRow, 5)
        normal("picker", pickerRow, 3)
        normal("column", columnRow, 1)
        normal("rail", railRow, 1)
        normal("rename", editor, 1)
        normal("header", header, 3)
        normal("grid", grid, 1)
        var range = grid.visibleRange()
        fakePane.listInFlight = false
        grid.requestThumbs()
        fakePane.listInFlight = true
        check("grid still uses an integer model", grid.model, fakePane.total)
        check("grid asks for its visible range only", fakeBackend.asked.length, range.last - range.first + 1)
        check("grid does not sweep the directory", fakeBackend.asked.length < fakePane.total, true)
        check("first asked tile", fakeBackend.asked[0], range.first)
        check("last asked tile", fakeBackend.asked[fakeBackend.asked.length - 1], range.last)
        var wrapped = normal("wrapped filename", longTile, 1)
        for (var w = 0; w < wrapped.length; w++) {
            // The hover tooltip is intentionally outside the tile; only the name owns its two lines.
            if (wrapped[w].parent === longTile)
                check("wrapped filename fits tile", wrapped[w].y + wrapped[w].height <= longTile.height, true)
        }
        normal("menu", menu, 1)
        normal("field", field, 2)
        normal("button", button, 1)
        normal("facts", facts, 2)
        normal("setting", setting, 2)
        var options = normal("segments", segment, 2)
        for (var i = 0; i < options.length; i++)
            check("segment fits vertically", options[i].height <= options[i].parent.height, true)
        check("rows fit the font", Flea.Theme.rowHeight >= Flea.Theme.bodyLineHeight + 2 * Flea.Theme.spacing.rowPaddingY, true)
        check("chrome fits the font", Flea.Theme.chromeHeight >= Flea.Theme.bodyLineHeight, true)
        var tile = grid.itemAtIndex(0)
        check("grid built a tile", tile !== null, true)
        if (tile) {
            var labels = texts(tile, [])
            for (var j = 0; j < labels.length; j++)
                check("grid label fits two lines", labels[j].y + 2 * Flea.Theme.bodyLineHeight <= tile.height, true)
        }
        var panelText = texts(settings, [])
        var primary = ["Text size", "Follow Omarchy", "Override", "Reading text", "Display"]
        var readCount = 0
        for (var p = 0; p < panelText.length; p++) {
            if (primary.indexOf(String(panelText[p].text)) < 0 || panelText[p].font.capitalization === Font.AllUppercase) continue
            readCount++
            check("settings " + panelText[p].text, panelText[p].font.pixelSize, c.body)
            check("settings label fits", panelText[p].width >= panelText[p].implicitWidth, true)
        }
        check("settings has primary labels", readCount >= 5, true)
        var ruler = settings.rows[2]
        check("ruler names the base", ruler.label, "Base")
        check("reading size is not the base when overridden by a token", settings.rows[3].value, c.body + "px")
        check("body token is exposed", Flea.Theme.tokens().indexOf("\nbody=" + c.body + "\n") >= 0, true)
        // The smaller tokens still mean what Omarchy means: changing their definitions is not a fix.
        if (c.mode === "system") {
            check("caption still follows Omarchy", Flea.Theme.font.caption, Style.font.caption)
            check("small token still follows Omarchy", Flea.Theme.font.bodySmall, Style.font.bodySmall)
        }
        if (c.label === "baseline 14") console.log("TOKENS\n" + Flea.Theme.tokens())
        console.log("CASE", c.label, "body=" + c.body)
    }
    function next() {
        if (root.step >= 0) checkCase()
        root.step++
        if (root.step >= root.cases.length) {
            root.step--
            // Editing uses the same size as the rail label, without clipping its input box.
            railRow.renaming = true
            Qt.callLater(function () {
                normal("rail rename", railRow, 1)
                var fields = texts(railRow, [])
                for (var i = 0; i < fields.length; i++)
                    check("rail editor fits", fields[i].height >= Flea.Theme.bodyLineHeight, true)
                console.log("TYPOGRAPHY", root.checks, "checks,", root.failures, "failed")
                Quickshell.execDetached(["kill", String(Quickshell.processId)])
            })
            timer.stop()
            return
        }
        var c = root.cases[root.step]
        Color.loadShell("[font]\nbase-size=" + c.base + (c.themeBody ? "\nbody=" + c.themeBody : ""))
        Color.loadUserShell(c.userBody ? "[font]\nbody=" + c.userBody : "")
        Flea.ViewState.state = {display: {textSize: {mode: c.mode}}}
    }
    Component.onCompleted: {
        var out = []
        for (var i = 0; i < root.stops.length; i++) {
            var n = root.stops[i]
            out.push({label: "baseline " + n, base: n, body: n, mode: "system"})
            out.push({label: "pinned " + n, base: 14, themeBody: 22, body: n, mode: n})
        }
        out.push({label: "large body override", base: 12, themeBody: 28, body: 28, mode: "system"})
        out.push({label: "theme body override", base: 12, themeBody: 20, body: 20, mode: "system"})
        out.push({label: "user wins", base: 12, themeBody: 20, userBody: 18, body: 18, mode: "system"})
        out.push({label: "follow restored", base: 12, body: 12, mode: "system"})
        root.cases = out
    }
    Timer { id: timer; running: true; repeat: true; interval: 150; onTriggered: root.next() }
}
