import QtQuick
import qs.Commons
import "js/Emblem.js" as Emblem
import "." as Flea

// The sync badge over a row's icon, made by js/Emblem.js only on a row a sync tool tagged.
// Its fill is a Theme role and its mark the background ink, so it follows the theme like every colour.
Rectangle {
    id: root

    property Item targetIcon: null
    property bool isGrid: false
    property string status: ""

    z: 3
    readonly property real badgeSize: root.isGrid
        ? Math.max(14, Math.round((root.targetIcon ? root.targetIcon.width : Theme.iconSize) * 0.22))
        : Math.max(9, Math.round((root.targetIcon ? root.targetIcon.width : Theme.iconSize) * 0.45))

    width: badgeSize
    height: badgeSize
    radius: badgeSize / 2

    anchors.right: root.targetIcon ? root.targetIcon.right : undefined
    anchors.bottom: root.targetIcon ? root.targetIcon.bottom : undefined
    anchors.rightMargin: root.isGrid ? 2 : -Math.round(badgeSize * 0.15)
    anchors.bottomMargin: root.isGrid ? 2 : -Math.round(badgeSize * 0.15)

    color: Theme.color[Emblem.role(root.status)]
    border.width: root.isGrid ? 1.5 : 1
    border.color: Theme.color.background

    Flea.Glyph {
        anchors.centerIn: parent
        width: Math.max(root.isGrid ? 10 : 6, root.badgeSize - (root.isGrid ? 4 : 2))
        height: width
        maxSize: width
        strokeWidth: 2.5
        name: Emblem.glyph(root.status)
        color: Theme.color.background
    }
}
