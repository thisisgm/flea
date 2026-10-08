import QtQuick

// The query line the search puts in the header's slot: the mark, the query with its caret, the
// scope, the count beside it, and the way out at the right edge. Presentation only; ui/Pane.qml
// owns every value it draws.
Item {
    id: root

    property string query: ""
    property string scope: ""
    // A live count while the walk runs, the terminal word once it stops, against the scope rather than at the right edge, because the number and the folder it counts are one fact.
    property string note: ""
    // The right edge, which is what esc does from here.
    property string wayOut: ""
    property bool typing: false

    readonly property real ruleOpacity: 0.12
    // Twice the hairline, the same weight the cursor row's mark uses, so one caret rule serves both.
    readonly property int caretWidth: Theme.spacing.hairline * 2

    implicitHeight: Theme.chromeHeight

    Rectangle {
        anchors.fill: parent
        color: Glass.surfacePlane
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
        // The chrome mark token, the size of the search button this strip replaces.
        width: Theme.chromeMarkSize
        height: Theme.chromeMarkSize
        name: "search"
        color: Theme.color.accent
    }

    Text {
        id: queryText
        anchors.left: mark.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.query
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
    }

    // The caret sits after the query while the strip has the keyboard, matching the canvas.
    Rectangle {
        id: caret
        visible: root.typing
        anchors.left: queryText.right
        anchors.leftMargin: Theme.spacing.hairline
        anchors.verticalCenter: parent.verticalCenter
        width: root.caretWidth
        height: Theme.font.bodySmall
        color: Theme.color.accent
    }

    Text {
        id: scopeText
        anchors.left: caret.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        // The scope gives way before the count does: the number is the one that keeps changing.
        width: Math.min(implicitWidth, Math.max(0, wayOutText.x - x - noteText.width - 2 * Theme.spacing.gap))
        text: root.scope.length > 0 ? "in " + root.scope : ""
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideLeft
        textFormat: Text.PlainText
    }

    Text {
        id: noteText
        anchors.left: scopeText.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.note
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Text {
        id: wayOutText
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: root.wayOut
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }
}
