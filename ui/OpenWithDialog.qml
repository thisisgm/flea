import QtQuick
import qs.Commons
import "." as Flea

// The Open with dialog: the desktop's whole installed list, searched, with the "always" box the
// flyout deliberately does not carry; its one write is the default. Hosted by ui/shell.qml over
// the pane, the way the convert popup is.
Item {
    id: root

    property bool opened: false
    // The file the dialog opened for, the row's Kind description for the always label, and the
    // holder the rest is reached through: the pane's opener for the launch, its backend for the
    // list and the write.
    property string filePath: ""
    property string kind: ""
    property var holder: null
    // The card, for ui/Ipc.qml the way the convert popup exposes its own. var, not Item:
    // BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card

    // The wire's answer, and the search's window over it. The list is asked once per open;
    // a second open asks again, because applications install while the window is open.
    property var apps: []
    property var shown: []
    property string search: ""
    property int cursor: 0
    // Unchecked by default, the convert popup's own reason: a default written silently is a
    // setting the operator did not ask for. Space or the click toggles it.
    property bool always: false

    signal closed()

    readonly property int dialogWidth: 380
    readonly property int clampMargin: 8

    anchors.fill: parent
    visible: root.opened
    z: 2

    function open(path, kind, holder) {
        root.filePath = path
        root.kind = kind
        root.holder = holder
        root.search = ""
        root.always = false
        root.refilter()
        if (holder && holder.backend)
            holder.backend.askApplications()
        root.opened = true
        field.forceActiveFocus()
    }

    function close() {
        if (!root.opened)
            return
        root.opened = false
        if (root.holder)
            root.holder.forceActiveFocus()
    }

    // The search's own window: a case-insensitive name match, the pane's own filter shape; a
    // refilter resets the cursor, because a cursor at 40 rows names another application once
    // the list narrows.
    function refilter() {
        var text = root.search.toLowerCase()
        var out = []
        for (var i = 0; i < root.apps.length; i++) {
            if (text.length === 0 || root.apps[i].name.toLowerCase().indexOf(text) >= 0)
                out.push(root.apps[i])
        }
        root.shown = out
        if (root.cursor >= out.length)
            root.cursor = Math.max(0, out.length - 1)
        // A narrowed list can leave the viewport scrolled past its content; the filter that
        // answers two rows must draw them, not air.
        if (list.contentY > list.contentHeight - list.height)
            list.contentY = Math.max(0, list.contentHeight - list.height)
    }

    function moveCursor(delta) {
        var n = root.shown.length
        if (n === 0)
            return
        root.cursor = Math.max(0, Math.min(n - 1, root.cursor + delta))
        var row = repeater.itemAt(root.cursor)
        if (row)
            list.reveal(row)
    }

    // One launch through the pane's opener, and — only when the always box says so — one
    // default written for the type the backend resolves from the path; the write is the only
    // thing the dialog adds beyond the flyout's own act.
    function commit() {
        var app = root.shown[root.cursor]
        if (!app || !root.holder)
            return
        var path = root.filePath
        root.close()
        root.holder.openWithPath(path, app.path)
        if (root.always && root.holder.backend)
            root.holder.backend.setDefault(path, app.id)
    }

    // A dimmed ground, and a click on it is a cancel, the same shape the convert popup uses.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.5

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            hoverEnabled: true
            onClicked: root.close()
            onWheel: function (wheel) { wheel.accepted = true }
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Theme.space(root.dialogWidth)
        readonly property int pad: Theme.spacing.rowPaddingX
        // How many rows the list claims before the card clamps to the window; a shorter window
        // gives the list less, never the controls below it. The chrome above and the controls
        // below are fixed, and the list viewport takes whatever the clamped card leaves between
        // them — the first cut laid the whole body out as one column, and its unclipped
        // delegates painted the later rows straight over the controls.
        readonly property int rowsShown: 7
        height: Math.min(card.pad + topCol.height + Theme.spacing.gap + card.rowsShown * Theme.rowHeight
                         + Theme.spacing.gap + bottomCol.height + card.pad,
                         root.height - 2 * root.clampMargin)
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.muted
        radius: Style.cornerRadius

        // The search field's own press must not fall through to the closing ground.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: {}
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Column {
            id: topCol
            x: card.pad
            y: card.pad
            width: parent.width - 2 * card.pad
            spacing: 0

            Text {
                id: title
                width: parent.width
                bottomPadding: Theme.spacing.gap
                text: "Open with"
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.body
                font.bold: true
                textFormat: Text.PlainText
            }

            Rectangle {
                width: parent.width
                height: Theme.spacing.hairline
                color: Theme.color.muted
                opacity: 0.4
            }

            // The dialog's whole filter; every other key stays text, the field owning the keyboard.
            Item {
                width: parent.width
                height: Theme.rowHeight

                Flea.Glyph {
                    id: searchMark
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.markSize
                    height: Theme.markSize
                    name: "search"
                    color: Theme.color.muted
                }

                TextInput {
                    id: field
                    anchors.left: searchMark.right
                    anchors.leftMargin: Theme.spacing.gap
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    clip: true
                    selectByMouse: true
                    text: root.search
                    onTextChanged: {
                        if (!root.opened)
                            return
                        root.search = text
                        root.refilter()
                    }

                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Escape) { root.close(); event.accepted = true; return }
                        if (event.key === Qt.Key_Down) { root.moveCursor(1); event.accepted = true; return }
                        if (event.key === Qt.Key_Up) { root.moveCursor(-1); event.accepted = true; return }
                        if (event.key === Qt.Key_PageDown) { root.moveCursor(10); event.accepted = true; return }
                        if (event.key === Qt.Key_PageUp) { root.moveCursor(-10); event.accepted = true; return }
                        if (event.key === Qt.Key_Space) {
                            // A space belongs to the filter first, so the always box is only
                            // toggled from an empty line, where there is nothing left to type.
                            if (field.text.length === 0) { root.always = !root.always; event.accepted = true }
                            return
                        }
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { root.commit(); event.accepted = true }
                    }
                }

                    Flea.Glyph {
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.spacing.gap
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.font.caption
                        height: Theme.font.caption
                        visible: root.search.length > 0
                        name: "x"
                        color: Theme.color.muted

                        TapHandler {
                            acceptedButtons: Qt.LeftButton
                            onTapped: { field.text = ""; field.forceActiveFocus() }
                        }
                    }
                }

                // The file the dialog is choosing for, the way Nautilus's chooser names it.
                Text {
                    width: parent.width
                    bottomPadding: Theme.spacing.gap
                    text: "Choose an application to open " + root.filePath.substring(root.filePath.lastIndexOf("/") + 1) + " with"
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                }
            }

        // The list's own viewport: a clipped band whose height is the clamped card's remainder.
        Flea.CardScroll {
            id: list
            x: card.pad
            y: card.pad + topCol.height + Theme.spacing.gap
            width: parent.width - 2 * card.pad
            height: Math.max(2 * Theme.rowHeight,
                             card.height - card.pad - bottomCol.height - Theme.spacing.gap - y)

            Column {
                id: rows
                width: parent.width

                Repeater {
                    id: repeater
                    model: root.shown

                    delegate: Flea.MenuRow {
                        required property var modelData
                        required property int index
                        width: rows.width
                        entry: ({ label: modelData.name, glyph: "app-window" })
                        // The entry's own Icon=; the cut glyph only when the theme carries neither it nor
                        // the generic one.
                        icon: modelData.icon || ""
                        current: root.cursor === index
                        onActivated: { root.cursor = index; root.commit() }
                    }
                }
            }

            // No match is a state, in the pane's own register; one long line centered in a
            // clipped viewport cut its ends as the filter grew, so the message wraps.
            Text {
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.spacing.rowPaddingX
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                visible: root.shown.length === 0
                text: root.search.length > 0 ? "No application matches " + root.search + "."
                                             : "No application on this system can be offered."
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }
        }

        Column {
            id: bottomCol
            x: card.pad
            width: parent.width - 2 * card.pad
            anchors.bottom: parent.bottom
            anchors.bottomMargin: card.pad
            spacing: 0

            Rectangle {
                width: parent.width
                height: Theme.spacing.hairline
                color: Theme.color.muted
                opacity: 0.4
            }

            // The always box, the convert popup's toggle; the write it stands for is the default
            // for the row's type, named by the listing's Kind.
            Item {
                id: alwaysRow
                width: parent.width
                // The label wraps to what the card's width makes of it, so the row is as tall as
                // it really is rather than one row with the rest elided away.
                height: Math.max(Theme.rowHeight, alwaysLabel.implicitHeight + Theme.spacing.gap)

                Rectangle {
                    id: box
                    anchors.left: parent.left
                    anchors.leftMargin: Theme.spacing.rowPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.font.caption
                    height: Theme.font.caption
                    color: "transparent"
                    border.width: Theme.spacing.hairline * 2
                    border.color: root.always ? Theme.color.accent : Theme.color.muted

                    Flea.Glyph {
                        anchors.fill: parent
                        visible: root.always
                        name: "check"
                        color: Theme.color.accent
                    }
                }

                Text {
                    id: alwaysLabel
                    anchors.left: box.right
                    anchors.leftMargin: Theme.spacing.gap
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.kind.length > 0 ? "Always use this application for " + root.kind + " files"
                                               : "Always use this application for this kind of file"
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.body
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }

                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onTapped: root.always = !root.always
                }
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                spacing: Theme.spacing.gap

                Flea.DialogButton {
                    label: "Cancel"
                    onActivated: root.close()
                }

                Flea.DialogButton {
                    label: "Open"
                    primary: true
                    onActivated: root.commit()
                }
            }
        }
    }

    // The wire's answer lands here, the columns view's own per-view reader; the plain list kept
    // beside the filtered one puts every row back on an empty line without a second ask.
    Connections {
        target: root.holder ? root.holder.backend : null
        function onApplications(apps) {
            root.apps = apps || []
            root.refilter()
        }
        // The write's terminal line; a refusal is an error line whose where ui/js/Errors.js words
        // and the pane's generic landing shows. The sentence carries the dialog's own Kind.
        function onDefaulted(ok) {
            if (ok)
                root.holder.message("The default for " + root.kind + " files changed; the next open uses the chosen application.", false)
        }
    }
}
