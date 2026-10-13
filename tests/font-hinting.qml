//@ pragma ShellId flea-font-hinting-test
//@ pragma NativeTextRendering

import QtQuick
import QtQuick.Window
import Quickshell
import "flea" as Flea

// Read the production fonts, including lazy match runs, after each text-size change.
ShellRoot {
    id: root
    property int stage: 0
    property int checks: 0
    property var failures: []
    readonly property var sizes: [9, 10, 11, 12, 14, 16, 20]
    readonly property var sample: ({ n: "sample-file.txt", d: false, i: "text-x-generic", p: 420, s: 1234, m: 1758835200, t: false, k: 0, v: 0 })

    Window {
        visible: true
        width: 1000
        height: 700
        Column {
            Flea.MatchText { id: match; width: 900; height: 32; text: root.sample.n }
            Flea.Row { id: row; width: 900; row: Object.assign({}, root.sample, { n: "Reference/" + root.sample.n }); hiddenCols: [] }
            Flea.ColumnRow { id: columnRow; width: 900; row: root.sample; showSize: true; pane: searchPane }
            Flea.GridTile { id: tile; width: Flea.Theme.grid.cellWidth; height: Flea.Theme.grid.cellHeight; row: Object.assign({}, root.sample, { n: "a-very-long-synthetic-filename-that-needs-an-elided-grid-caption-and-a-hover-tip.txt" }); hovered: true }
            Flea.RenameField { id: rename; width: 900; height: implicitHeight; name: root.sample.n }
        }
        Flea.FitMetrics { id: fit }
        FontMetrics {
            id: bodyFace
            font.family: Flea.Theme.font.family
            font.pixelSize: Flea.Theme.font.body
            font.hintingPreference: Font.PreferFullHinting
        }
        FontMetrics {
            id: smallFace
            font.family: Flea.Theme.font.family
            font.pixelSize: Flea.Theme.font.bodySmall
            font.hintingPreference: Font.PreferFullHinting
        }
        FontMetrics {
            id: captionFace
            font.family: Flea.Theme.font.family
            font.pixelSize: Flea.Theme.font.caption
            font.hintingPreference: Font.PreferFullHinting
        }
    }

    QtObject {
        id: searchPane
        property string searchMode: ""
        property var swap: ({ answeredQuery: "file", searchWarm: true })
        property string filterQuery: ""
        property bool filterTyping: false
    }

    function check(ok, name) {
        root.checks++
        if (!ok) root.failures.push(name)
    }

    function fonts(item, label) {
        var count = 0
        if ("font" in item && "renderType" in item) {
            root.check(item.font.hintingPreference === Font.PreferFullHinting, label + " hinting " + item.text)
            root.check(item.renderType === Text.NativeRendering, label + " rendering " + item.text)
            root.check(item.font.family === Flea.Theme.font.family, label + " family " + item.text)
            count++
        }
        for (var i = 0; i < item.children.length; i++) count += root.fonts(item.children[i], label)
        return count
    }

    Timer {
        id: stepper
        interval: 200
        running: true
        repeat: true
        onTriggered: {
            var at = Math.floor(root.stage / 2)
            if (at >= root.sizes.length) {
                stepper.stop()
                for (var i = 0; i < root.failures.length; i++) console.log("FONT_HINTING FAIL " + root.failures[i])
                console.log("FONT_HINTING DONE checks=" + root.checks + " failed=" + root.failures.length)
                Qt.quit()
                return
            }
            var size = root.sizes[at]
            if (root.stage % 2 === 0) {
                Flea.ViewState.setTextSize({ mode: size })
                // All three runs must get the same font when the highlight loads later.
                match.matchStart = at % 2 === 0 ? -1 : 7
                match.matchLength = 4
                row.searchQuery = at % 2 === 0 ? "" : "file"
                row.filtering = false
                row.dropTarget = at % 2 !== 0
                searchPane.searchMode = at % 2 === 0 ? "" : "name"
                columnRow.dropTarget = at % 2 !== 0
                tile.dropTarget = at % 2 !== 0
            } else {
                root.check(root.fonts(match, "match " + size) === (match.marked ? 3 : 1), "match run count " + size)
                root.check(root.fonts(row, "list " + size) >= 5, "list text coverage " + size)
                root.check(root.fonts(columnRow, "columns " + size) >= 2, "columns text coverage " + size)
                root.check(root.fonts(tile, "grid " + size) >= 2, "grid caption and lazy drop/tip coverage " + size)
                root.check(root.fonts(rename, "rename " + size) >= 2, "rename text coverage " + size)
                root.check(rename.inputItem.font.pixelSize === Flea.Theme.font.body, "rename size " + size)
                root.check(Math.abs(Flea.Theme.bodyAdvance - bodyFace.advanceWidth("0")) < 0.01, "body budget " + size)
                root.check(Math.abs(Flea.Theme.bodySmallAdvance - smallFace.advanceWidth("0")) < 0.01, "grid budget " + size)
                root.check(fit.cellWidth("size", root.sample, [], null) === Math.ceil(captionFace.advanceWidth("1.2 kB")), "size autofit " + size)
            }
            root.stage++
        }
    }
}
