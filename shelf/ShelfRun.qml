import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Run.js" as Run

// The transfer surface the shelf already has, off the same 250 ms sample the pane's card reads.
// No Flea window need be open: the transfer runs in the backend the action already called.
Item {
  id: root

  property var run: Run.idle()
  property color foreground: Commons.Color.popups.text
  property color muted: Qt.darker(foreground, 1.55)
  property color accent: Commons.Color.accent
  property string fontFamily: Style.font.family
  property real pad: Style.spacing.rowPaddingX

  signal cancelRequested()

  readonly property real gap: Style.space(9)
  readonly property real barHeight: Style.space(5)

  visible: root.run.running
  implicitHeight: visible ? column.implicitHeight + 2 * root.gap : 0

  Column {
    id: column
    x: root.pad
    y: root.gap
    width: parent.width - 2 * root.pad
    spacing: Style.space(5)

    Text {
      text: Run.runText(root.run)
      color: root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.body
      textFormat: Text.PlainText
    }

    Text {
      text: root.run.name
      color: root.muted
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      elide: Text.ElideMiddle
      width: parent.width
      textFormat: Text.PlainText
    }

    // The bar is the only place the accent is a fill here, and it is the figure itself.
    Rectangle {
      width: parent.width
      height: root.barHeight
      color: Qt.rgba(root.muted.r, root.muted.g, root.muted.b, 0.35)

      Rectangle {
        width: parent.width * Run.runFraction(root.run)
        height: parent.height
        color: root.accent
      }
    }

    // Rule 2: the shell's own button, the way every OEM panel spends one.
    Item {
      width: parent.width
      height: cancel.implicitHeight

      Button {
        id: cancel
        anchors.right: parent.right
        text: "Cancel"
        bordered: true
        foreground: root.foreground
        fontFamily: root.fontFamily
        onClicked: root.cancelRequested()
      }
    }
  }
}
