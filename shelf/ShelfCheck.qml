import QtQuick
import qs.Commons
import qs.Commons as Commons

// The shared control, Containers rule 14 and the Keys board: a 14 unit interior inside two 2px
// borders, filled in the foreground with the tick cut out of it, so no palette has to tell accent
// from foreground. The shelf carries its own copy because a plugin cannot import Flea's components.
Item {
  id: root

  property bool on: false
  property color foreground: Commons.Color.popups.text
  property color muted: Qt.darker(foreground, 1.55)
  property color ground: Commons.Color.popups.background

  readonly property int borderWidth: 2
  // 18 outer at bodySmall 13, derived so it lands on the same number at any text size.
  implicitHeight: Math.round(14 * Style.font.bodySmall / 13) + 2 * root.borderWidth
  implicitWidth: root.implicitHeight

  Rectangle {
    anchors.fill: parent
    color: root.on ? root.foreground : "transparent"
    border.width: root.borderWidth
    border.color: root.on ? root.foreground : root.muted

    ShelfGlyph {
      anchors.centerIn: parent
      width: Math.round(Style.font.bodySmall * 10 / 13)
      height: width
      visible: root.on
      stroke: 3
      // The tick, on the same 24 unit grid every mark here is drawn on.
      path: "M4 13l6 6l10 -13"
      color: root.ground
    }
  }
}
