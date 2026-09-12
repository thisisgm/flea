import QtQuick

// The pointer's title bar: a decorated window gets this gesture for free and this one cannot,
// because ui/shell.qml asks for no decorations and draws its chrome itself, so nothing in the tree
// ever requested a move and the window moved only on the compositor's own bind.
// A handler and not a MouseArea, so the strip's controls keep their grabs: a press that does not
// travel stays a click on the button, crumb or path under the pointer, and only a press that moves
// becomes a move. target is null because the move belongs to the window, not to any item here.
Item {
    id: root

    // Whether the caller's path editor is up, whose press-and-travel selects the text in the bar.
    required property bool editing

    anchors.fill: parent

    DragHandler {
        id: move

        target: null
        acceptedButtons: Qt.LeftButton
        // Well under Qt's own 10, because a title bar reads a few pixels of travel as intent.
        dragThreshold: 4
        enabled: !root.editing

        onActiveChanged: {
            // Quickshell's attached property, the route ui/CardScroll.qml and ui/Ipc.qml already
            // take to the window, so no window is threaded through the caller's property list.
            if (!move.active)
                return
            var window = root.Window.window
            if (window)
                window.startSystemMove()
        }
    }
}
