import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model
import "Run.js" as Run

// The card, Main board rules 1 to 12 as consolidated: an Omarchy panel and not a Flea window in
// miniature, so there is no hero and no header and the first row is the first thing on it.
Item {
  id: root

  // Main rule 9: one list, three sections. The pile is still here for the hero's own count.
  property var pile: Model.empty()
  property var rows: []
  // Rule 9: the caption names the kinds Settings has checked.
  property var kinds: ({ screenshots: true, recordings: true })
  // Rule 8: a tooltip carries the key only when Flea's own key hints setting is on.
  property bool keyHints: false
  property color foreground: Commons.Color.popups.text
  property color muted: Qt.darker(foreground, 1.4)
  property color accent: Commons.Color.accent
  property color urgent: Commons.Color.urgent
  property string fontFamily: Style.font.family
  // Rule 9: one transient line, one voice: the last result or the one error, never both.
  property string result: ""
  property string error: ""
  property int cursorIndex: -1
  // Rule 5: the voice says the path of the row the pointer is on, which is not the same question as
  // which row the cursor is on: the keyboard walks the list without narrating it.
  property int hoveredIndex: -1
  // Rule 4: the cache file each drawn path answered with, empty for a path that has none.
  property var thumbs: ({})
  // EdgeRail: how many a drag over the rail is offering, which the hero says and the line shows.
  property int incoming: 0
  // Actions: what is running, which the body draws and the voice's own half names.
  property var run: Run.idle()
  signal cancelRequested()
  // Keys: the subset gesture. Chosen by path, and the cursor is its own rung on top of it.
  property var chosen: ({})
  readonly property int chosenCount: Model.chosenCount(root.chosen, root.rows)
  // Rule 10: the word an empty pile says, which is also what puts a separator above the first group.
  readonly property bool emptyShown: Model.loose(root.pile.items).length === 0 && !root.run.running
  // Rule 8: the action row, which tab reaches only while it is on the card.
  readonly property bool stripShown: !root.run.running && root.rows.length > 0
                                     && (Model.loose(root.pile.items).length > 0 || root.cursorIndex >= 0)
  onStripShownChanged: if (!root.stripShown) root.stripIndex = -1

  // Keys: which action the strip's own focus is on, and -1 while the rows have it.
  property int stripIndex: -1
  // The list loses a row on a box with no Tailscale, so the focus cannot outlive the action it was on.
  onActionsChanged: if (root.stripIndex >= root.actions.length) root.stripIndex = root.actions.length - 1

  // Rule 6: p moves a row from one group to another, so the cursor follows the row it was on rather
  // than the place that row used to be in, and a second p is an unpin instead of another pin.
  property string followPath: ""
  onRowsChanged: {
    if (root.cursorIndex >= root.rows.length) {
      root.cursorIndex = root.rows.length - 1
    }
    if (root.hoveredIndex >= root.rows.length) {
      root.hoveredIndex = -1
    }
    if (root.followPath === "") {
      return
    }
    for (var i = 0; i < root.rows.length; i++) {
      if (root.rows[i].path === root.followPath) {
        root.cursorIndex = i
        root.followPath = ""
        return
      }
    }
  }

  signal removeRequested(int index)
  signal pinRequested(int index)
  signal openRequested(string path)
  // The card is carrying the pile out, which is the one time it must not hold the keyboard.
  signal carried(bool carrying)
  signal captureAddRequested(int index)
  // Summon: the pointer's way to the last five piles is the card's own menu.
  signal menuRequested()
  signal actionRequested(string id)
  // Rule 11: a row is the handle. What the drag is, a move or a copy, is decided from the rows it
  // carries and not from a modifier, because a platform drag runs a loop this window gets no keys in.
  signal liftRequested()
  // What that grab is carrying: the chosen rows, or the row it started from.
  property var carriedPaths: []

  // The payload the drag carries once the token is back: the token and the intent, then the paths
  // for every other application, which is offered a copy and never a move (DragOut rule 4).
  property var dragMime: ({})
  Drag.dragType: Drag.Automatic
  Drag.supportedActions: Qt.CopyAction
  Drag.proposedAction: Qt.CopyAction
  Drag.mimeData: root.dragMime

  // The token is asked for when the pointer passes the drag distance and arrives a few milliseconds
  // later with the button still down, which is where a platform drag can be started from.
  property bool wanted: false
  // True from the press to the release. A token that arrives after the release arms nothing, because
  // the gesture it belonged to is over.
  property bool pressing: false

  function lift(token, copying) {
    var uris = []
    var carried = root.carriedPaths
    for (var i = 0; i < carried.length; i++) {
      uris.push("file://" + Model.uriPath(carried[i]))
    }
    var mime = { "application/x-flea-shelf": token + "\n" + (copying ? "copy" : "move") }
    mime["text/uri-list"] = uris.join("\r\n") + "\r\n"
    if (!root.pressing || !root.wanted) {
      return
    }
    root.dragMime = mime
    root.beginCarry()
  }

  // The pointer left the press by more than the platform's own drag distance, so this is a carry and
  // the token is asked for now: a press that stays still is a click and mints nothing at all.
  function wantCarry() {
    if (root.wanted) {
      return
    }
    root.wanted = true
    root.liftRequested()
  }

  function beginCarry() {
    root.wanted = false
    // The window gives up the pointer and the keyboard for the length of the carry, and only now:
    // a press that turns out to be a click must still reach the row it landed on.
    root.carried(true)
    root.Drag.active = true
  }

  // A press that ended without ever moving: the token stays unspent and expires on its own.
  function dropCarry() {
    root.wanted = false
    root.pressing = false
    if (!root.Drag.active) {
      root.dragMime = ({})
      root.carried(false)
    }
  }

  Drag.onDragFinished: {
    root.Drag.active = false
    root.pressing = false
    root.dragMime = ({})
    root.carried(false)
  }

  // Keys, grouped the way the board groups them: move around, change the pile, do something with it.
  // The shelf card is its own context, so these letters collide with nothing in the pane.
  function key(event) {
    var items = root.rows
    if (items.length === 0 && root.stripIndex < 0) {
      return
    }
    var shift = (event.modifiers & Qt.ShiftModifier) !== 0
    var control = (event.modifiers & Qt.ControlModifier) !== 0
    if (root.stripIndex >= 0) {
      root.stripKey(event)
      return
    }
    if (event.key === Qt.Key_A && control) {
      root.chosen = Model.chooseAll(root.chosen, items)
      event.accepted = true
      return
    }
    if (control) {
      return
    }
    // A card can sit with no cursor, and a key that acts on a row must not invent one.
    var at = root.cursorIndex
    if (event.key === Qt.Key_J || event.key === Qt.Key_Down) {
      root.step(1, shift)
    } else if (event.key === Qt.Key_K || event.key === Qt.Key_Up) {
      root.step(-1, shift)
    } else if (event.key === Qt.Key_V && at >= 0) {
      root.chosen = Model.toggleChosen(root.chosen, items[at].path)
    } else if (event.key === Qt.Key_X && !shift && at >= 0) {
      root.removeRequested(at)
    } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && at >= 0) {
      root.openRequested(items[at].path)
    } else if (event.key === Qt.Key_Tab && root.stripShown) {
      root.stripIndex = 0
    } else if (root.actionFor(event.key).length > 0) {
      root.actionRequested(root.actionFor(event.key))
    } else {
      return
    }
    event.accepted = true
  }

  // Every action carries a key, and this is the same table read backwards.
  function actionFor(key) {
    for (var i = 0; i < root.actions.length; i++) {
      if (key === root.actions[i].key.toUpperCase().charCodeAt(0)) {
        return root.actions[i].id
      }
    }
    return ""
  }

  // Tab jumps to the action buttons; the arrows walk them, enter runs one and tab comes back.
  function stripKey(event) {
    if (event.key === Qt.Key_Tab) {
      root.stripIndex = -1
    } else if (event.key === Qt.Key_L || event.key === Qt.Key_Right) {
      root.stripIndex = (root.stripIndex + 1) % root.actions.length
    } else if (event.key === Qt.Key_H || event.key === Qt.Key_Left) {
      root.stripIndex = (root.stripIndex + root.actions.length - 1) % root.actions.length
    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      root.actionRequested(root.actions[root.stripIndex].id)
    } else {
      return
    }
    event.accepted = true
  }

  // Rule 3's pointer half: the three gestures Flea's own listing has, so a hand on the mouse can do
  // what the keyboard does. A range is taken from the last row clicked and not from the cursor,
  // because the pointer moves the cursor on its way to the second click.
  property int anchorIndex: -1

  function markRow(index) {
    var item = root.rows[index]
    if (item) {
      root.chosen = Model.toggleChosen(root.chosen, item.path)
      root.anchorIndex = index
    }
  }

  function markRange(index) {
    var from = root.anchorIndex >= 0 ? root.anchorIndex : index
    root.chosen = Model.chooseRange(root.chosen, root.rows, from, index)
  }

  function clearMarks(index) {
    root.chosen = ({})
    root.cursorIndex = index
    root.anchorIndex = index
  }

  // The card is taller than its cap on any real pile, so a cursor the keyboard moved has to bring
  // the view with it: nothing else scrolls this Flickable.
  onCursorIndexChanged: root.showCursor()

  function showCursor() {
    var item = drawn.itemAt(root.cursorIndex)
    if (!item || flick.height <= 0) {
      return
    }
    var top = item.y
    var bottom = top + item.height
    var to = flick.contentY
    if (top < to) {
      to = top
    } else if (bottom > to + flick.height) {
      to = bottom - flick.height
    }
    flick.contentY = Math.max(0, Math.min(to, Math.max(0, flick.contentHeight - flick.height)))
  }

  // j and k move the cursor; with shift they take everything they pass, which is the range gesture.
  function step(by, extending) {
    var items = root.rows
    if (items.length === 0) {
      return
    }
    // No cursor yet: j starts above the first row and k starts below the last, so one press of
    // either lands on the end it came from.
    var was = root.cursorIndex < 0 ? (by > 0 ? -1 : items.length) : root.cursorIndex
    var now = Math.max(0, Math.min(items.length - 1, was + by))
    root.cursorIndex = now
    if (extending) {
      root.chosen = Model.chooseRange(root.chosen, items, Math.max(0, was), now)
    }
  }

  // Rule 2: every measure is a token. The width is the one 8 of the 13 OEM widgets use, and the
  // height stops where the shell's own keyboard panels stop.
  readonly property real cardWidth: Style.space(380)
  readonly property real cardCap: Style.space(560)
  // Directive 68 as amended: the row's leading slot is ui/Row.qml's, in the shell's own tokens, so a
  // thumbnail, a kind mark and a checkbox all sit one distance from the name. Flea's slot is its row's
  // line box, its bodySmall by its own 1.8 ratio, and its inset and gap are rowPaddingX and rowGap.
  readonly property real slotSize: Math.round(Style.font.bodySmall * 1.8)
  // Actions rule 3: Send is absent entirely on a box with no Tailscale, not greyed and not empty.
  property bool sendable: true
  readonly property var actions: [
    { id: "move", label: "Move", key: "m" },
    { id: "copy", label: "Copy", key: "c" },
    { id: "zip", label: "Zip", key: "a" },
    { id: "send", label: "Send", key: "t" },
    { id: "paths", label: "Paths", key: "y" },
    { id: "pin", label: "Pin", key: "p" }
  ].filter(function (action) { return action.id !== "send" || root.sendable })

  // Rule 6: the same key both ways, so the button says which way it goes for the row under the cursor.
  function labelFor(action) {
    var row = root.rows[root.cursorIndex]
    return action.id === "pin" && row && row.pinned ? "Unpin" : action.label
  }

  function tipFor(action) {
    return Model.actionTip(root.labelFor(action), action.key, root.keyHints)
  }

  implicitWidth: root.cardWidth
  implicitHeight: Math.min(column.implicitHeight, root.cardCap)

  // The pointer route to Recent piles. Only the right button, so every left press still reaches a
  // row's own trailing button, a capture row's click and the lift.
  MouseArea {
    anchors.fill: parent
    acceptedButtons: Qt.RightButton
    onClicked: root.menuRequested()
  }

  Flickable {
    id: flick
    anchors.fill: parent
    contentWidth: width
    contentHeight: column.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.VerticalFlick
    interactive: contentHeight > height
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    Column {
      id: column
      width: flick.width
      // The gap both OEM panels put between rows of a list, so the shelf's pitch is theirs.
      spacing: Style.space(6)

      ShelfRun {
        width: parent.width
        run: root.run
        foreground: root.foreground
        muted: root.muted
        accent: root.accent
        fontFamily: root.fontFamily
        pad: 0
        onCancelRequested: root.cancelRequested()
      }

      // Rule 10: an empty pile says one word, in the section headers' own treatment, and the groups
      // that have anything follow it. Empty is a state, not a failure: no sentence, no onboarding.
      PanelSectionHeader {
        width: parent.width
        visible: root.emptyShown
        text: "EMPTY"
        foreground: root.foreground
        fontFamily: root.fontFamily
      }

      Repeater {
        id: drawn
        // While an action runs the body is the transfer surface, not the list: the card has one body.
        model: root.run.running ? [] : root.rows

        // Rule 2 and rule 3: a separator and a section header open a group, then one-line rows.
        ShelfRow {
          card: root
          width: column.width
        }
      }

      // EdgeRail: the line that says where the ones being dragged in will land, which is after the
      // pile, because the shelf holds what it was given in the order it was given it.
      Rectangle {
        width: parent.width
        height: visible ? Math.max(1, Style.space(2)) : 0
        visible: root.incoming > 0
        color: root.accent
      }

      // Rule 9: one transient line, in the place the OEM panels put their own transient copy.
      Text {
        width: parent.width
        visible: text !== ""
        text: root.run.running ? Run.runFooter(root.run)
                               : root.error !== "" ? root.error : root.result
        color: !root.run.running && root.error !== "" ? root.urgent : root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
        elide: Text.ElideMiddle
        textFormat: Text.PlainText
      }

      PanelSeparator {
        visible: strip.visible
        width: parent.width
        foreground: root.foreground
      }

      // Rule 8: the six actions as icon buttons in one row under the last separator, with tooltips.
      // No key legend: the README's keyboard map holds the keys.
      RowLayout {
        id: strip
        width: parent.width
        // Rule 10: on an empty pile there is no action row at all, and a card holding something
        // shows it once a row is under the cursor or the pointer.
        visible: root.stripShown
        spacing: Style.spacing.controlGap

        Repeater {
          model: root.actions

          // The delegate carries the model's own two properties, so the button type itself stays a
          // plain button every other caller can instantiate.
          Item {
            id: slot
            required property int index
            required property var modelData
            Layout.alignment: Qt.AlignVCenter
            implicitWidth: button.implicitWidth
            implicitHeight: button.implicitHeight

            ShelfActionButton {
              id: button
              anchors.fill: parent
              path: Model.ACTION_GLYPHS[slot.modelData.id]
              tooltipText: root.tipFor(slot.modelData)
              hasCursor: root.stripIndex === slot.index
              foreground: root.foreground
              onClicked: root.actionRequested(slot.modelData.id)
            }
          }
        }

        Item { Layout.fillWidth: true }
      }
    }
  }
}
