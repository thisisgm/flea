import QtQuick
import qs.Commons
import qs.Ui
import "." as Flea
import "js/Columns.js" as Columns
import "js/Sort.js" as Sort

// The column header renders sort state and owns none of it, so Pane stays the one state owner.
Item {
    id: root

    property string sortBy: "name"
    property bool sortDesc: false
    property bool dualMode: false
    readonly property real sizeWidth: root.dualMode ? Theme.dualColumn.size : Theme.column.size
    // The chooser's pair, matching ui/Row.qml: the check box slot and SendPicker.html's narrower date.
    property real leadingSlot: 0
    property bool compactDate: false
    readonly property real dateWidth: root.dualMode ? Theme.dualColumn.date : root.compactDate ? Theme.column.pickerDate : Theme.column.date

    // The click ui/js/Sort.js answers. The header owns no sort state, so it only says which column
    // was hit; the key is the protocol's own, which is why Modified sends "mtime".
    signal sortRequested(string key)

    // The pane whose held rows a double click fits; null where the picker reuses this header.
    property var pane: null
    // Only the list view heads columns; Pane wires its viewMode so other views build no handles.
    property string viewMode: "list"
    // A drag in flight, so the hairline follows the pointer and one write lands on release.
    property string dragKey: ""
    property real dragStartX: 0
    property real dragLastX: 0
    property real dragStartWidth: 0
    property real dragPreview: 0
    // True once the pointer travelled, so a click or a fit ending the drag writes nothing.
    property bool dragMoved: false

    // A right click over the titles opens the pane's one ContextMenu with the column toggles and
    // the hidden toggle; a left click still sorts, and sortable still gates everything on a search.
    signal menuRequested(var scenePosition)

    // The same hairline lift the status bar uses, so the two rules read alike.
    readonly property real ruleOpacity: 0.12

    // A column header is chrome, not a data row; see Theme.qml's chromeHeight comment.
    // The search's query line takes the header's slot whole, per the design canvas's Search board.
    property string searchMode: ""
    property string searchQuery: ""
    property string searchScope: ""
    property string searchNote: ""
    property string searchWayOut: ""

    // A search takes the header's slot whole, but the strip's ground is a plain Rectangle and
    // accepts no input, so the titles under it stay hittable unless the handlers go down with them.
    readonly property bool sortable: root.searchMode.length === 0
    // On glass the strip lets the desktop through, so what it covers when opaque goes down with it.
    readonly property bool underStrip: Glass.on && root.searchMode.length > 0
    // Sidebar040: Recent replaces Mode with fixed Location and heads its date as Used.
    property bool recent: false

    // The columns this width affords, less the hidden ones; rows draw lane-narrow like this header.
    property var hiddenCols: ViewState.hiddenCols
    readonly property real contentWidth: Math.max(0, root.width - Theme.spacing.rowPaddingX)
    readonly property var cols: root.recent ? Theme.columns(root.contentWidth, root.hiddenCols, root.dateWidth, true, root.dualMode) : root.dualMode ? Theme.dualColumns(root.contentWidth, root.hiddenCols) : Theme.columns(root.contentWidth, root.hiddenCols, root.dateWidth)

    implicitHeight: Theme.chromeHeight

    SearchStrip {
        anchors.fill: parent
        visible: root.searchMode.length > 0
        z: 1
        query: root.searchQuery
        scope: root.searchScope
        note: root.searchNote
        wayOut: root.searchWayOut
        // The canvas draws the caret on both its search boards, so it stays up as long as the strip is.
        typing: true
    }

    // The OEM lifts a section header off the body with a fill, on top of the hairline rule below.
    Rectangle {
        anchors.fill: parent
        visible: !root.underStrip
        color: Style.normalFill
    }

    PanelSectionHeader {
        id: headerName
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX + root.leadingSlot + (root.dualMode ? Theme.markSize + Theme.spacing.gap : 0)
        anchors.right: headerMode.left
        anchors.rightMargin: (root.recent ? root.cols.location : root.cols.mode) ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: !root.underStrip
        text: root.title("Name", "name")
        elide: Text.ElideRight

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("name") }
    }

    PanelSectionHeader {
        id: headerMode
        anchors.right: headerSize.left
        anchors.rightMargin: root.cols.size && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: (root.recent ? root.cols.location : root.cols.mode) && !root.underStrip
        width: root.recent ? (root.cols.location ? Theme.column.location : 0) : root.cols.mode ? (root.dragKey === "mode" ? root.dragPreview : Theme.column.mode) : 0
        text: root.recent ? "Location" : root.title("Mode", "mode")
    }

    PanelSectionHeader {
        id: headerSize
        anchors.right: headerDate.left
        anchors.rightMargin: root.cols.date && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.size && !root.underStrip
        width: root.cols.size ? (root.dragKey === "size" ? root.dragPreview : root.sizeWidth) : 0
        text: root.title("Size", "size")
        horizontalAlignment: Text.AlignRight

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("size") }
    }

    PanelSectionHeader {
        id: headerDate
        anchors.right: headerKind.left
        anchors.rightMargin: root.cols.kind && !root.recent ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.date && !root.underStrip
        width: root.cols.date ? (root.dragKey === "date" ? root.dragPreview : root.dateWidth) : 0
        text: root.title(root.recent ? "Used" : "Modified", "mtime")
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("mtime") }
    }

    PanelSectionHeader {
        id: headerKind
        anchors.right: parent.right
        // The header carries the lane the rows keep: its own padding plus the lane, so titles stay over their cells.
        anchors.rightMargin: 2 * Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.kind && !root.recent && !root.underStrip
        width: root.cols.kind && !root.recent ? (root.dragKey === "kind" ? root.dragPreview : Theme.column.kind) : 0
        text: root.title("Kind", "kind")
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("kind") }
    }

    // The header is chrome, but it is the chrome the columns belong to, so its right click is where
    // the columns are turned on and off. Left click sorts; sortable gates that, not this.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: function (eventPoint) { root.menuRequested(eventPoint.scenePosition) }
    }

    // ListColumns040: build edges only where usable; cells outside the Loader require x placement.
    Loader {
        id: handlesLoader
        anchors.fill: parent
        active: root.viewMode === "list" && root.visible && !root.dualMode && root.sortable && root.pane !== null
        sourceComponent: Item {
            anchors.fill: parent
            Flea.ResizeHandle {
                id: modeHandle
                visible: root.cols.mode
                x: headerMode.x - Math.floor(width / 2)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                z: 3
                columnKey: "mode"
                hot: root.dragKey === "mode"
                onPressed: function (mouse) { root.beginDrag("mode", modeHandle, mouse) }
                onMoved: function (mouse) { root.moveDrag("mode", modeHandle, mouse) }
                onReleased: root.endDrag()
                onDoubleClicked: root.autofitColumn("mode")
            }
            Flea.ResizeHandle {
                id: sizeHandle
                visible: root.cols.size
                x: headerSize.x - Math.floor(width / 2)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                z: 3
                columnKey: "size"
                hot: root.dragKey === "size"
                onPressed: function (mouse) { root.beginDrag("size", sizeHandle, mouse) }
                onMoved: function (mouse) { root.moveDrag("size", sizeHandle, mouse) }
                onReleased: root.endDrag()
                onDoubleClicked: root.autofitColumn("size")
            }
            Flea.ResizeHandle {
                id: dateHandle
                visible: root.cols.date
                x: headerDate.x - Math.floor(width / 2)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                z: 3
                columnKey: "date"
                hot: root.dragKey === "date"
                onPressed: function (mouse) { root.beginDrag("date", dateHandle, mouse) }
                onMoved: function (mouse) { root.moveDrag("date", dateHandle, mouse) }
                onReleased: root.endDrag()
                onDoubleClicked: root.autofitColumn("date")
            }
            Flea.ResizeHandle {
                id: kindHandle
                visible: root.cols.kind
                x: headerKind.x - Math.floor(width / 2)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                z: 3
                columnKey: "kind"
                hot: root.dragKey === "kind"
                onPressed: function (mouse) { root.beginDrag("kind", kindHandle, mouse) }
                onMoved: function (mouse) { root.moveDrag("kind", kindHandle, mouse) }
                onReleased: root.endDrag()
                onDoubleClicked: root.autofitColumn("kind")
            }
        }
    }

    // Built only while a fit measures, from the fit-only file, so rest carries no metrics object.
    Loader {
        id: fitLoader
        active: false
        source: ""
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        visible: !root.underStrip
        color: Theme.color.foreground
        opacity: root.ruleOpacity
    }

    function title(label, key) {
        return root.sortBy === key ? Sort.title(label, root.sortDesc) : label
    }

    // The stored width a drag or a fit wrote, or the measured one when nothing did.
    function currentWidthOf(key) { return Theme.column[key] }

    // One remembered edge: clamped to the rails and written once, so a drag is layout only.
    function writeWidth(key, px) {
        var next = Columns.clampListWidth(px)
        var cur = (ViewState.state.columnWidths || ({}))[key]
        if (cur === next)
            return
        var obj = {}
        var leaf = {}
        var stored = ViewState.state.columnWidths || {}
        for (var k in stored) obj[k] = stored[k]
        obj[key] = next
        leaf[key] = next
        ViewState.changeMapEntries("columnWidths", leaf, obj)
    }

    function beginDrag(key, handle, mouse) {
        root.dragStartX = handle.mapToItem(root, mouse.x, mouse.y).x
        root.dragLastX = root.dragStartX
        root.dragStartWidth = root.currentWidthOf(key)
        root.dragPreview = root.dragStartWidth
        root.dragMoved = false
        // Activate the preview only after capture, because its width binding moves the handle.
        root.dragKey = key
    }

    function moveDrag(key, handle, mouse) {
        if (root.dragKey !== key)
            return
        var x = handle.mapToItem(root, mouse.x, mouse.y).x
        // A press that never travels is a click; the grab zone sits on the column's left hairline of a right-anchored chain, so widening moves it left: start minus dx.
        if (Math.abs(x - root.dragStartX) < 1)
            return
        root.dragMoved = true
        root.dragLastX = x
        root.dragPreview = Columns.clampListWidth(root.dragStartWidth - (x - root.dragStartX))
    }

    // One remembered edge, written once on release after a real move, so a click or a fit writes nothing.
    function endDrag() {
        if (root.dragKey.length === 0)
            return
        if (root.dragMoved)
            root.writeWidth(root.dragKey, root.dragPreview)
        root.dragKey = ""
        root.dragMoved = false
    }

    // The fit-only file builds synchronously, so the same call can consume its item.
    function ensureFit() {
        if (fitLoader.item !== null)
            return fitLoader.item
        fitLoader.source = Qt.resolvedUrl("FitMetrics.qml")
        fitLoader.active = true
        return fitLoader.item
    }

    // Releasing clears the item, so rest carries no metrics object after a fit.
    function releaseFit() {
        fitLoader.active = false
        fitLoader.source = ""
    }

    // The strings ui/Row.qml draws over the held rows only; one F4 reuses one item.
    function fittedWidth(key) {
        var item = root.ensureFit()
        if (item === null)
            return -1
        var widths = []
        for (var i = 0; i < root.pane.rows.length; i++) {
            var row = root.pane.rows[i]
            if (!row)
                continue
            widths.push(item.cellWidth(key, row, root.pane.kindNames,
                item.dirSizeFor(root.pane.dirSizeState, root.pane.held, i)))
        }
        if (widths.length === 0)
            return -1
        return Columns.autofitWidth(widths, root.currentWidthOf(key))
    }

    // A double click fits one column and ends the drag, so the release after doubleClicked writes nothing back; F4 fits every drawn one.
    function autofitColumn(key) {
        root.dragKey = ""
        root.dragMoved = false
        if (!root.pane || root.dualMode || !root.sortable || !root.cols[key]) {
            root.releaseFit()
            return
        }
        var next = root.fittedWidth(key)
        root.releaseFit()
        if (next >= 0)
            root.writeWidth(key, next)
    }

    function autofitAll() {
        if (!root.pane || root.dualMode || !root.sortable)
            return
        var obj = {}
        var leaf = {}
        var stored = ViewState.state.columnWidths || {}
        for (var k in stored) obj[k] = stored[k]
        var changed = false
        var keys = ["mode", "size", "date", "kind"]
        if (root.ensureFit() === null) {
            root.releaseFit()
            return
        }
        for (var i = 0; i < keys.length; i++) {
            if (!root.cols[keys[i]])
                continue
            var next = root.fittedWidth(keys[i])
            if (next >= 0 && obj[keys[i]] !== Columns.clampListWidth(next)) {
                obj[keys[i]] = Columns.clampListWidth(next)
                leaf[keys[i]] = Columns.clampListWidth(next)
                changed = true
            }
        }
        root.releaseFit()
        if (changed)
            ViewState.changeMapEntries("columnWidths", leaf, obj)
    }

    // What the header case reads, built from the same values the header renders.
    function titles() {
        if (root.recent)
            return "Name" + (root.cols.location ? "|Location" : "") + (root.cols.size ? "|Size" : "") + (root.cols.date ? "|Used" : "")
        return "Name|Mode|Size|Modified|Kind"
    }

    // What the header is drawing right now, for the seam that reads it beside a row's.
    function columnSet() { return Columns.names(root.cols) }

    // The one lookup the geometry reader needs, the same by-key idiom Pane.itemFor uses for rows.
    function cell(key) {
        switch (key) {
        case "name": return headerName
        case "location": return root.recent ? headerMode : null
        case "mode": return root.recent ? null : headerMode
        case "size": return headerSize
        case "date": return headerDate
        case "kind": return headerKind
        }
        return null
    }
}
