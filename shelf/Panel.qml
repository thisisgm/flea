import QtQuick
import Quickshell
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model
import "Run.js" as Run

// The shelf's presence in the bar, and the host of everything the mark opens: the card, the rail,
// the menus and the keys. BarMark rule 6: it never self-hides, so its slot never moves.
Panel {
  id: root
  moduleName: "io.github.thisisgm.flea-shelf"

  // Without these the bar allocates a zero-width slot: the widget draws nothing and logs nothing.
  // SettingsRest rule 2: with the master off, or the bar row off, the slot is the one that goes.
  readonly property bool inBar: shelf.shelfSettings.enabled && shelf.shelfSettings.bar
  implicitWidth: root.inBar ? button.implicitWidth : 0
  implicitHeight: button.implicitHeight

  // BarMark rule 2: empty is 55 percent present and holding is 100, and nothing else changes.
  readonly property real emptyPresence: 0.55
  readonly property real markOpacity: shelf.holding ? 1.0 : root.emptyPresence
  // The motion vocabulary's first entry, Land: the pile grew, so presence takes 220 ms to full and
  // the accent decays over the 600 ms after it. Work and Refuse land with the units that give them
  // a trigger, the operations and the drop, because nothing animates for an event this cannot see.
  readonly property int landMs: 220
  readonly property int accentDecayMs: 600
  property real landAccent: 0

  // Rule 15: the bar's injected palette, bound once here and passed down, so a bar with its own
  // colours stays coherent inside the card.
  readonly property color foreground: root.bar ? root.bar.foreground : Commons.Color.popups.text
  // One grey for the whole panel, at the factor the OEM agents panel calls dim.
  readonly property color muted: Qt.darker(root.foreground, 1.55)
  readonly property color urgentColor: root.bar ? root.bar.urgent : Commons.Color.urgent
  // The bar's own face, the way every OEM panel takes it: a card in Style.font.family beside panels
  // in the bar's family reads as a different application.
  readonly property string fontFamily: root.bar ? root.bar.fontFamily : Style.font.family

  // The card's own slot, rule 5: one voice at a time, and the pile's own state owns neither.
  property string result: ""
  property string error: ""
  // Summon: the card's menu holds the last five piles, and a cleared pile says how to get it back
  // for the four seconds a transient lives.
  property bool menu: false
  // True from the lift until the platform drag ends, however it ended.
  property bool carrying: false
  readonly property int transientMs: 4000

  // A card that closed while its menu was up must come back as the pile, not as the menu: the menu is
  // a detour, and the shelf is what the next summon is asking for. A closed card also forgets the
  // gesture it was in the middle of, because the next one is a new question.
  onOpenedChanged: {
    if (root.opened) {
      card.cursorIndex = card.rows.length > 0 ? 0 : -1
      // The peers answer decides whether the strip carries Send at all, so it is asked on the way
      // in rather than when the action is pressed.
      doing.askPeers()
      return
    }
    root.menu = false
    root.pending = ""
    root.carrying = false
    root.result = ""
    root.error = ""
    card.chosen = ({})
    card.cursorIndex = -1
    card.hoveredIndex = -1
    card.anchorIndex = -1
    card.stripIndex = -1
  }

  ShelfService {
    id: shelf
    settings: root.settings
    // EdgeRail rule 1: the default edge is the one opposite the bar, so the rail needs to know it.
    barPosition: root.bar ? root.bar.position : "top"
    // Rule 4's budget: sizes are asked for while the card is up and never while it is closed.
    drawing: root.opened
    onGrew: landing.restart()
    // Summon: a keybind, a CLI call and the mark all arrive here, by the same path, and the master
    // makes every one of them inert without touching the pile.
    onSummoned: {
      if (!shelf.shelfSettings.enabled) {
        return
      }
      root.opened ? root.close() : root.open()
    }
    onCleared: function (count) {
      root.error = ""
      root.result = Model.clearedText(count)
      transient.restart()
    }
    onUndone: function (kind, count) {
      root.error = ""
      root.result = Model.undoneText(kind, count)
      transient.restart()
    }
    onMinted: function (token, moving) { card.lift(token, !moving) }
    onFailed: function (why) {
      // Rule 5: one voice. An error takes the slot from a result and expires the same way.
      root.result = ""
      root.error = why
      root.carrying = false
      transient.restart()
    }
  }

  Timer {
    id: transient
    interval: root.transientMs
    onTriggered: {
      root.result = ""
      root.error = ""
    }
  }

  // EdgeRail: the one summon path that is always live, which is why it is the one that must be
  // switchable. Off, left, right or bottom, defaulting to the edge opposite the bar.
  ShelfRail {
    id: rail
    edge: shelf.railEdge
    dwellMs: shelf.railDwellMs
    held: shelf.count
    foreground: root.foreground
    onDropped: function (paths) {
      root.error = ""
      shelf.addAll(paths)
    }
    onDwelled: if (!root.opened) root.open()
    onOpened: root.toggle()
  }

  // Actions: the doing half, which the card's strip and its five letters reach through here.
  ShelfActions {
    id: doing
    fleaCommand: shelf.fleaCommand
    onRan: shelf.reread()
    onRefused: function (why) {
      root.result = ""
      root.error = why
      transient.restart()
    }
    onLanded: function (sentence) {
      root.error = ""
      root.result = sentence
      transient.restart()
    }
    onBrowsed: function (dest) { root.runChosen(dest) }
  }

  // What the strip asked for and has not been given a destination for yet.
  property string pending: ""
  property var flyoutRows: []

  function actOn(id) {
    var paths = Model.actionPaths(card.chosen, card.rows)
    if (id === "pin") {
      // Chosen-or-whole has one exception in the other direction: with nothing chosen, Pin is about
      // the row under the cursor rather than the whole pile, because pinning a pile is not a gesture.
      if (card.chosenCount === 0) {
        card.pinRequested(card.cursorIndex)
        return
      }
      root.error = ""
      card.followPath = ""
      shelf.pinAll(paths, !Model.allPinned(paths, card.rows))
      return
    }
    if (paths.length === 0) {
      return
    }
    if (id === "paths") {
      Quickshell.clipboardText = doing.yank(paths)
      root.error = ""
      root.result = Run.copiedPathsText(paths.length)
      transient.restart()
      return
    }
    if (id === "zip") {
      doing.zip(Model.today(), paths)
      return
    }
    if (id === "send") {
      doing.askPeers()
      root.pending = "send"
      root.flyoutRows = doing.peers
      return
    }
    doing.askPlaces()
    root.pending = id
    root.flyoutRows = doing.places
  }

  // The flyout's rows arrive a moment after it opens, because the list is another process's answer.
  Connections {
    target: doing
    // Only the first answer lands: the flyout's own keys are positional, so a late listing would
    // move the row under the number the operator is about to press.
    function onPlacesChanged() {
      if ((root.pending === "move" || root.pending === "copy") && root.flyoutRows.length === 0) root.flyoutRows = doing.places
    }
    function onPeersChanged() {
      if (root.pending === "send" && root.flyoutRows.length === 0) root.flyoutRows = doing.peers
    }
  }

  function runChosen(dest) {
    var paths = Model.actionPaths(card.chosen, card.rows)
    if (root.pending === "send") {
      doing.send(dest, paths)
    } else if (root.pending.length > 0) {
      doing.transfer(root.pending === "move", dest, paths)
    }
    root.pending = ""
    card.chosen = ({})
  }

  SequentialAnimation {
    id: landing
    PropertyAction { target: root; property: "landAccent"; value: 1 }
    PauseAnimation { duration: root.landMs }
    NumberAnimation { target: root; property: "landAccent"; to: 0; duration: root.accentDecayMs }
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    visible: root.inBar
    bar: root.bar
    // Rule 3: the count lives in the tooltip, the card and the edge notch, never in the bar.
    tooltipText: Model.tooltip(shelf.pile)
    iconComponent: Component {
      Item {
        FleaShelfMark {
          anchors.centerIn: parent
          // Rule 4: accent is transient and supporting, never the resting colour.
          color: root.landAccent > 0
                 ? Qt.tint(root.barForeground, Qt.rgba(Commons.Color.accent.r, Commons.Color.accent.g, Commons.Color.accent.b, root.landAccent))
                 : root.barForeground
          opacity: root.markOpacity
          // Rule 2's only channel, and Land's 220 ms is the step it takes to full presence.
          Behavior on opacity { NumberAnimation { duration: root.landMs } }
        }
      }
    }
    // Keys board: right click on the bar mark brings back the last pile you cleared, which is the
    // route that survives when the card is closed and the keyboard is not where your hand is. The
    // button itself already takes all three buttons and says which one in its signal.
    onPressed: function (buttonCode) {
      if (buttonCode === Qt.RightButton) {
        shelf.restore(0)
        return
      }
      root.toggle()
    }
  }

  // Rule 2 and directive 64: the card is hosted by the shell's own popup host, so a click anywhere
  // outside it, a focus loss or esc closes it, and opening another panel closes it, exactly as the
  // Dropbox and Tailscale panels behave.
  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keys
    contentWidth: panel.fittedContentWidth(Style.space(380))
    contentHeight: panel.fittedContentHeight(body.implicitHeight, Style.space(560))
    // S9, measured on this box: a full-screen dismissal surface is a surface a drag can never
    // leave, so for the length of a carry this window takes no pointer input at all and the drop
    // reaches the window underneath it.
    mask: Region {
      width: root.carrying ? 0 : panel.screenW
      height: root.carrying ? 0 : panel.screenH
    }

    // The shelf's own key catcher rather than Ui/PanelKeyCatcher: that one turns keys into semantic
    // signals and drops the modifier with them, and this map needs shift-x to clear, shift-j to
    // extend the range and ctrl-a to take everything.
    Item {
      id: keys
      anchors.fill: parent
      focus: true
      Keys.priority: Keys.BeforeItem

      // Actions: while an action runs the first esc cancels it and the card stays; with nothing
      // running esc leaves the flyout, then the menu, and only then closes the card.
      Keys.onEscapePressed: function (event) {
        event.accepted = true
        if (doing.run.running) {
          doing.cancelRun()
        } else if (root.pending.length > 0) {
          root.pending = ""
        } else if (root.menu) {
          root.menu = false
        } else {
          root.close()
        }
      }
      // Keys board: shift-x clears and the pile becomes the last pile, z undoes whichever of the
      // clear and the last move out of the pile is newer, and in the menu a number takes that pile
      // straight back.
      Keys.onPressed: function (event) {
        if (root.pending.length > 0) {
          flyout.key(event)
          return
        }
        if (root.menu) {
          pileMenu.key(event)
          return
        }
        if (event.key === Qt.Key_X && (event.modifiers & Qt.ShiftModifier)) {
          shelf.clear()
          event.accepted = true
          return
        }
        if (event.key === Qt.Key_Z) {
          shelf.undo()
          event.accepted = true
          return
        }
        // Everything else the card owns: the cursor, the subset gesture and the action buttons.
        card.key(event)
      }

      Item {
        id: body
        anchors.fill: parent
        implicitHeight: root.menu ? pileMenu.implicitHeight
                      : root.pending.length > 0 ? flyout.implicitHeight
                      : card.implicitHeight

        ShelfMenu {
          id: pileMenu
          visible: root.menu
          width: parent.width
          muted: root.muted
          piles: shelf.piles
          foreground: root.foreground
          fontFamily: root.fontFamily
          onChosen: function (index) {
            shelf.restore(index)
            root.menu = false
          }
          onDismissed: root.menu = false
        }

        ShelfFlyout {
          id: flyout
          visible: root.pending.length > 0
          width: parent.width
          muted: root.muted
          title: Run.flyoutTitle(root.pending, Model.actionPaths(card.chosen, card.rows).length)
          rows: root.flyoutRows
          browsable: root.pending === "move" || root.pending === "copy"
          foreground: root.foreground
          fontFamily: root.fontFamily
          onChosen: function (index) { root.runChosen(root.flyoutRows[index]) }
          // The chooser answers in its own time, so what it is choosing for is still owed until it
          // does: clearing pending here would drop the destination the operator picked.
          onBrowse: doing.choose(flyout.title, root.flyoutRows.length > 0 ? root.flyoutRows[0] : "")
          onDismissed: root.pending = ""
        }

        ShelfCard {
          id: card
          visible: !root.menu && root.pending.length === 0
          anchors.fill: parent
          pile: shelf.pile
          rows: Model.rows(shelf.pile, shelf.captures, shelf.sizes)
          thumbs: shelf.thumbs
          kinds: shelf.shelfSettings
          keyHints: shelf.keyHints
          incoming: rail.incoming
          sendable: doing.peerable
          // Actions: the buttons act on what the card is drawing, chosen or whole.
          foreground: root.foreground
          muted: root.muted
          urgent: root.urgentColor
          fontFamily: root.fontFamily
          result: root.result
          error: root.error
          onRemoveRequested: function (index) {
            var row = card.rows[index]
            if (!row || row.section === "capture") {
              return
            }
            root.error = ""
            shelf.forget(row.path)
          }
          onPinRequested: function (index) {
            var row = card.rows[index]
            if (!row) {
              return
            }
            root.error = ""
            card.followPath = row.path
            shelf.pin(row.path, !row.pinned)
          }
          onCaptureAddRequested: function (index) {
            var row = card.rows[index]
            if (!row) {
              return
            }
            root.error = ""
            shelf.add(row.path)
          }
          onOpenRequested: function (path) {
            root.error = ""
            shelf.open(path)
          }
          onMenuRequested: {
            shelf.askPiles()
            root.menu = true
          }
          onActionRequested: function (id) { root.actOn(id) }
          onCancelRequested: doing.cancelRun()
          run: doing.run
          onLiftRequested: {
            var carried = card.carriedPaths
            shelf.mintDrag(Model.dragMoves(carried, card.rows, true), carried)
          }
          onCarried: function (carrying) { root.carrying = carrying }
        }
      }
    }
  }
}
