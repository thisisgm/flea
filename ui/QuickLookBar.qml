import QtQuick
import "." as Flea
import "js/LangMarks.js" as LangMarks

// Quick Look's strip: language mark, name, line count and close; no fill, the chrome surface rounds its corners.
Item {
    id: root

    property string markName: "file"
    property string name: ""
    readonly property string langSlug: ViewState.langMarks ? LangMarks.slugForFile(root.name) : ""
    readonly property bool thumbDrawn: false
    property var langLoader: null
    onLangSlugChanged: LangMarks.syncMark(root, barMark, Qt.resolvedUrl("LangMarkLoader.qml"), Component.Ready)
    // Empty hides the count; it hides too once the name has given up all its room.
    property string countLabel: ""
    // The keyboard is on the close mark; Quick Look's key handler owns the walk, as the PDF viewer's controls do.
    property bool closeFocused: false

    // Optional text actions are built only by text and Markdown hosts.
    property Component textActions: null

    signal closeRequested

    height: Theme.chromeHeight
    readonly property real ruleOpacity: 0.12
    // Where the right-hand group starts: the name and the count give way to it, it never gives way to them.
    readonly property real trail: actions.item !== null ? actions.x : barClose.x
    readonly property bool countShown: root.countLabel.length > 0
        && barName.x + 2 * Theme.spacing.gap + barLines.implicitWidth <= root.trail

    // The render suite reads the bar's order off these rects, the way PdfViewer.buttonFor opens its buttons.
    function geometry() {
        return { mark: barMark, language: root.langLoader ? root.langLoader.item : null, markName: barMark.name, name: barName, nameEnd: barName.x + Math.min(barName.width, barName.implicitWidth),
            lines: barLines, countShown: root.countShown, close: barClose, bar: root, height: root.height,
            segment: null, rendered: null, source: null, trail: root.trail,
            select: actions.item !== null ? actions.item.selectControl : null, copy: actions.item !== null ? actions.item.copyControl : null }
    }

    function closeState() { var c = barClose.mapToItem(null, barClose.width / 2, barClose.height / 2); return { hovered: barClose.hovered, pressed: barClose.pressed, focused: barClose.keyboardFocused, centre: Math.round(c.x) + " " + Math.round(c.y) } }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: root.ruleOpacity
    }

    // The kind's mark of the LanguageMarks set, in foreground and centred above the rule like the PDF viewer's kind mark.
    Flea.Glyph {
        id: barMark
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        y: Math.round((parent.height - Theme.spacing.hairline - height) / 2)
        width: Theme.chromeMarkSize
        height: Theme.chromeMarkSize
        visible: root.langSlug === ""
        name: root.markName
        color: Theme.color.foreground
    }

    // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
    Text {
        id: barName
        anchors.left: barMark.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, Math.max(0, root.trail - x - 2 * Theme.spacing.gap
            - (root.countShown ? barLines.implicitWidth : 0)))
        text: root.name
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
    }

    Text {
        id: barLines
        anchors.left: barName.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: root.countLabel
        visible: root.countShown
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Loader {
        id: actions
        anchors.right: barClose.left
        anchors.verticalCenter: parent.verticalCenter
        sourceComponent: root.textActions
        width: item !== null ? item.implicitWidth : 0
        height: root.height
    }

    Flea.ChromeButton {
        id: barClose
        gesturePolicy: TapHandler.ReleaseWithinBounds
        ruleRows: Theme.spacing.hairline
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        glyph: "x"
        keyboardFocused: root.closeFocused
        accessName: "Close"
        onActivated: root.closeRequested()
    }
}
