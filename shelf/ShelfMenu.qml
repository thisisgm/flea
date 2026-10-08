import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model

// Summon rule 4: one shelf, plus the last five piles. The pointer route to a cleared pile is here, in
// the card's own menu, and the keyboard route is the number beside each row; the bar mark's right
// click takes the newest one straight back.
Item {
  id: root

  property var piles: []
  property color foreground: Commons.Color.popups.text
  property color muted: Qt.darker(foreground, 1.55)
  property string fontFamily: Style.font.family
  property real pad: Style.spacing.rowPaddingX
  property real stripHeight: Style.space(23)
  property real rowHeight: Style.space(32)
  property int hoveredIndex: -1

  signal chosen(int index)
  signal dismissed()

  // Summon rule 4's keyboard route: the number beside a row takes that pile back.
  function key(event) {
    var picked = event.key - Qt.Key_1
    if (picked < 0 || picked >= root.piles.length) {
      return
    }
    root.chosen(picked)
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
        text: "Shelf menu"
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

    PanelSectionHeader {
      width: parent.width
      text: "RECENT PILES"
      foreground: root.foreground
      fontFamily: root.fontFamily
    }

    Repeater {
      model: root.piles

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
          id: number
          anchors.verticalCenter: parent.verticalCenter
          x: root.pad
          text: row.index + 1
          color: root.muted
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          textFormat: Text.PlainText
        }

        Text {
          anchors.verticalCenter: parent.verticalCenter
          x: number.x + number.width + Style.space(14)
          width: parent.width - x - root.pad
          text: Model.pileText(row.modelData)
          color: root.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          elide: Text.ElideRight
          textFormat: Text.PlainText
        }

        HoverHandler {
          onHoveredChanged: root.hoveredIndex = hovered ? row.index : (root.hoveredIndex === row.index ? -1 : root.hoveredIndex)
        }

        TapHandler { onSingleTapped: root.chosen(row.index) }
      }
    }

    // Rule 6 of the empty card, applied here: a menu with nothing in it says so and does not apologise.
    Item {
      width: parent.width
      height: visible ? root.rowHeight : 0
      visible: root.piles.length === 0

      Text {
        anchors.centerIn: parent
        text: "No pile has been cleared yet"
        color: root.muted
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
      }
    }
  }
}
