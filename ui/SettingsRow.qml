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
    property bool firstRow: false

    signal activated()
    signal pointerMoved()
    signal favouriteMoved(int to)
    // Every steppable row steps the same way, so h/l and the two chevrons fire one signal, never two.
    signal stepped(int direction)
    // The ruler's own way in: the same writer a step reaches, addressed by stop instead of direction.
    signal stopPicked(int stop)

    readonly property string kind: root.row.kind || "fact"
    readonly property bool isGroup: root.kind === "group"
    readonly property bool firstGroup: root.isGroup && root.firstRow
    // Settings headings scale their resolved 12/15/4/8 insets once from the boards' bodySmall 13 anchor.
    readonly property real groupScale: Theme.font.bodySmall / 13
    readonly property real groupGap: root.firstGroup ? 0 : Math.round(8 * root.groupScale)
    readonly property real groupPaddingTop: Math.round((root.firstGroup ? 12 : 15) * root.groupScale)
    readonly property real groupPaddingBottom: Math.round(4 * root.groupScale)
    readonly property real groupLineHeight: Theme.font.caption * 1.6
    readonly property bool isHint: root.kind === "hint"
    readonly property bool isFooter: root.isHint && root.row.footer === true
    readonly property bool isFavourite: root.kind === "favourite" || root.kind === "favouriteActions"
    readonly property Item favouriteItem: favourite
    readonly property bool isHero: root.kind === "hero"
    readonly property bool isKeyPreview: root.kind === "keyPreview"
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

    height: root.isGroup ? groupLabel.y + groupLabel.height + root.groupPaddingBottom
            : root.isFavourite ? favourite.implicitHeight : root.isHero ? hero.implicitHeight + 4 * Theme.spacing.rowPaddingY
            : root.isKeyPreview ? keyPreview.implicitHeight + 2 * Theme.spacing.rowPaddingY
            : root.isHint ? hint.y + hint.implicitHeight + (root.isFooter
                ? Theme.settings.railPaddingY + 2 * Theme.spacing.hairline : Theme.spacing.rowPaddingY) : Theme.rowHeight

    Grid {
        id: keyPreview
        visible: root.isKeyPreview
        x: Theme.spacing.rowPaddingX
        y: Theme.spacing.rowPaddingY
        width: parent.width - 2 * Theme.spacing.rowPaddingX
        columns: 2
        spacing: Theme.spacing.gap
        Repeater {
            model: root.isKeyPreview ? root.row.items : []
            delegate: Item {
                id: binding
                required property var modelData
                width: (keyPreview.width - keyPreview.spacing) / 2
                height: Theme.hitMin
                Rectangle {
                    id: cap
                    width: Math.min(capText.implicitWidth + Theme.spacing.gap, parent.width)
                    height: parent.height
                    color: "transparent"
                    border.width: Theme.spacing.hairline
                    border.color: Theme.color.muted
                    Text {
                        id: capText
                        anchors.fill: parent
                        anchors.margins: Theme.spacing.hairline
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        text: binding.modelData.keys
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        color: Theme.color.foreground
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                    }
                }
                Flea.Glyph {
                    id: bindingMark
                    x: cap.width + Theme.spacing.gap
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !!binding.modelData.glyph
                    width: visible ? Theme.chromeMarkSize : 0
                    height: width
                    name: binding.modelData.glyph || "file"
                    color: Theme.color.muted
                }
                Text {
                    x: bindingMark.x + bindingMark.width + (bindingMark.visible ? Theme.spacing.gap : 0)
                    width: Math.max(0, parent.width - x)
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                    text: binding.modelData.label
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    color: Theme.color.foreground
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
            }
        }
    }

    Flea.SettingsFavourite {
        id: favourite
        anchors.fill: parent
        visible: root.isFavourite
        row: root.row
        onActivated: root.activated()
        onActionPicked: function (action) { root.stopPicked(action) }
        onMoved: function (to) { root.favouriteMoved(to) }
    }

    Column {
        id: hero
        visible: root.isHero
        anchors.centerIn: parent
        spacing: Theme.spacing.gap
        Flea.FleaMark {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.markSize * 2
            height: width
            color: Theme.color.accent
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.row.label || ""
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            font.bold: true
            color: Theme.color.foreground
            textFormat: Text.PlainText
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.row.value || ""
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            color: Theme.color.foreground
            textFormat: Text.PlainText
        }
    }

    Rectangle {
        anchors.fill: parent
        visible: root.current
        color: Theme.color.foreground
        opacity: root.hoverOpacity
    }

    Rectangle {
        anchors.top: parent.top
        anchors.topMargin: root.isFooter ? Theme.settings.railPaddingY : root.groupGap
        width: parent.width
        height: Theme.spacing.hairline
        visible: root.isFooter || root.isGroup && !root.firstGroup
        color: Theme.color.muted
        opacity: 0.4
    }

    // A heading and a hint are the only two rows that are not a label and a control, so they draw
    // instead of the pair below rather than beside it.
    Text {
        id: groupLabel
        visible: root.isGroup
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        y: root.groupGap + (root.firstGroup ? 0 : Theme.spacing.hairline) + root.groupPaddingTop
        height: root.groupLineHeight + topPadding
        topPadding: Math.ceil(font.pixelSize * 0.15)
        verticalAlignment: Text.AlignVCenter
        text: root.row.label || ""
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        font.bold: true
        // The canvas sets every group eyebrow in small caps, the same treatment the rail's own headings take.
        font.capitalization: Font.AllUppercase
        font.letterSpacing: Theme.font.caption * 0.14
        textFormat: Text.PlainText
    }

    Text {
        id: hint
        visible: root.isHint
        x: root.isFooter ? Theme.spacing.rowPaddingX : Theme.settings.indent
        y: root.isFooter ? 2 * Theme.settings.railPaddingY + Theme.spacing.hairline : Theme.spacing.rowPaddingY
        width: parent.width - x - Theme.spacing.rowPaddingX
        text: root.row.label || ""
        color: root.row.role === "error" ? Theme.color.error
             : root.row.role === "accent" ? Theme.color.accent
             : root.row.role === "foreground" ? Theme.color.foreground : Theme.color.muted
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
        visible: !root.isGroup && !root.isHint && !root.isHero && !root.isFavourite && !root.isKeyPreview
        anchors.left: parent.left
        anchors.leftMargin: root.row.indented === true ? Theme.settings.indent - Theme.spacing.rowPaddingX : Theme.spacing.rowPaddingX
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
        id: rowLabel
        visible: !root.isGroup && !root.isHint && !root.isHero && !root.isFavourite && !root.isRuler && !root.isKeyPreview
        anchors.left: markSlot.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: caption.visible ? caption.left : trailing.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.row.label || ""
        color: root.isLock ? Theme.color.muted : Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    Text {
        id: caption
        visible: !!root.row.caption && width > 0
        anchors.right: trailing.left
        anchors.rightMargin: Theme.spacing.gap + Theme.settings.railPaddingY
        anchors.verticalCenter: parent.verticalCenter
        text: root.row.caption || ""
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        // Preserve the label and real controls first; only the explanatory caption elides in the remaining space.
        width: Math.min(implicitWidth, Math.max(0, trailing.x - anchors.rightMargin
            - Theme.spacing.gap - rowLabel.x - rowLabel.implicitWidth))
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

        // The master's count, a choice's name and a fact's value are all one thing: the value the
        // row currently holds, drawn on the right the way the boards draw it. A ruler has no label
        // of its own on the left, so it carries the board's "Effective 14px" reading here instead.
        Text {
            visible: root.kind === "fact" || root.kind === "action" || root.hasSteps || root.kind === "master" || root.isRuler
            height: Theme.markSize
            verticalAlignment: Text.AlignVCenter
            text: root.isRuler ? (root.row.label || "") + " " + (root.row.value || "")
                               : (root.row.value || "")
            color: root.hasSteps || root.isRuler ? Theme.color.foreground : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: root.isRuler ? Theme.font.caption : Theme.font.body
            textFormat: Text.PlainText
            width: Math.min(implicitWidth, root.width * 0.56)
            elide: Text.ElideRight
        }

        Flea.SettingsSegment {
            visible: root.hasSegment
            options: root.hasSegment ? root.row.options : []
            value: root.row.value || ""
            glyphs: root.row.id === "view" ? root.row.values : []
            onPicked: function (i) { root.stopPicked(i) }
        }

        Flea.Glyph {
            visible: root.hasSteps || root.kind === "action"
            width: visible ? Theme.markSize : 0
            height: Theme.markSize
            name: "chevron-right"
            color: Theme.color.muted

            TapHandler {
                enabled: root.hasSteps
                gesturePolicy: TapHandler.ReleaseWithinBounds
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
            // The boards frame a 14px interior at bodySmall 13 with two 2px borders.
            width: root.hasBox ? height : 0
            height: Math.round(14 * Theme.font.bodySmall / 13) + 2 * border.width
            color: "transparent"
            border.width: 2 * Theme.spacing.hairline
            border.color: root.boxGlyph.length > 0 ? Theme.color.accent : Theme.color.muted

            Flea.Glyph {
                anchors.centerIn: parent
                width: Theme.font.bodySmall * 10 / 13
                height: width
                strokeWidth: 3
                visible: root.boxGlyph.length > 0
                name: root.boxGlyph
                color: Theme.color.accent
            }
        }
    }

    HoverHandler {
        id: pointer
        enabled: root.hasBox || root.kind === "choice" || root.kind === "action" || root.isFavourite
        property bool armed: false
        property point restingAt
        onHoveredChanged: pointer.armed = false
        onPointChanged: {
            if (!pointer.hovered) return
            if (!pointer.armed) {
                pointer.armed = true
                pointer.restingAt = pointer.point.scenePosition
                return
            }
            // Scrolling moves the row beneath a resting pointer; only scene-space motion selects it.
            if (pointer.point.scenePosition.x !== pointer.restingAt.x || pointer.point.scenePosition.y !== pointer.restingAt.y)
                root.pointerMoved()
            pointer.restingAt = pointer.point.scenePosition
        }
        cursorShape: Qt.PointingHandCursor
    }

    // A segment and a ruler each own their own targets, so the row behind them must not also fire:
    // a tap on the option already showing would otherwise toggle the very setting it names.
    TapHandler {
        enabled: (root.hasBox || root.kind === "action") && !root.isLock && !root.hasSteps
                 && !root.hasSegment && !root.isRuler
        acceptedButtons: Qt.LeftButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: root.activated()
    }
}
