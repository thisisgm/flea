import QtQuick
import "." as Flea

// The Markdown bar and pane load only for a Markdown file.
Item {
    id: root

    property bool active: false
    property string path: ""
    property int size: 0
    property string view: "rendered"
    property int maxBytes: 1048576
    property bool truncate: false
    property string blockPath: ""

    // The keyboard is on the close mark; Quick Look's key handler owns the walk, as the PDF viewer's controls do.
    property bool closeFocused: false

    signal closeRequested

    readonly property var bodyItem: doc.bodyItem
    readonly property var tableWheel: doc.tableWheel
    function tableScroller() { return doc.tableScroller() }
    readonly property real viewportHeight: doc.height
    function scrollBy(pixelDelta) { doc.scrollBy(pixelDelta) }
    readonly property real scrollY: doc.scrollY
    function endGap() { return doc.endGap() }
    function closeState() { var c = barClose.mapToItem(null, barClose.width / 2, barClose.height / 2); return { hovered: barClose.hovered, pressed: barClose.pressed, focused: barClose.keyboardFocused, centre: Math.round(c.x) + " " + Math.round(c.y) } }
    readonly property string rawText: doc.rawText
    readonly property string status: doc.status
    readonly property string lineLabel: doc.lineLabel
    // The view the body draws, which Quick Look's IPC reads rather than the property that asks for it.
    readonly property string shownView: doc.view
    // Quick Look's seam reads figures through this pane, so it forwards the document's blocks.
    readonly property var blockList: doc.blockList
    function figureInfo(i) { return doc.figureInfo(i) }
    function blockItem(i) { return doc.blockItem(i) }
    readonly property bool contentReady: doc.contentReady
    readonly property bool firstScreen: doc.firstScreen
    readonly property bool loading: doc.loading
    readonly property bool blank: doc.blank
    // The page is the chrome surface, so code sits on the window colour (md_rendered code_bg).
    readonly property color codeSurface: doc.codeSurface

    // The render suite reads the bar's order off these rects, the way PdfViewer.buttonFor opens its buttons.
    function barGeometry() {
        return { mark: barMark, markName: barMark.name, name: barName, nameEnd: barName.x + Math.min(barName.width, barName.implicitWidth),
            lines: barLines, close: barClose, bar: bar, height: bar.height, ready: root.contentReady }
    }

    // No fill of its own: Quick Look's surface is already the chrome colour and rounds the corners this bar sits under.
    Item {
        id: bar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.chromeHeight
        readonly property real ruleOpacity: 0.12

        Rectangle {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: Theme.spacing.hairline
            color: Theme.color.foreground
            opacity: bar.ruleOpacity
        }

        // The Markdown mark of the LanguageMarks set, in foreground like the PDF viewer's kind mark.
        Flea.Glyph {
            id: barMark
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.chromeMarkSize
            height: Theme.chromeMarkSize
            name: "markdown"
            color: Theme.color.foreground
        }

        // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
        Text {
            id: barName
            anchors.left: barMark.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.max(0, barClose.x - x - 2 * Theme.spacing.gap
                - (root.contentReady ? barLines.implicitWidth : 0)))
            text: root.path.substring(root.path.lastIndexOf("/") + 1)
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
            text: root.lineLabel
            visible: root.contentReady
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
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

    Flea.PreviewMarkdown {
        id: doc
        anchors.fill: parent
        anchors.topMargin: bar.height
        active: root.active
        view: root.view
        path: root.path
        size: root.size
        maxBytes: root.maxBytes
        truncate: root.truncate
        blockPath: root.blockPath
        shareParse: true
        codeSurface: Theme.color.background
    }
}
