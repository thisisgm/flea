import QtQuick
import qs.Commons
import "js/Format.js" as Format

// The list row's metadata cells, in their own file so ui/Row.qml's recorded budget could hold the
// fifth column: the cells, their text functions and their colours moved here whole, and the row
// anchors its name and its editor to the mode cell through the aliases below. Every cell anchors
// to its right-hand neighbour, so a column that is not drawn takes neither its width nor its gap
// and the chain collapses onto the name.
Item {
    id: root

    property var row: null
    // Theme.columns()'s or Columns.dualSet()'s answer for this row's width.
    property var cols: ({})
    property bool dualMode: false
    property bool searching: false
    property bool dropTarget: false
    property bool lifted: false
    property bool foregroundMetadata: false
    property real sizeWidth: 70
    property real dateWidth: 125
    // A directory's recursive size, resolved by index in List.qml the same way thumb already is.
    property var dirSize: null
    // The picker's narrow date column and its compact form.
    property bool compactDate: false
    // row.k indexes this; List.qml hands down the array every row of one response shares.
    property var kindNames: []

    readonly property bool modeShown: !root.searching && root.cols.mode
    // The search column set keeps Size and drops the other four, so only this one ignores searching.
    readonly property bool sizeShown: root.cols.size
    readonly property bool dateShown: !root.searching && root.cols.date
    readonly property bool kindShown: !root.searching && root.cols.kind
    readonly property bool ageShown: !root.searching && root.cols.age
    readonly property real ageWidth: root.dualMode ? Age.dualWidth : Age.width
    // The OEM derives its secondary ink from the foreground rather than reading a separate palette key.
    readonly property color dim: Qt.darker(Theme.color.foreground, 1.4)

    // ui/Row.qml's editor and its name anchor to the mode cell's left edge, and the search row's
    // location to the size cell's, so the two stay reachable from outside this file.
    readonly property Item modeCell: mode
    readonly property Item sizeCell: size

    // A lifted row is a surface the theme never modelled, so its text takes the strongest ink.
    function cellColor() {
        return root.lifted || root.foregroundMetadata ? Theme.color.foreground : root.dim
    }

    Text {
        id: mode
        anchors.right: size.left
        anchors.rightMargin: root.sizeShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.modeShown && !root.dropTarget
        width: root.modeShown ? Theme.column.mode : 0
        text: root.row ? Format.permissions(root.row.p) : ""
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: size
        anchors.right: modified.left
        anchors.rightMargin: root.dateShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.sizeShown && !root.dropTarget
        width: root.sizeShown ? root.sizeWidth : 0
        text: root.row ? root.sizeText() : ""
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: modified
        anchors.right: kind.left
        anchors.rightMargin: root.kindShown ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.dateShown && !root.dropTarget
        width: root.dateShown ? root.dateWidth : 0
        text: root.dateText()
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: kind
        anchors.right: age.left
        anchors.rightMargin: root.ageShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.kindShown && !root.dropTarget
        width: root.kindShown ? Theme.column.kind : 0
        text: root.row ? root.kindText() : ""
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // The tint is the feature, and the tick repaints it: Age.nowMs moves every thirty seconds and
    // every binding that reads it re-evaluates, so an idle window ages without a listing. A lifted
    // row gives the tint up for contrast the way it gives up every semantic colour, and a row with
    // no real mtime renders the dash and tints nothing.
    Text {
        id: age
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        visible: root.ageShown && !root.dropTarget
        width: root.ageShown ? root.ageWidth : 0
        text: root.row ? (root.row.m === null ? "--" : Format.age(root.row.m, Age.nowMs)) : ""
        color: root.lifted || root.foregroundMetadata ? root.cellColor()
             : Age.tint(root.row ? root.row.m : null, root.lifted || root.foregroundMetadata)
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // A directory's own row.s is its dirent size, not the walk's, so this reads root.dirSize instead, see docs/protocol.md "dirsized".
    function sizeText() {
        // A link's own st_size is the length of its target path, which is not a size anyone means.
        if (Format.isSymlink(root.row.p))
            return "link"
        if (!root.row.d) {
            return Format.size(root.row.s)
        }
        if (!root.dirSize) {
            return "·"
        }
        return (root.dirSize.partial ? ">" : "") + Format.size(root.dirSize.bytes)
    }

    // The window's own four forms, or the picker's compact three; both are cell text and nothing more.
    function dateText() {
        if (!root.row) {
            return ""
        }
        // null marks a row with no real mtime yet (ui/ShareBrowser.qml's share rows).
        if (root.row.m === null) {
            return "--"
        }
        return root.compactDate ? Format.compactDate(root.row.m, Date.now()) : Format.date(root.row.m, Date.now())
    }

    // row.k indexes root.kindNames; an index past its bounds (a row held over from an older listing) reads as empty, never a crash.
    function kindText() {
        if (!root.row || root.row.k === undefined) {
            return ""
        }
        var text = root.kindNames[root.row.k]
        return text !== undefined ? text : ""
    }

    // The by-key idiom Header.cell uses, so the overflow reader can reach a specific cell.
    function cell(key) {
        switch (key) {
        case "mode": return mode
        case "size": return size
        case "date": return modified
        case "age": return age
        case "kind": return kind
        }
        return null
    }
}
