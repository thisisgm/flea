import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "Model.js" as Model

// One row of the card, Main board rules 3, 4 and 6: a separator and a section header when the row
// opens a group, then the row itself in the shape panels/dropbox gives its FileRow, a CursorSurface
// with the mark slot on the centre line, the name, the size and one trailing control.
Column {
  id: row
  // The card this row belongs to: its palette, its cursor, its chosen set and its signals.
  required property var card
  required property int index
  required property var modelData
  width: parent ? parent.width : 0
  // A group opens with the gap the OEM panels leave around a section header, then its rows.
  spacing: Style.space(10)
  readonly property bool picked: row.card.chosen[row.modelData.path] === true
  readonly property string caption: Model.captionFor(row.card.rows, row.index, row.card.kinds)
  // Rule 4: a thumbnail path is not a thumbnail, because the cache file can be evicted
  // between the answer and the decode; a row whose Image failed is marked by its kind.
  readonly property string thumb: Model.thumbFor(row.card.thumbs, row.modelData)
  readonly property bool thumbDrawn: row.thumb.length > 0 && shot.status === Image.Ready

  // Rule 2: a separator opens a group whenever something is above it on the card.
  PanelSeparator {
    visible: row.caption.length > 0 && (row.index > 0 || row.card.emptyShown)
    width: parent.width
    foreground: row.card.foreground
  }

  // The OEM panels pass their headers uppercase, so the shelf's read the way EXIT NODES does.
  PanelSectionHeader {
    visible: row.caption.length > 0
    width: parent.width
    text: row.caption.toUpperCase()
    foreground: row.card.foreground
    fontFamily: row.card.fontFamily
  }

  // Directive 68: the lift is Flea's own list row rather than the OEM's rounded surface, square and
  // at the row's full width, so the card reads as the app; ui/Row.qml is where both fills come from.
  Item {
    id: lift
    width: parent.width
    // The paint comes from the cursor, so one row is lit whichever hand moved it: the pointer's
    // own hover sets cursorIndex on the way in, which is the OEM's single-highlight contract.
    readonly property bool hasCursor: row.card.cursorIndex === row.index
    implicitHeight: Math.max(name.implicitHeight, trailing.implicitHeight) + Style.spacing.rowPaddingX

    // Passive, so it stays hovered over the row's own buttons, which the row's MouseArea does not.
    HoverHandler { id: liftHover }

    Rectangle {
      anchors.fill: parent
      color: lift.hasCursor ? Util.alpha(row.card.accent, Style.selectedFillAlpha)
           : row.picked ? Style.selectionFillFor(row.card.foreground, row.card.accent)
           : "transparent"
    }

    // Twice the hairline, the cursor mark a list row carries against its own left edge.
    Rectangle {
      visible: lift.hasCursor
      width: Style.spacing.hairline * 2
      height: parent.height
      color: row.card.accent
    }

    MouseArea {
      id: rowMouse
      anchors.fill: parent
      hoverEnabled: true
      preventStealing: true
      cursorShape: Qt.PointingHandCursor
      acceptedButtons: Qt.LeftButton
      // Rule 11: the token is minted on the press because a platform drag cannot start from
      // inside its own loop, and the carry waits for the drag distance so a click stays a click.
      property point pressAt: Qt.point(0, 0)
      // A ctrl or shift press is a marking gesture, so it mints nothing and carries nothing.
      property bool marking: false
      onEntered: {
        row.card.cursorIndex = row.index
        row.card.hoveredIndex = row.index
      }
      onExited: if (row.card.hoveredIndex === row.index) row.card.hoveredIndex = -1
      onPressed: function (mouse) {
        rowMouse.pressAt = Qt.point(mouse.x, mouse.y)
        rowMouse.marking = (mouse.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) !== 0
        if (rowMouse.marking) {
          return
        }
        row.card.pressing = true
        row.card.carriedPaths = Model.carryPaths(row.card.chosen, row.card.rows, row.index)
      }
      onPositionChanged: function (mouse) {
        if (!rowMouse.pressed || rowMouse.marking) {
          return
        }
        var dx = mouse.x - rowMouse.pressAt.x
        var dy = mouse.y - rowMouse.pressAt.y
        if (Math.abs(dx) + Math.abs(dy) >= Qt.styleHints.startDragDistance) {
          row.card.wantCarry()
        }
      }
      onReleased: row.card.dropCarry()
      onCanceled: row.card.dropCarry()
      // Rule 3, the contract Flea's own listing has for ctrl, shift and a plain click.
      onClicked: function (mouse) {
        if ((mouse.modifiers & Qt.ControlModifier) !== 0) {
          row.card.markRow(row.index)
        } else if ((mouse.modifiers & Qt.ShiftModifier) !== 0) {
          row.card.markRange(row.index)
        } else {
          row.card.clearMarks(row.index)
          if (row.modelData.section === Model.CAPTURE) {
            row.card.captureAddRequested(row.index)
          }
        }
      }

      // Rule 9: the row's own full path is its tooltip, so the card never spends a line on it.
      PanelToolTip {
        visible: parent.containsMouse
        text: row.modelData.path
        fontFamily: row.card.fontFamily
      }
    }

    RowLayout {
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.spacing.rowPaddingX
      anchors.rightMargin: Style.spacing.rowPaddingX
      spacing: Style.spacing.rowGap

      // Rules 3 and 4: one slot, because a row cannot say both what it is and whether it is taken.
      Item {
        id: slot
        Layout.alignment: Qt.AlignVCenter
        implicitWidth: row.card.slotSize
        implicitHeight: row.card.slotSize
        // Rule 3: the box is the pointer's way to mark, so it is there whenever the row is under the
        // pointer and whenever anything is marked; otherwise the slot says what kind the row is.
        readonly property bool choosing: row.card.chosenCount > 0 || row.card.hoveredIndex === row.index

        Image {
          id: shot
          anchors.fill: parent
          visible: !slot.choosing && row.thumbDrawn
          source: row.thumb.length > 0 ? "file://" + row.thumb : ""
          // Sized on purpose, the way ui/Row.qml sizes it: the decode is capped to the slot
          // it is drawn in rather than the cache file's own 256.
          sourceSize.width: row.card.slotSize * Screen.devicePixelRatio
          sourceSize.height: row.card.slotSize * Screen.devicePixelRatio
          fillMode: Image.PreserveAspectFit
          // A synchronous decode would land on the bar's own frame.
          asynchronous: true
        }

        ShelfGlyph {
          anchors.fill: parent
          visible: !slot.choosing && !row.thumbDrawn
          path: Model.glyphFor(row.modelData)
          color: row.card.foreground
        }

        ShelfCheck {
          anchors.centerIn: parent
          visible: slot.choosing
          on: row.picked
          foreground: row.card.foreground

          // A click on the box marks the row and nothing else: it never moves the cursor.
          MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton
            onClicked: row.card.markRow(row.index)
          }
        }
      }

      Text {
        id: name
        Layout.fillWidth: true
        text: row.modelData.name
        color: row.card.foreground
        font.family: row.card.fontFamily
        font.pixelSize: Style.font.body
        elide: Text.ElideMiddle
        textFormat: Text.PlainText
      }

      // Rule 3: the family is monospace, so the sizes are tabular without a font feature.
      Text {
        Layout.alignment: Qt.AlignVCenter
        text: Model.sizeText(row.modelData)
        color: row.card.muted
        font.family: row.card.fontFamily
        font.pixelSize: Style.font.caption
        horizontalAlignment: Text.AlignRight
        textFormat: Text.PlainText
      }

      // Copy to Clipboard, only while the pointer is on the row: the chosen rows when any are chosen,
      // else this row, the way a drag from it carries them; the files themselves, never their paths.
      ShelfActionButton {
        id: clipButton
        Layout.alignment: Qt.AlignVCenter
        visible: liftHover.hovered
        path: Model.ACTION_GLYPHS.clipboard
        tooltipText: Model.actionTip("Copy to Clipboard", "shift + c", row.card.keyHints)
        foreground: row.card.foreground
        onClicked: row.card.clipRequested(Model.carryPaths(row.card.chosen, row.card.rows, row.index))
      }

      // Rules 3 and 6: a capture keeps the width and spends it on nothing, so the sizes line up.
      ShelfActionButton {
        id: trailing
        Layout.alignment: Qt.AlignVCenter
        enabled: row.modelData.section !== Model.CAPTURE
        opacity: enabled ? 1 : 0
        path: row.modelData.pinned ? Model.ACTION_GLYPHS.pin : Model.ACTION_GLYPHS.remove
        tooltipText: row.modelData.pinned ? "Unpin" : "Take off the shelf"
        foreground: row.card.foreground
        onClicked: row.modelData.pinned ? row.card.pinRequested(row.index)
                                        : row.card.removeRequested(row.index)
      }
    }
  }
}
