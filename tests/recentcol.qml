import QtQuick
import "flea" as Flea
import "flea/js/Columns.js" as Columns

// Sidebar040: inspect the actual Header and Row items, including the lazy path split.
Item {
    id: root
    readonly property int paneWidth: 800
    readonly property int locationWidth: 150
    readonly property int wideDateWidth: 220
    readonly property int belowFloor: 1
    readonly property int hiddenLocationFloor: 375
    readonly property int hiddenUsedFloor: 350
    readonly property var premiseTokens: ({rowPaddingX: 14, gap: 9, iconSize: 23, nameMin: 156, size: 70, date: 125, location: root.locationWidth})
    readonly property real nameFloor: 2 * Flea.Theme.spacing.rowPaddingX + Flea.Theme.iconSize + Flea.Theme.spacing.gap + Flea.Theme.column.nameMin
    readonly property real locationFloor: root.nameFloor + Flea.Theme.column.size + Flea.Theme.column.date + root.locationWidth + 3 * Flea.Theme.spacing.gap
    readonly property real dualNameFloor: 2 * Flea.Theme.spacing.rowPaddingX + Flea.Theme.markSize + Flea.Theme.spacing.gap + Flea.Theme.dualColumn.nameMin
    readonly property real dualLocationFloor: root.dualNameFloor + Flea.Theme.dualColumn.size + Flea.Theme.dualColumn.date + root.locationWidth + Flea.Theme.spacing.gap
    readonly property var file: ({n: "Documents/a-very-long-parent-directory/another-long-parent-directory/notes.txt", p: 33188, d: false, s: 18000, m: 1758835200})
    readonly property var longFile: Object.assign({}, root.file, {n: "Documents/a-very-long-parent-directory/another-long-parent-directory/a-very-long-filename-that-must-use-the-whole-name-column-and-keep-its-extension.txt"})

    Flea.Header {
        id: header
        width: root.paneWidth
        recent: true
        hiddenCols: []
        sortBy: "mtime"
        sortDesc: true
        pane: ({rows: [root.file], kindNames: []})
    }
    Flea.Row { id: recent; width: header.contentWidth; row: root.file; recenting: true; hiddenCols: [] }
    Flea.Row { id: longRecent; width: recent.width; row: root.longFile; recenting: true; hiddenCols: [] }
    Flea.Header { id: floorHeader; width: root.locationFloor + Flea.Theme.spacing.rowPaddingX; recent: true; hiddenCols: [] }
    Flea.Row { id: floorRow; width: floorHeader.contentWidth; row: root.file; recenting: true; hiddenCols: [] }
    Flea.Header { id: narrowHeader; width: root.locationFloor - root.belowFloor + Flea.Theme.spacing.rowPaddingX; recent: true; hiddenCols: [] }
    Flea.Row { id: narrow; width: narrowHeader.contentWidth; row: root.file; recenting: true; hiddenCols: [] }
    Flea.Row { id: search; width: recent.width; row: root.file; searchQuery: "notes"; hiddenCols: [] }
    Flea.Row { id: plain; width: recent.width; row: root.file; hiddenCols: [] }
    Flea.Header { id: changingHeader; width: root.paneWidth; hiddenCols: [] }
    Flea.Row { id: changing; row: root.file; hiddenCols: []; assignedCols: Flea.Theme.columns(width, hiddenCols) }
    Flea.Header {
        id: dualHeader
        width: floorHeader.width
        dualMode: true
        recent: true
        hiddenCols: []
    }
    Flea.Row {
        id: dualRow
        width: dualHeader.contentWidth
        row: root.file
        dualMode: true
        recenting: true
        hiddenCols: []
    }
    Flea.Header {
        id: dualFloorHeader
        width: root.dualLocationFloor + Flea.Theme.spacing.rowPaddingX
        dualMode: true
        recent: true
        hiddenCols: []
    }
    Flea.Row {
        id: dualFloorRow
        width: dualFloorHeader.contentWidth
        row: root.file
        dualMode: true
        recenting: true
        hiddenCols: []
    }
    Flea.Header {
        id: dualNarrowHeader
        width: root.dualLocationFloor - root.belowFloor + Flea.Theme.spacing.rowPaddingX
        dualMode: true
        recent: true
        hiddenCols: []
    }
    Flea.Row {
        id: dualNarrowRow
        width: dualNarrowHeader.contentWidth
        row: root.file
        dualMode: true
        recenting: true
        hiddenCols: []
    }
    Component {
        id: mouseControl
        Item {
            MouseArea { anchors.fill: parent }
        }
    }
    Component {
        id: siblingControl
        Item {
            width: parent.width
            height: parent.height
            property alias buttons: tap.acceptedButtons
            property alias inputEnabled: tap.enabled
            TapHandler {
                id: tap
                acceptedButtons: Qt.LeftButton
            }
        }
    }

    function inputHandlers(item, walk) {
        var count = 0
        walk(item, function (o) {
            if (o instanceof PointerHandler || o instanceof MouseArea)
                count += 1
        })
        return count
    }

    // Menu checks require the header's own full-area TapHandler; other checks include all overlapping handlers.
    function titleInputHandlers(headerItem, title, button, walk, headerMenuOnly) {
        var count = 0
        walk(headerItem, function (o) {
            if (!(o instanceof PointerHandler || o instanceof MouseArea))
                return
            if (!o.enabled || !(o.acceptedButtons & button))
                return
            var area = o instanceof MouseArea ? o : o.parent
            if (headerMenuOnly && (!(o instanceof TapHandler) || o.parent !== headerItem
                || area !== headerItem || o.acceptedButtons !== Qt.RightButton || o.margin !== 0))
                return
            if (!area || !area.visible || !area.enabled || area.width <= 0 || area.height <= 0)
                return
            var start = title.mapToItem(area, 0, 0)
            var end = title.mapToItem(area, title.width, title.height)
            var margin = o.margin === undefined ? 0 : o.margin
            if (start.x < area.width + margin && end.x > -margin
                && start.y < area.height + margin && end.y > -margin)
                count += 1
        })
        return count
    }

    function locationInput(check, walk, title) {
        check("No header left-button handler covers Location", root.titleInputHandlers(header, title, Qt.LeftButton, walk), 0)
        check("One right-button handler covers Location", root.titleInputHandlers(header, title, Qt.RightButton, walk), 1)
        check("Location right-button handler belongs to the whole-header menu", root.titleInputHandlers(header, title, Qt.RightButton, walk, true), 1)
        var menu = null
        walk(header, function (o) {
            if (o instanceof TapHandler && o.parent === header && o.acceptedButtons === Qt.RightButton)
                menu = o
        })
        check("Header owns its right-button menu handler", menu !== null, true)
        if (!menu)
            return
        var control = siblingControl.createObject(header)
        check("Synthetic sibling TapHandler was built", control !== null, true)
        if (!control)
            return
        check("Synthetic sibling left-button handler makes Location exclusion fail", root.titleInputHandlers(header, title, Qt.LeftButton, walk), 1)
        control.buttons = Qt.RightButton
        check("Synthetic sibling right-button handler allows Location exclusion", root.titleInputHandlers(header, title, Qt.LeftButton, walk), 0)
        check("Synthetic sibling right-button handler is inspected", root.titleInputHandlers(header, title, Qt.RightButton, walk), 2)
        check("Foreign right-button handler does not count as the whole-header menu", root.titleInputHandlers(header, title, Qt.RightButton, walk, true), 1)
        menu.enabled = false
        check("Foreign right-button handler alone still covers Location", root.titleInputHandlers(header, title, Qt.RightButton, walk), 1)
        check("Foreign right-button handler alone cannot satisfy the whole-header menu check", root.titleInputHandlers(header, title, Qt.RightButton, walk, true), 0)
        menu.enabled = true
        check("Restored whole-header menu covers Location", root.titleInputHandlers(header, title, Qt.RightButton, walk, true), 1)
        control.buttons = Qt.LeftButton
        control.inputEnabled = false
        check("Disabled sibling handler allows Location exclusion", root.titleInputHandlers(header, title, Qt.LeftButton, walk), 0)
        control.inputEnabled = true
        control.visible = false
        check("Hidden sibling handler allows Location exclusion", root.titleInputHandlers(header, title, Qt.LeftButton, walk), 0)
        control.visible = true
        control.x = header.width
        check("Sibling handler outside Location allows exclusion", root.titleInputHandlers(header, title, Qt.LeftButton, walk), 0)
        control.destroy()
    }

    // Count built Location texts independently of their value and visibility; look up drawn texts separately.
    function texts(row, walk) {
        var out = {name: null, location: null, locations: 0}
        walk(row, function (o) {
            if (String(o).indexOf("MatchText") === 0 && o.visible && o.text === row.decoratedName) out.name = o
            if (o.elide === Text.ElideLeft) out.locations += 1
            if (o.visible && o.elide !== undefined && o.text === row.locationText && row.locationText.length > 0) out.location = o
        })
        return out
    }

    // List can deliver recenting before its shared assignedCols binding catches up.
    function transitions(check, walk) {
        check("Unlaid row has boolean Location", changing.cols.location, false)
        changing.width = recent.width
        var hidden = ["mode", "kind", "size", "date"]
        var sets = [Flea.Theme.columns(changing.width, []), Flea.Theme.columns(changing.width, hidden),
            Flea.Theme.dualColumns(changing.width, []), Flea.Theme.dualColumns(changing.width, hidden)]
        for (var i = 0; i < sets.length; i++) {
            changing.assignedCols = sets[i]
            changing.recenting = true
            check("Recent transition " + i + " keeps old Location boolean", changing.cols.location, false)
            check("Recent transition " + i + " reports the old assignment", changing.columnSet(), Columns.names(sets[i]))
            check("Recent transition " + i + " seam agrees with hidden Location", changing.columnSet().indexOf("location") >= 0, root.texts(changing, walk).location !== null)
            check("Recent transition " + i + " builds its location text", root.texts(changing, walk).locations, 1)
            changing.assignedCols = Flea.Theme.columns(changing.width, i % 2 ? hidden : [], undefined, true)
            check("Recent transition " + i + " rebuilds Location", changing.cols.location, true)
            check("Recent transition " + i + " draws Location", root.texts(changing, walk).location !== null, true)
            check("Recent transition " + i + " reports the Recent assignment", changing.columnSet(), Columns.names(changing.assignedCols))
            check("Recent transition " + i + " seam agrees with drawn Location", changing.columnSet().indexOf("location") >= 0, root.texts(changing, walk).location !== null)
            changing.recenting = false
        }
        changingHeader.recent = true
        check("Header transition draws Location", changingHeader.cols.location, true)
        changingHeader.hiddenCols = hidden
        check("Hidden metadata keeps Recent Location", changingHeader.cols.location, true)
        changingHeader.dualMode = true
        check("Dual Recent transition keeps Recent on", changingHeader.recent, true)
        check("Dual Recent transition keeps Location", changingHeader.cell("location").visible, true)
        check("Dual Recent transition reports its drawn Location", changingHeader.columnSet().indexOf("location") >= 0, changingHeader.cell("location").visible)
        changingHeader.recent = false
        changingHeader.dualMode = false
        changingHeader.hiddenCols = []
        check("Ordinary header uses the single-pane path", changingHeader.dualMode, false)
        check("Ordinary header has boolean Location", changingHeader.cols.location, false)
        check("Ordinary header keeps its Mode cell reader", changingHeader.cell("mode") !== null, true)
        check("Ordinary header Mode cell stays visible", changingHeader.cell("mode").visible, true)
        check("Ordinary header Mode cell keeps its title", changingHeader.cell("mode").text, "Mode")
        check("Ordinary header exposes no Location cell", changingHeader.cell("location"), null)
        changingHeader.dualMode = true
        check("Dual header uses the dual-pane path", changingHeader.dualMode, true)
        check("Dual header has boolean Location", changingHeader.cols.location, false)
    }

    function dualRecent(check, walk) {
        var dualTexts = root.texts(dualRow, walk)
        // Hinting and font advances can put the single-pane floor on either side of the dual floor.
        check("Dual Recent follows its own floor at the single-pane width", dualHeader.cols.location, root.locationFloor >= root.dualLocationFloor)
        check("Dual Recent header and row sets agree", dualRow.columnSet(), dualHeader.columnSet())
        check("Dual Recent builds its name", dualTexts.name !== null, true)
        if (dualTexts.name)
            check("Dual Recent name stays above the dual floor", dualTexts.name.width >= Flea.Theme.dualColumn.nameMin, true)
        var full = root.texts(dualFloorRow, walk)
        check("Dual Recent draws Location at its exact floor", dualFloorHeader.columnSet(), "name,location,size,date")
        check("Dual Recent exact-floor row matches its header", dualFloorRow.columnSet(), dualFloorHeader.columnSet())
        check("Dual Recent builds both drawn path cells", full.name !== null && full.location !== null, true)
        var below = root.texts(dualNarrowRow, walk)
        check("Dual Recent drops Location one pixel below its floor", dualNarrowHeader.cols.location, false)
        check("Dual Recent below-floor row drops Location", dualNarrowRow.columnSet().indexOf("location"), -1)
        check("Dual Recent below-floor header and row sets agree", dualNarrowRow.columnSet(), dualNarrowHeader.columnSet())
        check("Dual Recent below-floor row draws no Location", below.location !== null, false)
        check("Dual Recent uses the dual Size width", dualFloorRow.cell("size").width, Flea.Theme.dualColumn.size)
        check("Dual Recent Size header matches its row", dualFloorHeader.cell("size").width, dualFloorRow.cell("size").width)
        if (full.name && full.location) {
            var locationX = full.location.mapToItem(dualFloorRow, 0, 0).x
            check("Dual Recent Location header aligns with its row", dualFloorHeader.cell("location").x, locationX)
            check("Dual Recent keeps the zero Location-to-Size gap", locationX + full.location.width, dualFloorRow.cell("size").x)
            check("Dual Recent location keeps the board width", full.location.width, root.locationWidth)
            check("Dual Recent exact-floor name stays above the dual minimum", full.name.width >= Flea.Theme.dualColumn.nameMin, true)
        }
    }

    // Sweep every pixel so Location cannot appear before configured metadata or disappear while widening.
    function wideningRecent(check) {
        var panes = [{name: "List", dual: false}, {name: "Dual", dual: true}]
        var hiddenSets = [[], ["size"], ["date"], ["size", "date"]]
        for (var p = 0; p < panes.length; p++) {
            for (var h = 0; h < hiddenSets.length; h++) {
                var hidden = hiddenSets[h]
                var label = panes[p].name + " Recent hidden=" + hidden.join(",")
                var locationSeen = false
                var missingMetadataWidth = -1
                var disappearedWidth = -1
                for (var width = 0; width <= root.paneWidth; width++) {
                    var cols = Flea.Theme.columns(width, hidden, root.wideDateWidth, true, panes[p].dual)
                    if (cols.location && missingMetadataWidth < 0
                        && ((hidden.indexOf("size") < 0 && !cols.size)
                            || (hidden.indexOf("date") < 0 && !cols.date)))
                        missingMetadataWidth = width
                    if (locationSeen && !cols.location && disappearedWidth < 0)
                        disappearedWidth = width
                    if (cols.location)
                        locationSeen = true
                }
                check(label + " Location never precedes configured metadata (first bad px)", missingMetadataWidth, -1)
                check(label + " Location stays drawn while widening (first bad px)", disappearedWidth, -1)
                check(label + " sweep reaches drawn Location", locationSeen, true)
            }
        }
    }

    function run(check, walk) {
        root.wideningRecent(check)
        root.transitions(check, walk)
        root.dualRecent(check, walk)
        check("Hidden Size and Used release Location at 375px", Columns.recentSet(root.hiddenLocationFloor, root.premiseTokens, ["size", "date"]).location, true)
        check("Hidden Size releases Used at 350px", Columns.recentSet(root.hiddenUsedFloor, root.premiseTokens, ["size"]).date, true)
        check("Hidden Size and Used keep Location hidden one pixel below 375px", Columns.recentSet(root.hiddenLocationFloor - root.belowFloor, root.premiseTokens, ["size", "date"]).location, false)
        check("Hidden Size keeps Used hidden one pixel below 350px", Columns.recentSet(root.hiddenUsedFloor - root.belowFloor, root.premiseTokens, ["size"]).date, false)
        var r = root.texts(recent, walk)
        var l = root.texts(longRecent, walk)
        var s = root.texts(search, walk)
        var n = root.texts(narrow, walk)
        var h = header.cell("location")
        check("Recent titles match Sidebar040", header.titles(), "Name|Location|Size|Used")
        check("Recent header names its drawn columns", header.columnSet(), "name,location,size,date")
        check("Recent row names its drawn columns", recent.columnSet(), header.columnSet())
        check("Recent exposes its Location header", h !== null, true)
        check("Recent exposes no Mode header", header.cell("mode"), null)
        check("Location header is visible with non-zero width", h !== null && h.visible && h.width > 0, true)
        check("Recent builds its name and location", r.name !== null && r.location !== null, true)
        if (r.name && r.location) {
            var locationX = r.location.mapToItem(recent, 0, 0).x
            check("Location header and row share the exact left edge", h ? h.x : -1, locationX)
            check("Recent location has the board's fixed width", r.location.width, root.locationWidth)
            check("Recent location head elides", r.location.elide, Text.ElideLeft)
            check("Recent location is long enough to elide", r.location.truncated, true)
            check("Recent location is left aligned", r.location.horizontalAlignment, Text.AlignLeft)
            check("Recent location uses caption type", r.location.font.pixelSize, Flea.Theme.font.caption)
            check("Recent location uses row metadata ink", String(r.location.color), String(recent.cellInk))
            check("Recent name fills its own column", r.name.width, r.location.x - Flea.Theme.spacing.gap)
            check("Recent location ends before Size", locationX + r.location.width + Flea.Theme.spacing.gap, recent.cell("size").x)
            check("Recent short name no longer shrinks its slot", r.name.width > r.name.implicitWidth, true)
        }
        if (h) {
            check("Location header uses fixed width", h.width, root.locationWidth)
            check("Location header is left aligned", h.horizontalAlignment, Text.AlignLeft)
            check("Location header shares title ink", String(h.color), String(header.cell("name").color))
            check("Location header shares title weight", h.font.weight, header.cell("name").font.weight)
            check("Location header has no input handler", root.inputHandlers(h, walk), 0)
            root.locationInput(check, walk, h)
            var control = mouseControl.createObject(h)
            check("Synthetic MouseArea control was built", control !== null, true)
            check("Synthetic MouseArea makes Location exclusion fail", root.inputHandlers(h, walk) === 0, false)
            if (control)
                control.destroy()
        }
        var modeHandle = null
        walk(header, function (o) {
            if (o.columnKey === "mode")
                modeHandle = o
        })
        check("Recent builds its Mode resize handle for inspection", modeHandle !== null, true)
        if (modeHandle)
            check("Recent Mode resize handle is hidden", modeHandle.visible, false)
        check("Used alone has the descending arrow", header.cell("date").text, "Used ▾")
        if (r.location && l.location) {
            check("Location never follows name length", l.location.mapToItem(longRecent, 0, 0).x, r.location.mapToItem(recent, 0, 0).x)
            check("Recent long name keeps the entire name slot", l.name.width, r.name.width)
        }
        check("Location draws exactly at its name floor", floorHeader.columnSet(), "name,location,size,date")
        check("Row keeps Location at the exact floor", floorRow.columnSet(), floorHeader.columnSet())
        check("Location drops below its name floor", narrowHeader.columnSet().indexOf("location"), -1)
        check("Narrow Recent header and row agree", narrow.columnSet(), narrowHeader.columnSet())
        check("Narrow Recent titles omit Location", narrowHeader.titles(), "Name|Size|Used")
        check("Narrow Recent draws no location", n.location ? n.location.visible && n.location.width > 0 : false, false)
        check("Narrow Recent draws its name", n.name !== null && n.name.visible, true)
        if (n.name) check("Narrow Recent name fills the available slot", n.name.width, narrow.cell("size").x - n.name.mapToItem(narrow, 0, 0).x - Flea.Theme.spacing.gap)
        check("Search builds both path texts", s.name !== null && s.location !== null, true)
        if (s.name && s.location) {
            check("Search keeps its inline name share", s.name.width, Math.min(s.name.implicitWidth, search.searchSlot * search.nameShare))
            check("Search location follows its name", s.location.x, s.name.width + Flea.Theme.spacing.gap)
            check("Search location keeps head elision", s.location.elide, Text.ElideLeft)
        }
        check("Ordinary row builds no location text", root.texts(plain, walk).locations, 0)
        console.log("RECENTCOL floor=" + root.locationFloor + " row-width; pane-floor=" + (root.locationFloor + Flea.Theme.spacing.rowPaddingX))
    }
}
