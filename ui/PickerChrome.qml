import QtQuick
import QtQuick.Layouts
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Picker.js" as Picker

// The picker's two chrome strips: what was asked for on top, where the list is standing underneath; the answers are ui/PickerAnswers.qml.
Item {
    id: root
    // Its Jump dropdown hangs below the strip, so the chrome rises over the places and the listing while it shows.
    z: entry.jumpShown || hiddenButton.hovered ? 1 : 0

    property var picker: null
    property var submissionFocus: null
    property var listing: null
    property var backend: null

    readonly property bool editing: entry.editing
    readonly property var jump: entry.jump
    function startEdit() { if (root.inputLive) entry.startEdit() }

    function submissionChanged() {
        var window = root.Window.window
        if (root.picker.submitting) {
            var before = window ? window.activeFocusItem : null
            root.submissionFocus = null
            root.picker.stepFocus(null, false)
            root.submissionFocus = {before: before, stepped: window ? window.activeFocusItem : null}
            return
        }
        var pending = root.submissionFocus
        // Replies clear submitting before they answer and before disabled controls re-enable.
        Qt.callLater(function() {
            if (!pending || root.submissionFocus !== pending) return
            root.submissionFocus = null
            if (root.picker.answered || root.picker.submitting || root.picker.backendUnavailable) return
            var before = pending.before
            if (before && before.visible && before.enabled && pending.stepped && pending.stepped.activeFocus)
                before.forceActiveFocus()
        })
    }

    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() {
            if (root.submissionFocus && root.Window.window.activeFocusItem !== root.submissionFocus.stepped)
                root.submissionFocus = null
        }
    }

    signal backRequested()
    signal upRequested()
    signal viewChosen(string mode)

    readonly property var req: root.picker.req

    readonly property color edge: root.picker.edge

    implicitHeight: ask.height + where.height

    // Back and Up rest in the foreground like the main chrome; only an unlit view mark rests muted.
    component Mark: Flea.ChromeButton {
        id: mark
        ruleRows: Theme.spacing.hairline
        property bool lit: false
        readonly property bool available: mark.enabled
        property string name: mark.accessName
        gesturePolicy: TapHandler.ReleaseWithinBounds
        restingColor: Theme.color.foreground
        activeFocusOnTab: mark.enabled
        keyboardFocused: mark.activeFocus
        Keys.onTabPressed: function(event) { root.picker.stepFocus(mark, (event.modifiers & Qt.ShiftModifier) !== 0) }
        Keys.onBacktabPressed: root.picker.stepFocus(mark, true)
    }

    readonly property bool inputLive: !root.picker.submitting && !root.picker.backendUnavailable

    // One 33 px line at body: the caller's title as it came, then the requester after a middot when the portal names one.
    Item {
        id: ask
        objectName: "pickerAsk"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: Math.max(Theme.chromeHeight, caller.implicitHeight + 2 * Theme.spacing.rowPaddingY)

        Rectangle {
            anchors.fill: parent
            color: Theme.color.surface
        }

        RowLayout {
            id: titles
            objectName: "pickerTitleLine"
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0

            // The caller's own words, and a filename is arbitrary text, so PlainText here too.
            Text {
                id: caller
                Layout.alignment: Qt.AlignBaseline
                Layout.fillWidth: true
                Layout.maximumWidth: implicitWidth
                text: Picker.title(root.req)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.body
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }

            // Only a portal identity is drawn, in a Text of its own that the caller's title never reaches.
            Text {
                Layout.alignment: Qt.AlignBaseline
                visible: requester.visible
                text: "\u00a0\u00b7\u00a0"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Text {
                id: requester
                objectName: "pickerRequester"
                Layout.alignment: Qt.AlignBaseline
                Layout.fillWidth: true
                visible: text.length > 0
                text: Picker.subtitle(root.req)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: Theme.spacing.hairline
            color: root.edge
        }
    }

    Item {
        id: where
        objectName: "pickerWhere"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: ask.bottom
        // The board's 27 px strip at base-size 14: the chrome's own height, so the field and its dropdown sit as in the main window.
        height: Theme.chromeHeight

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: Theme.spacing.hairline
            color: root.edge
        }

        Row {
            id: moves
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            Mark {
                id: backButton
                objectName: "pickerBack"
                glyph: "arrow-left"
                enabled: !root.picker.backendUnavailable && root.picker.history.length > 0 && !root.picker.submitting
                onActivated: root.backRequested()
            }

            Mark {
                id: upButton
                objectName: "pickerUp"
                glyph: "arrow-up"
                accessName: root.picker.recent ? "Parent folder unavailable in Recent" : "Parent folder"
                // The board's own rule, drawn as its disabled Up: a history has no directory above it.
                enabled: !root.picker.backendUnavailable && !root.picker.submitting && !root.picker.recent && Picker.parentOf(root.picker.path) !== root.picker.path
                onActivated: root.upRequested()
            }
        }

        // Parents muted, the leaf lit inline after them; a long path elides its parents from the left and keeps the leaf whole.
        Row {
            id: strip
            objectName: "pickerStrip"
            anchors.left: moves.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.right: parent.right
            anchors.rightMargin: views.width + Theme.spacing.rowPaddingX + Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            // The marks centre in the band above the rule, so the path does too.
            anchors.verticalCenterOffset: -Theme.spacing.hairline / 2
            visible: !entry.editing
            // Recent is a location and not a path, so the strip says its name, never a tilde form of the token.
            readonly property string display: root.picker.recent ? Picker.RECENT_LABEL : Format.tilde(root.picker.path, root.picker.home)
            readonly property string leaf: root.picker.recent ? strip.display : Format.leafPart(strip.display)
            readonly property string parents: root.picker.recent ? "" : strip.display.substring(0, strip.display.length - strip.leaf.length)
            Text {
                id: stripParents
                width: Math.min(implicitWidth, Math.max(0, strip.width - stripLeaf.width))
                visible: strip.parents.length > 0
                text: strip.parents
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideLeft
                textFormat: Text.PlainText
            }
            Text {
                id: stripLeaf
                width: Math.min(implicitWidth, strip.width)
                text: strip.leaf
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideMiddle
                textFormat: Text.PlainText
            }
        }

        // Picker040 path entry: the shared field, whose line pathLogic below answers.
        Flea.PathEntry {
            id: entry
            anchors.left: moves.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.right: parent.right
            anchors.rightMargin: views.width + Theme.spacing.rowPaddingX + Theme.spacing.gap
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.bottomMargin: Theme.spacing.hairline
            path: pathLogic.basePath
            home: root.picker.home
            showHidden: root.picker.showHidden
            // The drawn path and its padding open the same editor as Ctrl+L.
            HoverHandler {
                enabled: root.inputLive && !entry.editing
                cursorShape: Qt.IBeamCursor
            }
            TapHandler {
                enabled: root.inputLive && !entry.editing
                acceptedButtons: Qt.LeftButton
                onTapped: root.startEdit()
            }
            onCommitLine: function (text) { pathLogic.commitTyped(text) }
            onJumpChosen: function (path) { pathLogic.ask(path) }
            onEditClosed: pathLogic.editClosed()
            onCompleteRequested: function (dir, hidden) { root.listing.peek(dir, root.picker.windowSize, hidden) }
            onSaid: function (text) { root.picker.say(text, false) }
            onJumpRequested: function (id, ranking, favourites, recent) { root.listing.jump(id, ranking, favourites, recent) }
        }

        Flea.PickerPathBar {
            id: pathLogic
            picker: root.picker
            listing: root.listing
            backend: root.backend
            entry: entry
        }
    }

    // Pointer targets with listing-key equivalents; the established Tab walk stays unchanged.
    Row {
        id: views
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: where.verticalCenter
        spacing: Theme.spacing.gap

        Mark {
            id: hiddenButton
            objectName: "pickerHidden"
            glyph: root.picker.showHidden ? "eye" : "eye-off"
            accessName: root.picker.showHidden ? "Hide hidden files" : "Show hidden files"
            active: root.picker.showHidden
            activeFocusOnTab: false
            enabled: root.inputLive
            onActivated: {
                root.picker.showHidden = !root.picker.showHidden
                if (root.picker.path.length > 0) root.picker.openWithoutHistory(root.picker.path)
            }
        }

        Mark {
            id: listButton
            objectName: "pickerListView"
            glyph: "list"
            lit: root.picker.viewMode === "list"
            restingColor: listButton.lit ? Theme.color.foreground : Theme.color.mutedFrame
            activeFocusOnTab: false
            onActivated: root.viewChosen("list")
        }

        Mark {
            id: gridButton
            objectName: "pickerGridView"
            glyph: "grid"
            lit: root.picker.viewMode === "grid"
            restingColor: gridButton.lit ? Theme.color.foreground : Theme.color.mutedFrame
            activeFocusOnTab: false
            onActivated: root.viewChosen("grid")
        }
    }

    Rectangle {
        visible: hiddenButton.hovered && hiddenButton.enabled
        x: Math.max(0, hiddenButton.mapToItem(root, hiddenButton.width, 0).x - width)
        y: hiddenButton.mapToItem(root, 0, hiddenButton.height).y
        width: hiddenHint.implicitWidth + 2 * Theme.spacing.gap
        height: hiddenHint.implicitHeight + 2 * Theme.spacing.rowPaddingY
        color: Theme.color.surface
        Text {
            id: hiddenHint
            anchors.centerIn: parent
            text: hiddenButton.accessName
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            color: Theme.color.foreground
            textFormat: Text.PlainText
        }
    }

    function focusItems() {
        return [backButton, upButton]
    }
    function controls() {
        return root.focusItems().concat([hiddenButton, listButton, gridButton]).map(function(item) { return root.picker.control(item.name, item, item.available) })
    }
}
