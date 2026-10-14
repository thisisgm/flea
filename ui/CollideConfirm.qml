import QtQuick
import qs.Commons
import "." as Flea
import "js/Collide.js" as Collide
import "js/Icons.js" as Icons

// One choice for every name a paste or a drop would land on, on ui/TrashConfirm.qml's card, keys and look.
FocusScope {
    id: root
    anchors.fill: parent
    visible: opened || opacity > 0
    // Closed means no input, even while the fade-out is still drawn.
    enabled: opened
    opacity: 0
    property bool opened: false
    property string titleText: ""
    property var names: []
    property string moreText: ""
    property string focusName: Collide.START
    // Merge is present only when a folder lands on a folder; the second card never offers it.
    property bool showMerge: false
    readonly property string explainText: Collide.EXPLAIN
    readonly property real cardPadding: Theme.card.padding
    readonly property real cardBottomPadding: Theme.card.padding
    // The card grows with the labels rather than wrapping one, and Keep both stays the default with the ring.
    readonly property real contentWidth: Math.max(Theme.card.narrowContent, buttons.implicitWidth)
    signal chosen(string choice)
    readonly property var cardItem: card
    readonly property bool titleTruncated: title.truncated
    readonly property bool buttonsFit: buttons.width <= body.holderWidth
    readonly property int explainLines: explain.lineCount
    function buttonItem(name) { return ({cancel: cancelButton, skip: skipButton, keep: keepButton, merge: mergeButton, replace: replaceButton})[name] || null }
    function open(heading, list, more, merge) {
        titleText = heading
        names = list
        moreText = more
        showMerge = merge === true
        // A hidden Merge never holds the ring: the card opens on Keep both either way.
        focusName = Collide.START
        body.contentY = 0
        opened = true
        forceActiveFocus()
    }
    function choose(choice) {
        opened = false
        chosen(choice)
    }
    Flea.Reveal { target: root; shown: root.opened; lift: card }
    // A press lands on its own button first, so the accent never shows one choice while another is taken.
    function press(choice) {
        focusName = choice
        choose(choice)
    }
    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Escape) root.choose("cancel")
        else if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) { event.accepted = true; return }
        else if (Collide.activates(event.key)) root.choose(root.focusName)
        else root.focusName = Collide.moved(root.focusName, event.key, root.showMerge, event.modifiers)
        if (root.opened) body.reveal(root.buttonItem(root.focusName))
        event.accepted = true
    }
    Flea.DialogScrim {
        onDismissed: root.choose("cancel")
    }
    Rectangle {
        id: card
        x: Theme.cardOrigin(root.width, width)
        y: Theme.cardOrigin(root.height, height)
        width: Theme.cardSpan(root.contentWidth + 2 * root.cardPadding + 2 * Theme.spacing.hairline, root.width - 2 * Theme.card.inset)
        height: Theme.cardSpan(body.wanted + root.cardPadding + root.cardBottomPadding + 2 * Theme.spacing.hairline, root.height - 2 * Theme.card.inset)
        color: Theme.color.surface
        border.color: Theme.color.mutedFrame
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onWheel: function(wheel) { wheel.accepted = true }
        }
        Flea.CardFrame {
            id: body
            bleed: Theme.focusClearance
            anchors.fill: parent
            anchors.leftMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.rightMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.topMargin: root.cardPadding + Theme.spacing.hairline - Theme.focusClearance
            anchors.bottomMargin: root.cardBottomPadding + Theme.spacing.hairline - Theme.focusClearance
            title: Flea.DialogHeader {
                id: title
                width: parent.width
                text: root.titleText
            }
            buttons: Row {
                id: buttons
                anchors.right: parent.right
                spacing: Theme.spacing.gap
                Flea.DialogButton { id: cancelButton; label: Collide.LABELS.cancel; focused: root.focusName === "cancel"; onActivated: root.press("cancel") }
                Flea.DialogButton { id: skipButton; label: Collide.LABELS.skip; focused: root.focusName === "skip"; onActivated: root.press("skip") }
                Flea.DialogButton { id: keepButton; label: Collide.LABELS.keep; primary: true; focused: root.focusName === "keep"; onActivated: root.press("keep") }
                Flea.DialogButton { id: mergeButton; visible: root.showMerge; label: Collide.LABELS.merge; focused: root.focusName === "merge"; onActivated: root.press("merge") }
                Flea.DialogButton { id: replaceButton; label: Collide.LABELS.replace; destructive: true; focused: root.focusName === "replace"; onActivated: root.press("replace") }
            }
            Column {
                width: parent.width
                spacing: Theme.spacing.gap
                Column {
                    width: parent.width
                    Repeater {
                        model: root.names
                        delegate: Row {
                            id: nameRow
                            required property var modelData
                            width: parent.width
                            height: Theme.fileRowHeight
                            spacing: Theme.spacing.gap
                            Flea.Glyph { width: Theme.markSize; height: parent.height; name: nameRow.modelData.d ? "folder" : Icons.glyphFor(nameRow.modelData.i); color: Theme.color.foreground }
                            Text { width: Math.max(0, parent.width - Theme.markSize - parent.spacing); anchors.verticalCenter: parent.verticalCenter; text: nameRow.modelData.n; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.color.foreground; font { family: Theme.font.family; pixelSize: Theme.font.body } }
                        }
                    }
                    Text {
                        width: parent.width
                        visible: root.moreText.length > 0
                        text: root.moreText
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Theme.color.foreground
                        font { family: Theme.font.family; pixelSize: Theme.font.caption }
                    }
                }
                Text {
                    id: explain
                    width: parent.width
                    text: root.explainText
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.color.foreground
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                }
            }
        }
    }
}
