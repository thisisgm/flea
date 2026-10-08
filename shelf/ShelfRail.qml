import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Commons as Commons
import "Model.js" as Model

// EdgeRail: a live drop region at the screen's edge and no pixels at rest. Rule 4: the drop is
// taken the instant it arrives, and the dwell only decides whether the card opened on the way.
PanelWindow {
  id: root

  // off, left, right or bottom, defaulting to the edge opposite the bar (rules 1 and 5).
  property string edge: "right"
  property int dwellMs: 120
  property int held: 0
  property color accent: Commons.Color.accent
  property color foreground: Commons.Color.popups.text

  signal dropped(var paths)
  signal dwelled()
  signal opened()
  // How many a drag over the rail is offering, which is what the card's header says while it hovers.
  property int incoming: 0

  readonly property bool vertical: root.edge === "left" || root.edge === "right"
  // Rule 1: four pixels, and rule 2: the middle 60 percent, so neither corner is ever the target.
  readonly property real thickness: Style.space(4)
  readonly property real coverage: 0.6
  // Rule 3's notch: the pile's own size in peripheral vision, stepped rather than counted.
  readonly property real notchStep: Style.space(14)
  readonly property real notchBase: Style.space(20)

  visible: root.edge !== "off"
  color: "transparent"
  // Rule 1: exclusiveZone 0, so no window gives up a pixel of its area for the drop region.
  exclusionMode: ExclusionMode.Ignore
  WlrLayershell.namespace: "flea-shelf-rail"
  WlrLayershell.layer: WlrLayer.Overlay
  WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

  anchors {
    left: root.edge === "left"
    right: root.edge === "right"
    bottom: root.edge === "bottom"
  }

  implicitWidth: root.vertical ? root.thickness : Math.round(root.screen.width * root.coverage)
  implicitHeight: root.vertical ? Math.round(root.screen.height * root.coverage) : root.thickness

  // Rule 3: at rest and holding, the notch is all there is; at rest and empty, nothing is drawn.
  Rectangle {
    id: notch
    color: root.foreground
    opacity: drop.containsDrag ? 0 : 0.55
    visible: root.held > 0
    width: root.vertical ? parent.width : Math.min(parent.width, root.notchBase + root.held * root.notchStep)
    height: root.vertical ? Math.min(parent.height, root.notchBase + root.held * root.notchStep) : parent.height
    anchors.centerIn: parent
  }

  // Rule 2's bloom: the full region in the accent, which is the teaching moment and the acceptance.
  Rectangle {
    anchors.fill: parent
    color: root.accent
    opacity: drop.containsDrag ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: 120 } }
  }

  DropArea {
    id: drop
    anchors.fill: parent
    keys: ["text/uri-list"]

    onEntered: function (event) {
      root.incoming = Model.pathsFromUris(event.getDataAsString("text/uri-list")).length
      dwell.restart()
    }

    onExited: {
      root.incoming = 0
      dwell.stop()
    }

    onDropped: function (event) {
      dwell.stop()
      root.incoming = 0
      var paths = Model.pathsFromUris(event.getDataAsString("text/uri-list"))
      if (paths.length > 0) {
        root.dropped(paths)
        // Copy, never the proposed action: the shelf records a path and the file does not move, so
        // a source told its move succeeded would delete the original out from under the pile.
        event.accept(Qt.CopyAction)
      }
    }
  }

  // Rule 4: the dwell opens the card on the way past, and never gates the drop itself.
  Timer {
    id: dwell
    interval: root.dwellMs
    onTriggered: root.dwelled()
  }

  // Rule 4 again: a plain click on the rail opens the card, which is the pointer's own way in.
  MouseArea {
    anchors.fill: parent
    acceptedButtons: Qt.LeftButton
    onClicked: root.opened()
  }
}
