import QtQuick
import qs.Commons
import qs.Commons as Commons

// One flyout, two verbs and the send: Flea's own places and Taildrop's own peers, numbered so a
// keyboard reaches any of them in one press. Actions: recent destinations first, because the same
// folder is usually the answer twice running.
Item {
  id: root

  property string title: ""
  property var rows: []
  // The destination flyout offers the chooser as its last row; the peer flyout has no such thing.
  property bool browsable: false
  property color foreground: Commons.Color.popups.text
  property color muted: Qt.darker(foreground, 1.55)
  property string fontFamily: Style.font.family
  property real pad: Style.spacing.rowPaddingX
  property real stripHeight: Style.space(23)
  property real rowHeight: Style.space(32)
  property int hoveredIndex: -1

  signal chosen(int index)
  signal browse()
  signal dismissed()

  // 1 to 9 take a row straight away, which is the whole of the chooser's keyboard on the Keys board.
  readonly property int reachable: Math.min(9, root.rows.length)

  function key(event) {
    var picked = event.key - Qt.Key_1
    if (picked >= 0 && picked < root.reachable) {
      root.chosen(picked)
    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      if (!root.browsable) {
        return
      }
      root.browse()
    } else {
      return
    }
    event.accepted = true
  }

  implicitHeight: column.implicitHeight

  Column {
    id: column
    width: parent.width

    Item {
      width: parent.width
      height: root.stripHeight

      Text {
        anchors.verticalCenter: parent.verticalCenter
        x: root.pad
        text: root.title
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
      }

      Text {
        anchors.verticalCenter: parent.verticalCenter
        anchors.right: parent.right
        anchors.rightMargin: root.pad
        text: "esc"
        color: root.muted
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText

        TapHandler { onSingleTapped: root.dismissed() }
      }
    }

    Repeater {
      model: root.rows

      Item {
        id: row
        required property int index
        required property var modelData
        width: column.width
        height: root.rowHeight

        Rectangle {
          anchors.fill: parent
          color: root.hoveredIndex === row.index ? Style.hoverFill : "transparent"
        }

        Text {
          id: label
          anchors.verticalCenter: parent.verticalCenter
          x: root.pad
          width: parent.width - x - number.width - 2 * root.pad
          text: row.modelData
          color: root.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          elide: Text.ElideMiddle
          textFormat: Text.PlainText
        }

        Text {
          id: number
          anchors.verticalCenter: parent.verticalCenter
          anchors.right: parent.right
          anchors.rightMargin: root.pad
          visible: row.index < root.reachable
          text: row.index + 1
          color: root.muted
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          textFormat: Text.PlainText
        }

        HoverHandler {
          onHoveredChanged: root.hoveredIndex = hovered ? row.index : (root.hoveredIndex === row.index ? -1 : root.hoveredIndex)
        }

        TapHandler { onSingleTapped: root.chosen(row.index) }
      }
    }

    // The way out of a list of shortcuts, which is the whole point of a chooser that is not a picker.
    Item {
      width: parent.width
      height: visible ? root.rowHeight : 0
      visible: root.browsable

      Text {
        anchors.verticalCenter: parent.verticalCenter
        x: root.pad
        text: "Choose a folder"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        textFormat: Text.PlainText
      }

      Text {
        anchors.verticalCenter: parent.verticalCenter
        anchors.right: parent.right
        anchors.rightMargin: root.pad
        text: "enter"
        color: root.muted
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
      }

      TapHandler { onSingleTapped: root.browse() }
    }

    // A flyout with nothing in it says so: a box with no Tailscale has no peer to send to.
    Item {
      width: parent.width
      height: visible ? root.rowHeight : 0
      visible: root.rows.length === 0 && !root.browsable

      Text {
        anchors.centerIn: parent
        text: "Nothing to send to on this box"
        color: root.muted
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
      }
    }
  }
}
