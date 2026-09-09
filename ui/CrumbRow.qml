import QtQuick
import qs.Commons
import "js/Nav.js" as Nav

// Issue 45's segments as one control: the path drawn as pieces a click can land on, in the window's
// chrome and in the picker's nav strip alike. The tail identifies the directory, so a path too long
// for the slot loses its head: the row slides left inside this clipped box, which is the left
// elision a single Text drew, made of pieces. The leaf is where the listing already stands, so it
// stays a label and only the segments above it answer a tap. What a chosen segment does is the
// owner's: ui/ChromeBar.qml navigates the pane, ui/PickerChrome.qml walks the picker.
Item {
    id: root

    property string path: ""
    property string home: ""

    signal crumbChosen(string path)
    // ui/ChromeBar.qml opens its path bar on this; the picker's location field is its own control
    // and leaves it unbound. It is declared here because the tap pair below has to be exclusive,
    // so a double click on a segment types instead of navigating twice.
    signal doubleTapped()

    // The pieces, and the Repeater's items, for the seams tests/ui.sh and tests/picker.sh read.
    readonly property var model: Nav.crumbs(root.path, root.home)
    readonly property alias items: crumbs
    readonly property bool overflowing: row.width > root.width

    implicitWidth: row.width
    clip: true

    // A driven click needs a segment that is on screen: the slot hides its head when the path
    // overflows, so a crumb that slid clean off the left has no centre to press.
    function shownItem(index) {
        var item = crumbs.itemAt(index)
        if (!item || !root.visible)
            return null
        var box = item.mapToItem(root, 0, 0)
        return box.x >= 0 && box.x + item.width <= root.width ? item : null
    }

    Row {
        id: row
        anchors.verticalCenter: parent.verticalCenter
        x: Math.min(0, root.width - row.width)

        Repeater {
            id: crumbs
            model: root.model

            // corner: a path is arbitrary text, so PlainText, the same rule every filename on this surface follows.
            delegate: Text {
                id: crumb
                required property var modelData
                text: crumb.modelData.text
                color: crumb.modelData.last ? Theme.color.foreground : Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
                // The box is the strip's height with the glyphs centred in it, because the handlers
                // below are the strip's whole gesture and a text-tall box left 11 of the chrome's
                // 27 px dead, measured at the window.
                height: root.height
                verticalAlignment: Text.AlignVCenter

                HoverHandler {
                    cursorShape: crumb.modelData.last ? Qt.IBeamCursor : Qt.PointingHandCursor
                }

                // Both flags together, measured on Qt 6.11.2: one of them alone suppresses the
                // other signal instead of waiting, and only the pair makes the tap count decide.
                // The gesture is on the crumb and not on the strip because a TapHandler on a
                // parent item takes the second tap away from the child under the pointer.
                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    exclusiveSignals: TapHandler.SingleTap | TapHandler.DoubleTap
                    onSingleTapped: if (!crumb.modelData.last) root.crumbChosen(crumb.modelData.path)
                    onDoubleTapped: root.doubleTapped()
                }
            }
        }
    }
}
