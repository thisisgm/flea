import QtQuick
import "." as Flea
import "js/Crumbs.js" as Crumbs

// A dual pane's own path, drawn with the chrome's crumbs so issue 45's one tap on a parent reaches either pane.
Rectangle {
    id: root

    property string path: ""
    property string home: ""
    property var pathAlias: null
    property bool focused: false
    // ui/ChromeBar.qml's inputLive: false while Quick Look covers the strip, so a press on it stays Quick Look's.
    property bool inputLive: true
    readonly property alias crumbItems: crumbs
    readonly property alias crumbSlot: slot

    signal chosen(string path)
    signal editRequested()

    color: root.focused ? Theme.color.surface : Theme.color.background

    // Where no crumb is, the strip keeps the one gesture it had before issue 45: a double click types the path.
    component TypeArea: Item {
        HoverHandler {
            enabled: root.inputLive
            cursorShape: Qt.IBeamCursor
        }
        TapHandler {
            enabled: root.inputLive
            acceptedButtons: Qt.LeftButton
            onDoubleTapped: root.editRequested()
        }
    }

    // One glyph's advance is every glyph's advance in this face, the same fit ui/ChromeBar.qml makes.
    TextMetrics {
        id: metrics
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: "0"
    }

    Item {
        id: slot
        anchors.fill: parent
        clip: true

        Row {
            id: row
            x: Theme.spacing.rowPaddingX
            height: slot.height

            Repeater {
                id: crumbs
                // Empty while the strip is hidden, because the single view keeps it at height 0 and would rebuild its crumbs on every move.
                model: root.visible
                       ? Crumbs.fitCrumbs(Crumbs.crumbs(root.path, root.home, root.pathAlias),
                                          Math.floor((slot.width - 2 * Theme.spacing.rowPaddingX) / metrics.advanceWidth))
                       : []

                delegate: Flea.Crumb {
                    height: slot.height
                    restColor: Theme.color.foreground
                    inputLive: root.inputLive
                    onChosen: function (path) { root.chosen(path) }
                    onEditRequested: root.editRequested()
                }
            }
        }

        TypeArea {
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            width: Theme.spacing.rowPaddingX
        }

        TypeArea {
            anchors { left: row.right; right: parent.right; top: parent.top; bottom: parent.bottom }
        }
    }

    Rectangle {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Theme.spacing.hairline
        color: Theme.color.muted
    }
}
