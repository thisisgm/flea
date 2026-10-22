//@ pragma ShellId flea-rename-frame-test
import QtQuick
import QtTest
import Quickshell
import "flea" as Flea
import "flea/js/TextSize.js" as TextSize
import "rename-frame-checks.js" as Checks

// Decided 2026-10-03: the rename editor is one field in every view, pinned in six hosts at every text stop and density.
ShellRoot {
    id: root
    readonly property string fixture: Quickshell.env("HOME") + "/fixture"
    readonly property var pane: body.currentPane
    readonly property int hairline: Flea.Theme.spacing.hairline
    // The column cut to its rows while the last row's error is measured, null otherwise.
    property var squeezed: null
    readonly property int gap: Flea.Theme.spacing.gap
    readonly property int rowInset: Flea.Theme.spacing.rowPaddingX
    // What the checks in rename-frame-checks.js read: the window's body, the caption line and the list's body size.
    readonly property Item windowBody: body
    readonly property real captionLineHeight: Flea.Theme.grid.captionLineHeight
    readonly property int bodyPx: Flea.Theme.font.body
    readonly property alias metrics: metricsProbe
    // A step that waits gives up after this many ticks of the stepper, so a stuck probe fails instead of hanging.
    readonly property int stallTicks: 200
    // The row's text line box, which every host's frame is unless the typed line plus its two hairlines is taller.
    readonly property int lineBox: Flea.Theme.rowHeight - 2 * Flea.Theme.spacing.rowPaddingY
    // The first frame height this stop and density measured, which every other host must draw too.
    property real comboHeight: -1
    readonly property var allStops: TextSize.STOPS
    readonly property var densities: ["tight", "compact", "normal", "comfortable"]
    readonly property string errorSample: "A file by that name exists"
    // Each host is a view the editor opens in and the name it renames (the rail renames a place); onlyStop runs a host at that stop alone.
    readonly property var hosts: [
        { name: "list", view: "list", target: "a.txt" },
        { name: "dual", view: "dual", target: "a.txt" },
        { name: "columns file", view: "columns", target: "a.txt" },
        { name: "columns folder", view: "columns", target: "sub" },
        { name: "columns last", view: "columns", target: "link.txt", last: true, onlyStop: 14 },
        { name: "grid", view: "grid", target: "a.txt" },
        { name: "rail", view: "list", target: "" }
    ]
    property var combos: []
    property int comboIndex: 0
    property int hostIndex: 0
    property int stage: 0
    property int stageTicks: 0
    property int checks: 0
    property int measured: 0
    property int overhung: 0
    property int overhungAtStart: 0
    // The smallest stop at Tight, where a face whose typed line is taller than the row's line box overhangs it: Noto Sans Mono's
    // 9 px line is 13 px under Qt 6.12, so its frame of 15 overhangs the 14 px row, where the CI face's 12 px line fills it.
    readonly property int overhangStop: 9
    // rename-frame.sh's second pass runs that stop alone under Noto Sans Mono, so the overhang arm is measured every run.
    readonly property bool overhangPass: Quickshell.env("RENAMEFRAME_PASS") === "overhang"
    // Room left under the frame in its column before the error shows, read to prove the last row needs the scroll.
    property real roomBelow: 0
    property bool errorMeasured: false
    property var failures: []

    function check(cond, label) {
        root.checks++
        if (!cond) {
            root.failures.push(label)
            console.log("RENAMEFRAME FAIL " + label)
        }
    }
    function ready() { return !pane.listInFlight && pane.path === fixture && pane.listingState === "ready" }
    function ofType(item, type, out) {
        if (String(item).indexOf(type + "_") === 0) out.push(item)
        var kids = item && item.children ? item.children : []
        for (var i = 0; i < kids.length; i++) root.ofType(kids[i], type, out)
        return out
    }
    // Sample input: a QtQuick Text prints as "QQuickText(0x55d0)", which no Flea type name does.
    function texts(item, out) {
        if (String(item).indexOf("QQuickText") === 0) out.push(item)
        var kids = item && item.children ? item.children : []
        for (var i = 0; i < kids.length; i++) root.texts(kids[i], out)
        return out
    }
    function indexOfName(name) {
        for (var i = 0; i < pane.total; i++)
            if (pane.rowFor(i) && String(pane.rowFor(i).n) === name) return i
        return -1
    }
    // Sample input: whole({x: 3, y: 4.5, width: 20, height: 23}) is false, a box on whole pixels is true.
    function whole(box) {
        return box.x === Math.round(box.x) && box.y === Math.round(box.y)
            && box.width === Math.round(box.width) && box.height === Math.round(box.height)
    }
    // Sample input: inside({x: 4, y: 4, width: 20, height: 23}, 100, 31) is true.
    function inside(box, w, h) { return box.x >= 0 && box.y >= 0 && box.x + box.width <= w && box.y + box.height <= h }
    function boxText(box) { return box.x + "," + box.y + " " + box.width + "x" + box.height }
    // The host's live editor and the cell it draws in: a row, a tile or a rail row, and for the columns the one row's delegate.
    function hostOf(host) {
        if (host.name === "rail") {
            var place = pane.sidebar.renameEditor()
            return place ? { cell: place, editor: place.editorField } : null
        }
        var held = pane.renameEditor()
        if (!held) return null
        if (host.view !== "columns") return { cell: held, editor: held.editorField }
        var rows = root.ofType(held, "ColumnRow", [])
        for (var i = 0; i < rows.length; i++)
            if (rows[i].index === held.renameViewIndex) return { cell: rows[i], editor: held.editorField, column: held }
        return null
    }

    // The trailing cells of a column row, as boxes in the row's own space: the size text, and the chevron a chosen folder draws.
    function trailing(row) {
        var out = []
        var size = row.sizeGeom()
        var chevron = row.chevronGeom()
        out.push({ name: "size", x: size[0], width: size[1] })
        if (chevron[1] > 0) out.push({ name: "chevron", x: chevron[0], width: chevron[1] })
        return out
    }

    function measure(tag, host) {
        var found = root.hostOf(host)
        if (!found || !found.editor || !found.editor.inputItem.activeFocus) return false
        var editor = found.editor
        var frame = editor.frame
        var field = editor.inputItem
        var cell = found.cell
        root.measured++
        var box = frame.mapToItem(cell, 0, 0, frame.width, frame.height)
        var scene = frame.mapToItem(null, 0, 0, frame.width, frame.height)
        var line = field.contentHeight
        console.log("RENAMEFRAME HOST " + tag + " frame=" + root.boxText(box) + " row=" + cell.width + "x" + cell.height + " line=" + line)
        var want = Math.max(root.lineBox, Math.ceil(line) + 2 * root.hairline)
        root.check(frame.height === want, tag + " frame is " + frame.height + " tall, not the one line box " + want)
        if (root.comboHeight < 0) root.comboHeight = frame.height
        root.check(frame.height === root.comboHeight, tag + " frame is " + frame.height + " tall, another host drew " + root.comboHeight)
        root.checkPlacement(tag, box, cell, want)
        root.check(line <= frame.height - 2 * root.hairline, tag + " text line " + line + " does not fit the frame " + frame.height + " less its two hairlines")
        root.check(root.whole(scene), tag + " frame " + root.boxText(scene) + " is not on whole pixels")
        root.checkPatch(tag + " at rest", editor, host)
        Checks.neighbours(root, tag, found, scene, want)
        Checks.selection(root, tag, editor)
        if (host.name !== "grid") root.check(Math.abs(box.y + box.height / 2 - cell.height / 2) <= 0.5, tag + " frame " + root.boxText(box) + " is not centred in its " + cell.height + " px row")
        if (host.name === "grid") Checks.grid(root, tag, found, box)
        var text = field.mapToItem(frame, 0, 0)
        root.check(text.x === root.gap, tag + " text starts " + text.x + " inside the frame, not one gap " + root.gap)
        if (found.column) {
            // The row hides its own name, so nothing but the row's wash lies under the editor: no ground of another shade.
            var named = cell.children.filter(function (c) { return c.visible && c.text !== undefined && String(c.text).indexOf(host.target) >= 0 })
            root.check(cell.renaming === true && named.length === 0, tag + " the row still draws its name under the editor")
            var grounds = cell.parent.children.filter(function (c) { return c.visible && String(c).indexOf("QQuickRectangle") === 0 && Checks.overlaps(scene, Checks.sceneBox(c)) })
            root.check(grounds.length === 0, tag + " " + grounds.length + " item(s) besides the row lie under the editor: " + grounds.join(","))
            var cells = root.trailing(cell)
            root.check(cells[0].width > 0, tag + " the size cell is empty, so the trailing cell was not measured")
            root.check(host.target !== "sub" || cells.length === 2, tag + " the chosen folder draws no chevron")
            root.check(box.x + box.width === cells[0].x - root.gap, tag + " frame ends at " + (box.x + box.width) + ", not one gap " + root.gap + " before the size cell at " + cells[0].x)
            for (var i = 0; i < cells.length; i++)
                root.check(box.x + box.width <= cells[i].x, tag + " frame ends at " + (box.x + box.width) + " over the " + cells[i].name + " cell at " + cells[i].x)
        }
        return true
    }

    // A row that holds the frame keeps it whole inside; a row shorter than the frame is overhung by the deficit, split top and bottom.
    // Sample input: a 15 px frame at y=0 in a 14 px row overhangs 0 above and 1 below, which is within one pixel of even.
    function checkPlacement(tag, box, cell, want) {
        if (cell.height >= want) {
            root.check(root.inside(box, cell.width, cell.height), tag + " frame " + root.boxText(box) + " is not inside its row " + cell.width + "x" + cell.height)
            return
        }
        var above = -box.y
        var below = box.y + box.height - cell.height
        root.overhung++
        root.check(box.height === want, tag + " overhanging frame is " + box.height + " tall, not the typed line and two hairlines " + want)
        root.check(Math.abs(above - below) <= 1, tag + " frame " + root.boxText(box) + " overhangs its " + cell.height + " px row by " + above + " above and " + below + " below, not evenly")
        root.check(box.x >= 0 && box.x + box.width <= cell.width, tag + " overhanging frame " + root.boxText(box) + " leaves its row sideways")
    }

    // The extension's muted patch shows while the stem alone is selected, so a rename of a.txt reads as ".txt" muted.
    function checkPatch(tag, editor, host) {
        if (host.target !== "a.txt") return
        var field = editor.inputItem
        root.check(editor.extensionPatch.visible, tag + " extension patch is hidden: selection " + field.selectionStart + "-" + field.selectionEnd + " of '" + field.text + "', focus " + field.activeFocus)
    }

    // The error line is read where the host draws it: the frame keeps its height, and the whole line stays in view.
    function measureError(tag, host) {
        var found = root.hostOf(host)
        if (!found) return false
        if (!root.errorMeasured) {
            var rest = found.editor.frame.mapToItem(found.column || found.cell, 0, 0, found.editor.frame.width, found.editor.frame.height)
            root.roomBelow = found.column ? found.column.height - (rest.y + rest.height) : 0
            pane.renameError = root.errorSample
            root.errorMeasured = true
            return false
        }
        var editor = found.editor
        var frame = editor.frame
        var box = frame.mapToItem(found.cell, 0, 0, frame.width, frame.height)
        root.check(frame.height === root.comboHeight, tag + " error frame is " + frame.height + " tall, not the one line box " + root.comboHeight)
        root.check(Qt.colorEqual(frame.border.color, Flea.Theme.color.error), tag + " error frame is not the error role")
        var labels = root.texts(editor, []).filter(function (t) { return t.text === root.errorSample && t.visible })
        root.check(labels.length === 1, tag + " draws " + labels.length + " error lines, not one")
        if (labels.length === 1) {
            var line = labels[0].mapToItem(null, 0, 0, labels[0].width, labels[0].height)
            var under = frame.mapToItem(null, 0, 0, frame.width, frame.height)
            root.check(line.y >= under.y + under.height, tag + " error line " + line.y + " is not under the frame ending " + (under.y + under.height))
            root.check(root.inside(line, window.width, window.height), tag + " error line " + root.boxText(line) + " leaves the window")
            if (found.column) Checks.columnError(root, tag, host, found, labels[0], line)
        }
        root.check(root.whole(frame.mapToItem(null, 0, 0, frame.width, frame.height)), tag + " error frame is not on whole pixels")
        root.check(box.y >= 0, tag + " error frame moved above its row")
        pane.renameError = ""
        root.checkPatch(tag + " after the error clears", editor, host)
        root.errorMeasured = false
        return true
    }

    function buildCombos() {
        if (root.overhangPass) {
            root.combos = [{ stop: root.overhangStop, density: "tight" }]
            return
        }
        var out = []
        for (var i = 0; i < root.allStops.length; i++)
            for (var d = 0; d < root.densities.length; d++)
                out.push({ stop: root.allStops[i], density: root.densities[d] })
        root.combos = out
    }

    // The column takes its parent's height back, the binding ColumnsArea gave it.
    function releaseColumn() {
        var column = root.squeezed
        column.height = Qt.binding(function () { return column.parent.height })
        root.squeezed = null
    }

    function runsAt(host, combo) { return host.onlyStop === undefined || host.onlyStop === combo.stop }
    function hostsAt(combo) { return root.hosts.filter(function (h) { return root.runsAt(h, combo) }).length }

    function closeEditor(host) {
        if (host.name === "rail") pane.sidebar.cancelRename()
        else pane.renamingIndex = -1
    }

    // Each stage returns true when done and false to be asked again on the next tick.
    function advance() {
        var combo = root.combos[root.comboIndex]
        // Stage 6 runs with hostIndex past the last host, which is why the host is read last.
        var host = root.hosts[Math.min(root.hostIndex, root.hosts.length - 1)]
        var tag = "stop " + combo.stop + " " + combo.density + " " + host.name
        if (root.stage === 0) {
            if (!root.ready()) return false
            Flea.ViewState.setTextSize({ mode: combo.stop })
            Flea.ViewState.changeKey("density", combo.density)
            root.hostIndex = 0
            root.comboHeight = -1
            root.overhungAtStart = root.overhung
            root.stage = 1
            return true
        }
        if (root.stage === 1) {
            if (Flea.ViewState.state.view !== host.view) { pane.chooseView(host.view); return false }
            if (!root.ready()) return false
            if (host.name === "rail") {
                if (pane.sidebar.networkEntries.length === 0) return false
                pane.sidebar.startRename(pane.sidebar.placesEntries.length)
                root.stage = 3
                return true
            }
            pane.listArea.forceActiveFocus()
            pane.clearSelection()
            pane.setCursor(root.indexOfName(host.target))
            root.stage = 2
            return true
        }
        if (root.stage === 2) {
            // The cursor lands on the named row before F2 asks to rename it.
            if (pane.cursorIndex !== root.indexOfName(host.target)) return false
            driver.keyClick(Qt.Key_F2, Qt.NoModifier, -1)
            root.stage = 3
            return true
        }
        if (root.stage === 3) {
            if (!root.measure(tag, host)) return false
            // The rail has no error line: its editor has no pane to read one from, so it is measured at rest only.
            root.stage = host.name === "rail" ? 5 : host.last ? 7 : 4
            return true
        }
        if (root.stage === 7) {
            // The last row's error only needs the scroll when the viewport ends where the rows do, so the column is cut to that.
            var found = root.hostOf(host)
            if (!found || !found.column) return false
            var column = found.column
            column.height = pane.shownTotal * Flea.Theme.fileRowHeight + Flea.Theme.spacing.rowPaddingY
            root.squeezed = column
            root.stage = 4
            return true
        }
        if (root.stage === 4) {
            if (!root.measureError(tag, host)) return false
            root.stage = 5
            return true
        }
        if (root.stage === 5) {
            root.closeEditor(host)
            if (root.squeezed) root.releaseColumn()
            root.hostIndex++
            while (root.hostIndex < root.hosts.length && !root.runsAt(root.hosts[root.hostIndex], combo)) root.hostIndex++
            root.stage = root.hostIndex >= root.hosts.length ? 6 : 1
            return true
        }
        if (root.stage === 6) {
            // The overhang arm is only worth its code while a stop still exercises it, so the smallest Tight stop must under the overhang pass's face.
            if (root.overhangPass && combo.stop === root.overhangStop && combo.density === "tight")
                root.check(root.overhung > root.overhungAtStart, "stop " + combo.stop + " tight overhung no frame, so the overhang arm went unmeasured")
            root.stage = 0
            root.comboIndex++
            return true
        }
        return true
    }

    function report() {
        stepper.running = false
        // A host that never opened its editor is a failure: every combo measures every host that runs at its stop.
        var want = 0
        for (var c = 0; c < root.combos.length; c++) want += root.hostsAt(root.combos[c])
        root.check(root.measured === want, "measured " + root.measured + " editors, not " + want + " (" + root.combos.length + " stops and densities, " + root.hosts.length + " hosts)")
        console.log("RENAMEFRAME OVERHANG measured=" + root.overhung)
        console.log("RENAMEFRAME DONE checks=" + root.checks + " failed=" + root.failures.length)
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }

    Component.onCompleted: root.buildCombos()

    FloatingWindow {
        id: window
        implicitWidth: 1000
        implicitHeight: 650
        Flea.WindowBody { id: body; host: window }
        Item { anchors.fill: parent; TestEvent { id: driver } }
    }

    TextMetrics { id: metricsProbe }

    Timer {
        id: stepper
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            if (root.comboIndex >= root.combos.length) {
                root.report()
                return
            }
            var before = root.stage
            var beforeHost = root.hostIndex
            var beforeCombo = root.comboIndex
            if (root.advance() === true) {
                root.stageTicks = 0
                return
            }
            root.stageTicks++
            if (root.stageTicks > root.stallTicks) {
                root.failures.push("stage " + before + " of combo " + beforeCombo + " host " + beforeHost + " never completed")
                console.log("RENAMEFRAME FAIL stalled at stage " + before + " combo " + beforeCombo + " host " + root.hosts[Math.min(beforeHost, root.hosts.length - 1)].name + " index=" + pane.renamingIndex + " cursor=" + pane.cursorIndex + " view=" + Flea.ViewState.state.view)
                root.report()
            }
        }
    }
}
