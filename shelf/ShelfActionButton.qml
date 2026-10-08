import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Ui/PanelActionButton's shape with Flea's own mark as its ink: the OEM button draws a nerd glyph
// from a string, and this project's icon language is the Omarchy cut.
BorderSurface {
  id: root

  property string path: ""
  property string tooltipText: ""
  property color foreground: Commons.Color.popups.text
  property bool hasCursor: false
  property real iconSize: Style.font.icon
  property real size: Math.max(Style.space(22), iconSize + Style.spacing.sm * 2)

  signal clicked()

  readonly property bool hot: (mouse.containsMouse || root.hasCursor) && root.enabled

  implicitWidth: size
  implicitHeight: size
  radius: Style.cornerRadius
  color: root.hot ? Style.hoverFillFor(root.foreground, root.foreground) : "transparent"
  borderSpec: Border.none()

  Behavior on color { ColorAnimation { duration: 60 } }

  ShelfGlyph {
    anchors.centerIn: parent
    visible: root.path !== ""
    width: root.iconSize
    height: root.iconSize
    path: root.path
    color: root.enabled ? root.foreground : Qt.darker(root.foreground, 2.0)
  }

  // A button with no path draws a brand's own mark instead: Send is Taildrop.
  ShelfTailscaleMark {
    anchors.centerIn: parent
    visible: root.path === ""
    iconSize: root.iconSize
    color: root.enabled ? root.foreground : Qt.darker(root.foreground, 2.0)
  }

  MouseArea {
    id: mouse
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    onClicked: root.clicked()
  }

  PanelToolTip {
    visible: root.tooltipText !== "" && mouse.containsMouse
    text: root.tooltipText
  }
}
