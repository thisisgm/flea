import QtQuick
import qs.Commons
import "." as Flea

// One row of the settings panel's pane, drawn from the object ui/js/Settings.js rows() built. Every
// kind is one row height, the window's own, except a hint, which wraps and takes the height its
// wording needs; the Settings board's anatomy note is where the "no metric of its own" rule comes from.
Item {
    id: root

    // {kind, label, value?, on?, state?} from ui/js/Settings.js; kind decides what is drawn.
    property var row: ({})
    property bool current: false

    signal activated()
    // Every steppable row steps the same way, so h/l and the two chevrons fire one signal, never two.
    signal stepped(int direction)
    // The ruler's own way in: the same writer a step reaches, addressed by stop instead of direction.
    signal stopPicked(int stop)

    readonly property string kind: root.row.kind || "fact"
    readonly property bool isGroup: root.kind === "group"
    readonly property bool isHint: root.kind === "hint"
    readonly property bool isLock: root.kind === "lock"
    readonly property bool isRuler: root.kind === "ruler"
    readonly property bool hasBox: root.kind === "check" || root.kind === "master"
    // A row that names its options draws them side by side, so it is not one the chevrons walk.
    // Length and not Array.isArray: a Repeater hands the delegate a QVariantList wrapper, on which
    // isArray reads false while typeof is "object" and length is right, so the segment never drew.
    readonly property bool hasSegment: root.row.options !== undefined && root.row.options.length > 1
    readonly property bool hasSteps: root.kind === "choice" && !root.hasSegment
    // The hover lift ui/MenuRow.qml uses, so a settings row and a menu row read alike.
    readonly property real hoverOpacity: 0.08
    // The tri-state master: all six on is a check, some on is a dash, none is an empty box.
    readonly property string boxGlyph: root.kind === "master"
        ? (root.row.state === "all" ? "check" : (root.row.state === "some" ? "minus" : ""))
        : (root.row.on === true ? "check" : "")

    height: root.isHint ? hint.implicitHeight + 2 * Theme.spacing.rowPaddingY : Theme.rowHeight

    Rectangle {
        anchors.fill: parent
        visible: root.current
        color: Theme.color.foreground
        opacity: root.hoverOpacity
    }

    // A heading and a hint are the only two rows that are not a label and a control, so they draw
    // instead of the pair below rather than beside it.
    Text {
        visible: root.isGroup
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: root.row.label || ""
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        font.bold: true
        // The canvas sets every group eyebrow in small caps, the same treatment the rail's own headings take.
        font.capitalization: Font.AllUppercase
        font.letterSpacing: 1
        textFormat: Text.PlainText
    }

    Text {
        id: hint
        visible: root.isHint
        x: Theme.settings.indent
        y: Theme.spacing.rowPaddingY
        width: parent.width - Theme.settings.indent - Theme.spacing.rowPaddingX
        text: root.row.label || ""
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    // Every board row carries a mark in one column, so a section reads as a column and not a ragged
    // list; a row the boards give no mark keeps the slot open rather than closing it up. ui/MenuRow.qml
    // is the pattern, brand marks included, and the slot sets the label's indent the same way.
    Item {
        id: markSlot
        visible: !root.isGroup && !root.isHint
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.markSize
        height: Theme.markSize

        Flea.Glyph {
            anchors.fill: parent
            visible: root.row.glyph !== undefined && root.row.mark === undefined
            name: root.row.glyph !== undefined ? root.row.glyph : "file"
            color: Theme.color.muted
        }

        Flea.TailscaleMark {
            anchors.centerIn: parent
            visible: root.row.mark === "tailscale"
            iconSize: Theme.markSize
            color: Theme.color.muted
        }

        Flea.DropboxMark {
            anchors.centerIn: parent
            visible: root.row.mark === "dropbox"
            iconSize: Theme.markSize
            color: Theme.color.muted
        }
    }

    // A ruler is the row above it continued, so it takes the boards' own continuation indent
    // rather than the label column: five settings boards draw both it and a hint at that inset.
    Flea.SettingsRuler {
        visible: root.isRuler
        anchors.left: parent.left
        anchors.leftMargin: Theme.settings.indent
        anchors.right: trailing.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        height: Theme.markSize
        stops: root.row.stops !== undefined ? root.row.stops : []
        index: root.row.index !== undefined ? root.row.index : -1
        active: root.row.on === true
        onPicked: function (stop) { root.stopPicked(stop) }
    }

    Text {
        visible: !root.isGroup && !root.isHint && !root.isRuler
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: trailing.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.row.label || ""
        color: root.isLock ? Theme.color.muted : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.bodySmall
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // Row lays its children out itself, so none of them anchors vertically: each takes the mark
    // height and centres its own content inside that, which keeps one baseline across four kinds.
    Row {
        id: trailing
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacing.gap

        Flea.Glyph {
            visible: root.hasSteps
            width: root.hasSteps ? Theme.markSize : 0
            height: Theme.markSize
            name: "chevron-left"
            color: Theme.color.muted

            TapHandler {
                enabled: root.hasSteps
                onTapped: root.stepped(-1)
            }
        }

        // The master's count, a choice's name and a fact's value are all one thing: the value the
        // row currently holds, drawn on the right the way the boards draw it. A ruler has no label
        // of its own on the left, so it carries the board's "Effective 14px" reading here instead.
        Text {
            visible: root.kind === "fact" || root.hasSteps || root.kind === "master" || root.isRuler
            height: Theme.markSize
            verticalAlignment: Text.AlignVCenter
            text: root.isRuler ? (root.row.label || "") + " " + (root.row.value || "")
                               : (root.row.value || "")
            color: root.hasSteps || root.isRuler ? Theme.color.foreground : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: root.isRuler ? Theme.font.caption : Theme.font.bodySmall
            textFormat: Text.PlainText
        }

        Flea.SettingsSegment {
            visible: root.hasSegment
            options: root.hasSegment ? root.row.options : []
            value: root.row.value || ""
            onPicked: root.stepped(1)
        }

        Flea.Glyph {
            visible: root.hasSteps
            width: root.hasSteps ? Theme.markSize : 0
            height: Theme.markSize
            name: "chevron-right"
            color: Theme.color.muted

            TapHandler {
                enabled: root.hasSteps
                onTapped: root.stepped(1)
            }
        }

        // A locked row draws the lock mark where the box would be, so the section stays a complete
        // list of what the menu can contain rather than hiding the two rows nobody can switch off.
        Flea.Glyph {
            visible: root.isLock
            width: root.isLock ? Theme.markSize : 0
            height: Theme.markSize
            name: "lock"
            color: Theme.color.muted
        }

        Rectangle {
            visible: root.hasBox
            width: root.hasBox ? Theme.markSize : 0
            height: Theme.markSize
            color: "transparent"
            border.width: Theme.spacing.hairline
            border.color: root.boxGlyph.length > 0 ? Theme.color.accent : Theme.color.muted

            Flea.Glyph {
                anchors.fill: parent
                visible: root.boxGlyph.length > 0
                name: root.boxGlyph
                color: Theme.color.accent
            }
        }
    }

    HoverHandler {
        enabled: !root.isGroup && !root.isHint
        cursorShape: Qt.PointingHandCursor
    }

    // A segment and a ruler each own their own targets, so the row behind them must not also fire:
    // a tap on the option already showing would otherwise toggle the very setting it names.
    TapHandler {
        enabled: !root.isGroup && !root.isHint && !root.isLock && !root.hasSteps
                 && !root.hasSegment && !root.isRuler
        acceptedButtons: Qt.LeftButton
        onTapped: root.activated()
    }
}
