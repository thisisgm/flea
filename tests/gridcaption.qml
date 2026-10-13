//@ pragma ShellId flea-gridcaption-test

import QtQuick
import Quickshell
import "flea" as Flea
import "flea/js/GridGeometry.js" as GridGeometry
import "flea/js/Density.js" as Density

// GridTile caption geometry at text sizes 12 and 14 (the old slot must fail at 14), the caption alone at rest, the drop label added under a drag.
ShellRoot {
    id: root

    // The native witness name, so GridNames wraps and elides exactly the failing caption.
    readonly property string longName: "long-grid-caption-with-enough-words-to-wrap-and-truncate-after-two-complete-lines.txt"
    readonly property var fileRow: ({ n: root.longName, i: "text-x-generic", p: 0, d: false, s: 1, m: 0, t: false, v: 0 })

    // The real cell height GridArea gives a tile, so bottom<=height means the window contains it.
    function cellH() {
        return GridGeometry.cellHeightFor(Flea.ViewState.thumbnailPixels, Flea.Theme.spacing.gap,
            Math.ceil(Flea.Theme.grid.captionHeight),
            Density.gridPadY(Flea.Theme.spacing.rowPaddingX, Flea.ViewState.density))
    }

    Column {
        Flea.GridTile { id: normalTile; width: 147; height: root.cellH(); row: root.fileRow }
        Flea.GridTile { id: dropTile; width: 147; height: root.cellH(); row: root.fileRow; dropTarget: true }
        Flea.GridTile { id: wrapTile; width: 147; height: root.cellH(); row: null }
    }

    property int phase: 0
    property var failures: []

    // Each phase lands a turn after the last write, so the relayout it measures has settled.
    Timer {
        id: stepper
        interval: 700
        running: true
        repeat: true
        onTriggered: root.step()
    }

    function check(cond, label) {
        if (!cond)
            root.failures.push(label)
    }

    // Text items under a tile, each laid out when built whether drawn or not.
    function texts(item) {
        var found = [], stack = [item]
        while (stack.length > 0) {
            var o = stack.pop()
            if (o !== item && String(o).indexOf("QQuickText") === 0) found.push(o)
            for (var i = 0; i < o.children.length; i++) stack.push(o.children[i])
        }
        return found
    }

    function measure(base, withNegative) {
        var lineH = Flea.Theme.grid.captionLineHeight
        var cap = normalTile.captionItem
        var ntext = String(cap.text)
        console.log("GRIDCAPTION MEASURE base=" + base + " tile=normal lines=" + cap.lineCount + " textHeight=" + cap.contentHeight + " slotHeight=" + cap.height + " bottom=" + (cap.y + cap.height) + " tileHeight=" + normalTile.height)
        root.check(cap.lineCount === 2, "base " + base + " normal lines " + cap.lineCount)
        root.check(ntext.slice(-4) === ".txt", "base " + base + " normal tail " + ntext.slice(-12))
        root.check(ntext.indexOf("…") >= 0, "base " + base + " normal no ellipsis")
        root.check(Math.ceil(cap.contentHeight) <= cap.height, "base " + base + " normal textHeight " + cap.contentHeight + " over slot " + cap.height)
        root.check(cap.y + cap.height <= normalTile.height, "base " + base + " normal bottom past tile")
        // Full hinting can fit the fractional line box exactly; its rounded pixel extent must still fit.
        if (withNegative)
            root.check(Math.ceil(cap.contentHeight) > 2 * lineH, "base " + base + " old slot already contained it")
        var dcap = dropTile.captionItem
        console.log("GRIDCAPTION MEASURE base=" + base + " tile=drop lines=" + dcap.lineCount + " textHeight=" + dcap.contentHeight + " slotHeight=" + dcap.height + " bottom=" + (dcap.y + dcap.height) + " tileHeight=" + dropTile.height)
        root.check(dcap.lineCount === 1, "base " + base + " drop lines " + dcap.lineCount)
        root.check(Math.ceil(dcap.contentHeight) <= dcap.height, "base " + base + " drop textHeight " + dcap.contentHeight + " over slot " + dcap.height)
        root.check(dcap.y + dcap.height <= dropTile.height, "base " + base + " drop bottom past tile")
        // The hover tip follows Flea's own elide: on for the cut name, which Qt sees whole, off for a name that wraps whole.
        root.check(!cap.truncated, "base " + base + " Qt cut the caption Flea elided")
        normalTile.hovered = true
        root.check(root.tipShows(normalTile, root.longName), "base " + base + " hovered elided caption shows no tip")
        normalTile.hovered = false
        root.check(!root.tipShows(normalTile, root.longName), "base " + base + " tip stays without hover")
        var per = Math.floor(wrapTile.captionPerLine)
        // A break after a hyphen and one after a space: both keep every char, so neither caption is cut.
        var seps = ["-", " "]
        wrapTile.hovered = true
        for (var k = 0; k < seps.length; k++) {
            var name = "a".repeat(per - 1) + seps[k] + "bbb.txt"
            wrapTile.row = { n: name, i: "text-x-generic", p: 0, d: false, s: 1, m: 0, t: false, v: 0 }
            var wtext = String(wrapTile.captionItem.text)
            root.check(wtext.indexOf("\n") > 0 && wtext.indexOf("…") < 0, "base " + base + " wrap witness " + JSON.stringify(wtext))
            root.check(!root.tipShows(wrapTile, name), "base " + base + " whole wrapped caption " + JSON.stringify(wtext) + " shows a tip")
        }
        wrapTile.hovered = false
        var rest = root.texts(normalTile), drop = root.texts(dropTile)
        root.check(rest.length === 1 && rest[0] === cap, "base " + base + " a resting tile lays out " + rest.length + " Texts, want its caption alone")
        root.check(drop.length === 2 && drop.indexOf(dcap) >= 0, "base " + base + " a drop-target tile lays out " + drop.length + " Texts, want its caption and drop label")
        root.check(drop.some(function (t) { return t.text === "move here" }), "base " + base + " a drop-target tile labels no move")
    }

    // Whether the tile's tip is built and shows the whole name.
    function tipShows(tile, name) {
        for (var i = 0; i < tile.children.length; i++) {
            var c = tile.children[i]
            if (c.objectName === "nameTip")
                return c.active && c.item !== null && c.item.children[0].text === name
        }
        root.failures.push("no nameTip loader")
        return false
    }

    function step() {
        if (root.phase === 0)
            Flea.ViewState.setTextSize({ mode: 12 })
        else if (root.phase === 1)
            root.measure(12, false)
        else if (root.phase === 2)
            Flea.ViewState.setTextSize({ mode: 14 })
        else if (root.phase === 3)
            root.measure(14, true)
        else
            root.report()
        root.phase++
    }

    function report() {
        stepper.running = false
        if (root.failures.length === 0) {
            console.log("GRIDCAPTION PASS base=12,14 normal=2lines drop=1line texts=1,2")
            // The hover tip half of the same pass, beside the gate's single-suffix line.
            console.log("GRIDCAPTION TIP tip=elided-only")
        }
        for (var f = 0; f < root.failures.length; f++)
            console.log("GRIDCAPTION FAIL " + root.failures[f])
        Quickshell.execDetached(["kill", String(Quickshell.processId)])
    }
}
