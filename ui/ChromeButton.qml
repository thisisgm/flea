import QtQuick
import qs.Commons
import "." as Flea
import "js/Motion.js" as Motion

// One glyph button in the window chrome: muted at rest, accent when it names the current view, and
// dimmed when there is nowhere for it to go.
Item {
    id: root

    property string glyph: "file"
    property bool active: false

    signal activated()

    // A control with nowhere to go still occupies its slot, so the bar never reflows as history changes.
    readonly property real disabledOpacity: 0.35

    readonly property string accessName: {
        if (root.glyph === "arrow-left")
            return "Back"
        if (root.glyph === "arrow-up")
            return "Parent folder"
        if (root.glyph === "search")
            return "Search"
        if (root.glyph === "list")
            return "List view"
        if (root.glyph === "columns")
            return "Columns view"
        if (root.glyph === "grid")
            return "Grid view"
        return root.glyph
    }

    // The mark stays the chrome token; the hit box is at least 24 px wide and the strip's height.
    implicitWidth: Math.max(Theme.hitMin, Theme.chromeMarkSize)
    implicitHeight: Theme.chromeHeight
    scale: tap.pressed && root.enabled && !Motion.reduced ? 0.96 : 1

    Accessible.role: Accessible.Button
    Accessible.name: root.accessName
    Accessible.onPressAction: if (root.enabled) root.activated()

    Behavior on scale {
        enabled: !Motion.reduced
        NumberAnimation { duration: 150; easing.type: Easing.OutQuad }
    }

    Flea.Glyph {
        anchors.centerIn: parent
        width: Theme.chromeMarkSize
        height: Theme.chromeMarkSize
        name: root.glyph
        color: root.active ? Theme.color.accent : Theme.color.muted
        opacity: root.enabled ? 1 : root.disabledOpacity
    }

    HoverHandler {
        cursorShape: Qt.PointingHandCursor
    }

    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        onTapped: root.activated()
    }
}
