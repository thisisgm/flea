import QtQuick
import "js/Filter.js" as Filter

// The filter's query line: the funnel mark, the query with its caret, and the caveat when the pane
// is not holding the whole listing. Presentation only, the way ui/SearchStrip.qml is; ui/Pane.qml
// owns every value it draws and ui/js/Filter.js owns every transition.
//
// It sits below the column header rather than covering it, which is what the search's own strip
// does: the sort mark has to stay readable, because filtering a sorted view keeps that order.
Item {
    id: root

    property var pane: null

    readonly property bool up: !root.pane.trash.opened && (root.pane.filterTyping || root.pane.filterQuery.length > 0)
    readonly property real ruleOpacity: 0.12
    // Twice the hairline, the same weight the cursor row's mark uses, so one caret rule serves both.
    readonly property int caretWidth: Theme.spacing.hairline * 2

    // A chrome strip is not a data row; see Theme.qml's chromeHeight comment. Collapsing rather than
    // only hiding is what keeps the rows starting at the top of the pane when no filter is up.
    implicitHeight: Theme.chromeHeight
    visible: root.up
    height: root.up ? implicitHeight : 0

    // SearchFilter rule 4: the funnel sits on the pane's own ground, where the magnifier sits on the
    // raised chrome, because a filter is about the listing under it and a search is not about here.
    Rectangle {
        anchors.fill: parent
        color: Glass.backgroundPlane
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: root.ruleOpacity
    }

    Glyph {
        id: mark
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        // The chrome mark token, the size the search strip's own lens is drawn at.
        width: Theme.chromeMarkSize
        height: Theme.chromeMarkSize
        name: "filter"
        color: Theme.color.accent
    }

    // corner: a query is arbitrary typed text, so PlainText, the same rule every filename cell carries.
    Text {
        id: queryText
        anchors.left: mark.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.pane.filterQuery
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
    }

    // The caret stands only while the query line has the keyboard: once enter hands it back to the
    // list, the filter is standing rather than being typed, and the strip has to read that way.
    Rectangle {
        id: caret
        visible: root.pane.filterTyping
        anchors.left: queryText.right
        anchors.leftMargin: Theme.spacing.hairline
        anchors.verticalCenter: parent.verticalCenter
        width: root.caretWidth
        height: Theme.font.bodySmall
        color: Theme.color.accent
    }

    // SearchFilter rule 1: the strip owns what it created, so the count and its scope read here and
    // no sentence is left standing in the listing, where it would be a row that is not a file.
    Text {
        id: summaryText
        anchors.left: caret.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: wayOut.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        elide: Text.ElideRight
        text: Filter.summary(root.pane.shown, root.pane.rows.length, root.pane.total)
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Text {
        id: wayOut
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: "esc clears"
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }
}
