import QtQuick
import qs.Commons
import qs.Ui

// The column header renders sort state and owns none of it, so Pane stays the one state owner.
Item {
    id: root

    property string sortBy: "name"
    property bool sortDesc: false

    // The click ui/js/Sort.js answers. The header owns no sort state, so it only says which column
    // was hit; the key is the protocol's own, which is why Date Modified sends "mtime".
    signal sortRequested(string key)

    // The same hairline lift the status bar uses, so the two rules read alike.
    readonly property real ruleOpacity: 0.12

    // A column header is chrome, not a data row; see Theme.qml's chromeHeight comment.
    // The search's query line takes the header's slot whole, per the design canvas's Search board.
    property string searchMode: ""
    property string searchQuery: ""
    property string searchScope: ""
    property string searchNote: ""

    // A search takes the header's slot whole, but the strip's ground is a plain Rectangle and
    // accepts no input, so the titles under it stay hittable unless the handlers go down with them.
    readonly property bool sortable: root.searchMode.length === 0

    // The columns this width affords. ui/Row.qml resolves its own from a width anchoring keeps
    // equal to this one, so the header can never head a column no row below it is drawing.
    readonly property var cols: Theme.columns(root.width)

    implicitHeight: Theme.chromeHeight

    SearchStrip {
        anchors.fill: parent
        visible: root.searchMode.length > 0
        z: 1
        query: root.searchQuery
        scope: root.searchScope
        note: root.searchNote
        // The canvas draws the caret on both its search boards, so it stays up as long as the strip is.
        typing: true
    }

    // The OEM lifts a section header off the body with a fill, on top of the hairline rule below.
    Rectangle {
        anchors.fill: parent
        color: Style.normalFill
    }

    PanelSectionHeader {
        id: headerName
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.right: headerMode.left
        anchors.rightMargin: root.cols.mode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        text: root.title("Name", "name")
        elide: Text.ElideRight

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("name") }
    }

    PanelSectionHeader {
        id: headerMode
        anchors.right: headerSize.left
        anchors.rightMargin: root.cols.size ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.mode
        width: root.cols.mode ? Theme.column.mode : 0
        text: root.title("Mode", "mode")
    }

    PanelSectionHeader {
        id: headerSize
        anchors.right: headerDate.left
        anchors.rightMargin: root.cols.date ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.size
        width: root.cols.size ? Theme.column.size : 0
        text: root.title("Size", "size")

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("size") }
    }

    PanelSectionHeader {
        id: headerDate
        anchors.right: headerKind.left
        anchors.rightMargin: root.cols.kind ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.date
        width: root.cols.date ? Theme.column.date : 0
        text: root.title("Date Modified", "mtime")
        elide: Text.ElideRight

        TapHandler { enabled: root.sortable; onTapped: root.sortRequested("mtime") }
    }

    PanelSectionHeader {
        id: headerKind
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        visible: root.cols.kind
        width: root.cols.kind ? Theme.column.kind : 0
        text: root.title("Kind", "kind")
        elide: Text.ElideRight
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: root.ruleOpacity
    }

    // Two geometric characters the stock monospace has, so the mark scales with the font like the label.
    function title(label, key) {
        if (root.sortBy !== key) {
            return label
        }
        return label + " " + (root.sortDesc ? "▾" : "▴")
    }

    // What the header case reads, built from the same values the header renders.
    function titles() {
        return "Name|Mode|Size|Date Modified|Kind"
    }

    // What the header is drawing right now, for the seam that reads it beside a row's.
    function columnSet() { return Theme.columnNames(root.width) }

    // The one lookup the geometry reader needs, the same by-key idiom Pane.itemFor uses for rows.
    function cell(key) {
        switch (key) {
        case "name": return headerName
        case "mode": return headerMode
        case "size": return headerSize
        case "date": return headerDate
        case "kind": return headerKind
        }
        return null
    }
}
