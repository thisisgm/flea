import QtQuick
import qs.Commons
import "." as Flea

// The Open with dialog: the desktop's whole installed list, searched, with the "always" box the
// flyout deliberately does not carry. The flyout is the one-off override; this is the Windows and
// Nautilus surface, and its one write is the default, which is why it opens for one file and
// never for a selection. Hosted by ui/shell.qml over the pane, the way the convert popup is.
Item {
    id: root

    property bool opened: false
    // The file the dialog opened for, the row's Kind description for the always label, and the
    // holder everything else is reached through: the pane's own opener for the launch and the
    // pane's backend for the list and the write.
    property string filePath: ""
    property string kind: ""
    property var holder: null
    // The card's title, for ui/Ipc.qml, the way the convert popup exposes its own.
    readonly property alias titleItem: title
    // var, not Item: BorderSurface is a qs.Ui type qmllint cannot resolve, and Item would read as incompatible.
    readonly property var cardItem: card

    // The wire's answer, and the search's window over it. The list is asked for once per open;
    // a second open asks again, because applications are installed while the window is open.
    property var apps: []
    property var shown: []
    property string search: ""
    property int cursor: 0
    // Unchecked by default, the same reason the convert popup's strip toggle is: a default written
    // silently is a setting the operator did not ask for. Space or the click toggles it.
    property bool always: false

    signal closed()

    readonly property int dialogWidth: 380
    readonly property int listHeight: 7 * Theme.rowHeight
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

    // The search's own window: a case-insensitive name match, the pane's own filter shape. A
    // refilter resets the cursor, because a cursor at 40 rows of an unfiltered list names another
    // application the moment the list narrows to two.
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
    }

    function moveCursor(delta) {
        var n = root.shown.length
        if (n === 0)
            return
        var next = root.cursor + delta
        if (next < 0)
            next = 0
        if (next > n - 1)
            next = n - 1
        root.cursor = next
        var row = repeater.itemAt(root.cursor)
        if (row)
            body.reveal(row)
    }

    // One launch of the picked application through the pane's own opener, and — only when the
    // always box says so — one default written for the type the backend resolves from the path.
    // The launch is the flyout's own act; the write is the only thing the dialog adds.
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
        height: Math.min(title.height + rule.height + searchRow.height + root.listHeight
                         + alwaysRow.height + buttonsRow.height + 4 * Theme.spacing.rowPaddingX,
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

        Flea.CardScroll {
            id: body
            anchors.fill: parent
            anchors.margins: Theme.spacing.rowPaddingX

            Column {
                width: parent.width
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
                    id: rule
                    width: parent.width
                    height: Theme.spacing.hairline
                    color: Theme.color.muted
                    opacity: 0.4
                }

                // The search line, the dialog's whole filter. Every key that is not one of the
                // four stays text: the field owns the keyboard, which is what a search field is.
                Item {
                    id: searchRow
                    width: parent.width
                    height: Theme.rowHeight

                    Text {
                        id: searchMark
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.markSize
                        text: "/"
                        color: Theme.color.muted
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.body
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

                // The list itself, its height fixed so the card holds its shape across filters.
                Item {
                    width: parent.width
                    height: Math.min(root.listHeight, root.shown.length * Theme.rowHeight)

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
                                current: root.cursor === index
                                onActivated: { root.cursor = index; root.commit() }
                            }
                        }
                    }

                    // No match is a state and not an absence: the pane's own empty answer names the
                    // query, and this one does the same in the same register.
                    Text {
                        anchors.centerIn: parent
                        visible: root.shown.length === 0
                        text: root.search.length > 0 ? "No application matches " + root.search + "."
                                                     : "No application on this system can be offered."
                        color: Theme.color.muted
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        textFormat: Text.PlainText
                    }
                }

                Rectangle {
                    width: parent.width
                    height: Theme.spacing.hairline
                    color: Theme.color.muted
                    opacity: 0.4
                }

                // The always box, drawn the convert popup's toggle is drawn: a 24-grid square with
                // the check glyph inside it when it is ticked. The write it stands for is the
                // default for the row's own type, named here by the Kind the listing shows.
                Item {
                    id: alwaysRow
                    width: parent.width
                    height: Theme.rowHeight

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
                        elide: Text.ElideRight
                    }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        gesturePolicy: TapHandler.ReleaseWithinBounds
                        onTapped: root.always = !root.always
                    }
                }

                Row {
                    id: buttonsRow
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
    }

    // The wire's answer lands here, the same per-view reader the columns view is: the dialog asked
    // for the list and this is the one surface that draws it. The plain list is kept as well as
    // the filtered one, so clearing the search line puts back every row without a second ask.
    Connections {
        target: root.holder ? root.holder.backend : null
        function onApplications(apps) {
            root.apps = apps || []
            root.refilter()
        }
        // The write's own terminal line. The success sentence carries the application and the Kind
        // the dialog opened with; a refusal is an error line whose where is the request's own, so
        // ui/js/Errors.js words it and the pane's generic error landing shows it.
        function onDefaulted(ok) {
            if (ok)
                root.holder.message("The default for " + root.kind + " files changed; the next open uses the chosen application.", false)
        }
    }
}
