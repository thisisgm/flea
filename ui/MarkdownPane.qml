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
    // The backend that colours Source and the fences, and the wrap Quick Look's w keeps for Source.
    property var backend: null
    property bool wrap: false

    // The keyboard is on the close mark; Quick Look's key handler owns the walk, as the PDF viewer's controls do.
    property bool closeFocused: false

    signal closeRequested

    readonly property var bodyItem: doc.bodyItem
    readonly property var tableWheel: doc.tableWheel
    function tableScroller() { return doc.tableScroller() }
    readonly property real viewportHeight: doc.viewportHeight
    function scrollBy(pixelDelta) { doc.scrollBy(pixelDelta) }
    readonly property real scrollY: doc.scrollY
    function endGap() { return doc.endGap() }
    function closeState() { return bar.closeState() }
    readonly property string rawText: doc.rawText
    readonly property string status: doc.status
    readonly property string lineLabel: doc.lineLabel
    // The view the body draws, which Quick Look's IPC reads rather than the property that asks for it.
    readonly property string shownView: doc.shownView
    // Quick Look's seam reads figures through this pane, so it forwards the document's blocks.
    readonly property var blockList: doc.blockList
    function figureInfo(i) { return doc.figureInfo(i) }
    function blockItem(i) { return doc.blockItem(i) }
    readonly property bool contentReady: doc.contentReady
    readonly property bool firstScreen: doc.firstScreen

    // The render suite reads the bar's order off these rects; the pane adds what only it knows.
    function barGeometry() {
        var g = bar.geometry()
        g.ready = root.contentReady
        return g
    }

    Flea.QuickLookBar {
        id: bar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        markName: "markdown"
        name: root.path.substring(root.path.lastIndexOf("/") + 1)
        countLabel: root.contentReady ? root.lineLabel : ""
        closeFocused: root.closeFocused
        onCloseRequested: root.closeRequested()
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
        backend: root.backend
        slot: "look"
        wrap: root.wrap
        shareParse: true
        // The page is the chrome surface, so code sits on the window colour (md_rendered code_bg).
        codeSurface: Theme.color.background
    }
}
