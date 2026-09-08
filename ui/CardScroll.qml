import QtQuick
import "." as Flea

// A card body that can be taller than the window: the content keeps its own height, this viewport
// clamps to what the card gave it and scrolls the rest, by wheel and by a focus move below the fold.
Flickable {
    id: root

    default property alias content: holder.data
    // What the content wants, for the card to clamp against the window.
    readonly property real wanted: holder.childrenRect.height

    clip: true
    contentWidth: width
    contentHeight: root.wanted
    boundsBehavior: Flickable.StopAtBounds

    // Tab into a field below the fold scrolls it into view, so a form is never typed into blind.
    function reveal(item) {
        if (!item || root.contentHeight <= root.height || !root.holds(item))
            return
        var top = item.mapToItem(holder, 0, 0).y
        var bottom = top + item.height
        if (top < root.contentY)
            root.contentY = Math.max(0, top)
        else if (bottom > root.contentY + root.height)
            root.contentY = Math.min(root.contentHeight - root.height, bottom - root.height)
    }

    function holds(item) {
        for (var it = item; it; it = it.parent) {
            if (it === holder)
                return true
        }
        return false
    }

    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() { root.reveal(root.Window.window.activeFocusItem) }
    }

    Flea.FastScrollHandler {
        parent: root
        flickable: root
    }

    Item {
        id: holder
        width: root.width
    }
}
