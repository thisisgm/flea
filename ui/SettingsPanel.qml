import QtQuick
import qs.Commons
import "." as Flea
import "js/Keymap.js" as Keymap
import "js/Settings.js" as Settings

// The settings panel the Settings board draws: one floating surface, a fixed rail of sections and a
// pane that scrolls inside the work-area clamp. A plain overlay and not a QQC Popup, the call
// ui/ContextMenu.qml already made, because the one Controls import cost 10 ms of warm startup.
Item {
    id: root

    property bool opened: false
    property Item focusHolder: null
    // "keys", "display" or "menus"; the panel opens on Display because that is the promise it
    // carries, and the rail keeps the boards' own relative order around it.
    property string section: "display"
    property int cursor: 0
    // "rail" or "pane", which side Tab last gave the cursor to.
    property string side: "pane"

    // Border-box 560 wide with a 150 rail and a 408 pane, the board's resolved geometry at base 14;
    // ui/Theme.qml holds the derivation and the seam reports all three.
    readonly property int panelWidth: Theme.settings.panelWidth
    readonly property int railWidth: Theme.settings.railWidth
    // The Menus board's work-area clamp: a floating surface never renders taller than its bounds
    // less this margin, and the pane scrolls inside that while the rail stays put.
    readonly property int clampMargin: 8
    readonly property real groundOpacity: 0.5

    readonly property var rows: Settings.rows(root.section, {
        textSize: ViewState.textSize,
        hidden: ViewState.menuHidden,
        keyHints: ViewState.keyHints,
        preset: ViewState.keysPreset,
        baseSize: Theme.baseSize,
        monitorScale: Theme.monitorScale,
        cornerRadius: Style.cornerRadius,
        presetKeys: Keymap.PRESET_KEYS
    })

    // What a test reads instead of running OCR over the panel, the same idiom ui/KeymapSheet.qml's
    // rows() uses: one row per line, its kind, its wording and whatever value it currently holds.
    function rowsText() {
        var out = []
        for (var i = 0; i < root.rows.length; i++) {
            var row = root.rows[i]
            out.push(row.kind + "|" + row.label + "|" + (row.value !== undefined ? row.value : ""))
        }
        return out.join("\n")
    }

    function open(holder) {
        root.focusHolder = holder
        root.cursor = Settings.firstRow(root.rows)
        root.side = "pane"
        root.opened = true
        keys.forceActiveFocus()
    }

    function close() {
        if (!root.opened)
            return
        root.opened = false
        if (root.focusHolder)
            root.focusHolder.forceActiveFocus()
    }

    function showSection(id) {
        root.section = id
        root.cursor = Settings.firstRow(root.rows)
        flick.contentY = 0
    }

    // Enter and Space both land here. A choice steps rather than opening a menu of its own, because
    // no control the panel draws has more values than a walk can reach.
    function activate(index) {
        var row = root.rows[index]
        if (!row || !Settings.focusable(row))
            return
        // The hints row is the one check that is not a menu action, so it has a writer of its own.
        if (row.id === "keyHints")
            ViewState.toggleKeyHints()
        else if (row.kind === "check")
            ViewState.toggleMenuAction(row.id)
        else if (row.kind === "master")
            ViewState.toggleMenuBasic()
        else
            root.stepRowValue(index, 1)
    }

    // h, l and the two chevrons. Every writer is ui/ViewState.qml's own, which is the same state the
    // Ctrl+Shift chords reach, so a keystroke and a control can never hold two different sizes.
    function stepRowValue(index, direction) {
        var row = root.rows[index]
        if (!row || !Settings.focusable(row))
            return
        if (row.id === "textMode") {
            ViewState.toggleTextFollow()
            return
        }
        if (row.id === "textStop") {
            ViewState.stepTextSize(direction)
            return
        }
        // A check or a master is toggled by activate(), never walked, so h and l stop here.
        if (row.kind !== "choice")
            return
        var at = Settings.PRESETS.indexOf(ViewState.keysPreset)
        var next = (at + direction + Settings.PRESETS.length) % Settings.PRESETS.length
        ViewState.setKeysPreset(Settings.PRESETS[next])
    }

    // A tick on the ruler names a stop outright. It lands in the same ViewState writer stepTextSize
    // itself calls, so a click, an h and a Ctrl+Shift+Plus cannot leave two different sizes stored.
    function pickRowStop(index, stop) {
        var row = root.rows[index]
        if (!row || !Settings.focusable(row))
            return
        ViewState.setTextSize({ mode: stop })
    }

    function moveCursor(delta) {
        if (root.side === "rail") {
            var at = Settings.SECTIONS.map(function (s) { return s.id }).indexOf(root.section)
            var to = Math.max(0, Math.min(Settings.SECTIONS.length - 1, at + delta))
            root.showSection(Settings.SECTIONS[to].id)
            return
        }
        root.cursor = Settings.stepRow(root.rows, root.cursor, delta)
        root.showCursor()
    }

    // The Column inside the Flickable holds rows of two different heights, so the visible window is
    // moved onto the row itself rather than derived from an index times a row height.
    function showCursor() {
        var item = rowItems.itemAt(root.cursor)
        if (!item)
            return
        if (item.y < flick.contentY)
            flick.contentY = item.y
        else if (item.y + item.height > flick.contentY + flick.height)
            flick.contentY = item.y + item.height - flick.height
    }

    anchors.fill: parent
    visible: root.opened
    // Above the context menu and the keymap sheet: the panel is the surface that opened last.
    z: 3

    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: root.groundOpacity

        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: root.panelWidth
        // Each side carries its own inset, above the first row and below the last, the way
        // Settings.dc.html gives the rail column a 10 of its own and the pane the row padding.
        height: Math.min(Theme.chromeHeight + 2 * Theme.spacing.hairline
                         + Math.max(rail.implicitHeight + 2 * Theme.settings.railPaddingY,
                                    pane.implicitHeight + 2 * Theme.spacing.rowPaddingY),
                         root.height - root.clampMargin)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        // Mirrors hyprland decoration:rounding, same as ui/KeymapSheet.qml; 0 on a stock box stays square.
        radius: Style.cornerRadius
        clip: true

        // The board's border-box panel: the card's own border is the two outer hairlines, so the
        // 558 they leave is what the chrome, the rail and the pane are laid out inside.
        Item {
            anchors.fill: parent
            anchors.margins: Theme.spacing.hairline

            Item {
                id: chrome
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: Theme.chromeHeight

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Settings"
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.bodySmall
                    font.bold: true
                    textFormat: Text.PlainText
                }

                // The board's own header mark, the one surface control a pointer has for closing the panel.
                Item {
                    id: closeMark
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.hitMin
                    height: Theme.hitMin

                    Accessible.role: Accessible.Button
                    Accessible.name: "Close settings"
                    Accessible.onPressAction: root.close()

                    Flea.Glyph {
                        anchors.centerIn: parent
                        width: Theme.chromeMarkSize
                        height: Theme.chromeMarkSize
                        name: "x"
                        color: Theme.color.foreground
                    }

                    HoverHandler {
                        cursorShape: Qt.PointingHandCursor
                    }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        onTapped: root.close()
                    }
                }

                Text {
                    anchors.right: closeMark.left
                    anchors.rightMargin: Theme.spacing.gap
                    anchors.verticalCenter: parent.verticalCenter
                    text: "esc"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: Theme.spacing.hairline
                    color: Theme.color.muted
                    opacity: 0.4
                }
            }

            Flea.SettingsRail {
                id: rail
                anchors.left: parent.left
                anchors.top: chrome.bottom
                anchors.topMargin: Theme.settings.railPaddingY
                width: root.railWidth
                section: root.section
                focused: root.side === "rail"
                onChosen: function (id) {
                    root.side = "rail"
                    root.showSection(id)
                }
            }

            // Inside the rail's own 150, not beside it: the board's rail is a border-box whose right
            // edge is this line, which is what leaves the pane the 408 the anatomy note names.
            Rectangle {
                anchors.right: rail.right
                anchors.top: chrome.bottom
                anchors.bottom: parent.bottom
                width: Theme.spacing.hairline
                color: Theme.color.muted
                opacity: 0.4
            }

            Flickable {
                id: flick
                anchors.left: rail.right
                anchors.right: parent.right
                anchors.top: chrome.bottom
                anchors.bottom: parent.bottom
                anchors.topMargin: Theme.spacing.rowPaddingY
                contentWidth: width
                contentHeight: pane.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: pane
                    width: flick.width

                    Repeater {
                        id: rowItems
                        model: root.rows

                        delegate: Flea.SettingsRow {
                            required property var modelData
                            required property int index
                            width: pane.width
                            row: modelData
                            current: root.side === "pane" && root.cursor === index
                            onActivated: {
                                root.side = "pane"
                                root.cursor = index
                                root.activate(index)
                            }
                            onStepped: function (direction) {
                                root.side = "pane"
                                root.cursor = index
                                root.stepRowValue(index, direction)
                            }
                            onStopPicked: function (stop) {
                                root.side = "pane"
                                root.cursor = index
                                root.pickRowStop(index, stop)
                            }
                        }
                    }
                }
            }
        }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true

        Keys.onPressed: function (event) {
            event.accepted = true
            if (event.key === Qt.Key_Escape) {
                root.close()
                return
            }
            if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
                root.side = root.side === "rail" ? "pane" : "rail"
                return
            }
            if (event.key === Qt.Key_Down || event.text === "j") {
                root.moveCursor(1)
                return
            }
            if (event.key === Qt.Key_Up || event.text === "k") {
                root.moveCursor(-1)
                return
            }
            // h and l step a control, which only the pane has; from the rail they do nothing rather
            // than reaching across and changing a value the cursor is not on.
            if (event.key === Qt.Key_Right || event.text === "l") {
                if (root.side === "pane")
                    root.stepRowValue(root.cursor, 1)
                return
            }
            if (event.key === Qt.Key_Left || event.text === "h") {
                if (root.side === "pane")
                    root.stepRowValue(root.cursor, -1)
                return
            }
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                if (root.side === "rail")
                    root.side = "pane"
                else
                    root.activate(root.cursor)
            }
            // Every other key stops here: an open panel that let one through would move the cursor
            // in the listing behind it, which is the hidden-view keyboard defect AGENTS.md records.
        }
    }
}
