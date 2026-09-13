import QtQuick
import qs.Commons
import "." as Flea

// Chrome uses muted ink; active and focused controls use the accent.
Item {
    id: root

    property string glyph: "file"
    property bool active: false
    property bool keyboardFocused: false
    property color restingColor: Theme.color.muted
    property real glyphSize: Theme.chromeMarkSize

    signal activated()

    // A control with nowhere to go still occupies its slot, so the bar never reflows as history changes.
    property real disabledOpacity: 1

    property string accessName: {
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
    scale: tap.pressed && root.enabled && !Theme.reducedMotion ? 0.96 : 1

    Accessible.role: Accessible.Button
    Accessible.name: root.accessName
    Accessible.onPressAction: if (root.enabled) root.activated()

    Rectangle {
        anchors.centerIn: parent
        width: Theme.hitMin
        height: Theme.hitMin
        visible: root.keyboardFocused
        color: "transparent"
        border.width: Theme.spacing.hairline
        border.color: Theme.color.accent
    }

    Behavior on scale {
        enabled: !Theme.reducedMotion
        NumberAnimation { duration: 150; easing.type: Easing.OutQuad }
    }

    Flea.Glyph {
        anchors.centerIn: parent
        width: root.glyphSize
        height: root.glyphSize
        name: root.glyph
        color: !root.enabled ? Theme.color.muted : root.active || root.keyboardFocused ? Theme.color.accent : root.restingColor
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
