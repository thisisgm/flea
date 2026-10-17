import QtQuick
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/Markdown.js" as Markdown
import "js/Csv.js" as Csv

// Quick Look's code and text pane: the bar, then the lines coloured by the backend; a network or phone file is read here, never by the backend.
Item {
    id: root

    property bool active: false
    property string path: ""
    property int size: 0

    // Past this size the pane draws the too-large frame; remote storage sets 256 KiB, the most FileView reads whole into memory here.
    property int maxBytes: 1048576
    // True on network and phone storage, where the backend is never given the path.
    property bool truncate: false
    // The backend that colours the lines; a bare stub leaves the file's own text drawn plain.
    property var backend: null
    property string markName: "file"
    property bool numbered: false
    property bool wrap: false
    // The keyboard is on the bar's close mark; Quick Look's key handler owns the walk.
    property bool closeFocused: false

    signal closeRequested

    // The row count for a table, the line count for code and text.
    function countText() {
        if (!root.csv) return Markdown.countLine(lines.total)
        return root.body !== null ? Csv.rowCount(Csv.dataRows(root.body.records)) : ""
    }

    readonly property bool engine: root.backend !== null && typeof root.backend.askCode === "function"
    readonly property bool fromFile: root.truncate || !root.engine
    // A .csv or .tsv is drawn as a table in place of the line view.
    readonly property bool csv: Csv.delimiter(root.path) !== ""
    readonly property var body: root.csv ? csvLoader.item : lines
    // Over the gate by the listing's size, or by the backend's own count when the file grew since.
    readonly property bool tooLarge: root.size > root.maxBytes || (root.body !== null && root.body.answer === "large")
    property bool readFailed: false
    readonly property bool failed: root.readFailed || (root.body !== null && root.body.answer === "failed")
    readonly property bool binary: root.body !== null && root.body.answer === "binary"
    // For ui/Ipc.qml: the drawn lines and their box.
    readonly property var bodyItem: root.body
    readonly property bool firstScreen: root.body !== null && root.body.firstScreen
    // CodeLines' scrolling view, which the scroll bar and the wrap toggle read; its one Flickable child.
    readonly property var codeList: {
        var kids = lines.children
        for (var i = 0; i < kids.length; i++)
            if (kids[i].contentY !== undefined) return kids[i]
        return null
    }
    readonly property var list: root.csv ? (root.body !== null ? root.body.list : null) : root.codeList
    readonly property real scrollY: root.list !== null ? root.list.contentY : 0
    readonly property real viewportHeight: root.list !== null ? root.list.height : root.height
    function scrollBy(pixelDelta) {
        var view = root.list
        if (view === null) return
        var low = view.originY - view.topMargin
        var high = Math.max(low, view.originY + view.contentHeight - view.height + view.bottomMargin)
        view.cancelFlick()
        view.contentY = Math.max(low, Math.min(high, view.contentY + pixelDelta))
    }
    function shownText() { return root.fromFile ? file.text() : root.body !== null ? root.body.lines.join("\n") : "" }
    readonly property string status: {
        if (root.tooLarge) return "This file is too large to preview."
        if (root.failed) return "This file could not be read."
        return root.body !== null && root.body.answer !== "" ? "ready" : "loading"
    }

    visible: root.active

    // The line at the top of the view stays at the top while the rows change height.
    onWrapChanged: if (root.list !== null) Qt.callLater(root.keepTop, Math.max(0, root.list.indexAt(0, root.list.contentY + 1)))
    function keepTop(top) {
        if (root.list === null) return
        root.list.forceLayout()
        root.list.positionViewAtIndex(top, ListView.Beginning)
    }

    FileView {
        id: file
        path: (root.active && root.size <= root.maxBytes && root.fromFile) ? root.path : ""
        printErrors: false
        onLoadFailed: root.readFailed = true
        onPathChanged: root.readFailed = false
    }

    Flea.QuickLookBar {
        id: bar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        markName: root.markName
        name: root.path.substring(root.path.lastIndexOf("/") + 1)
        countLabel: root.body !== null && root.body.settled ? root.countText() : ""
        closeFocused: root.closeFocused
        onCloseRequested: root.closeRequested()
    }

    Flea.CodeLines {
        id: lines
        anchors.fill: parent
        anchors.topMargin: bar.height + Theme.spacing.rowPaddingX
        anchors.margins: Theme.spacing.rowPaddingX
        visible: !root.csv && !root.tooLarge && !root.failed && !root.binary
        backend: root.backend
        slot: "look"
        path: root.path
        fromText: root.fromFile
        text: root.fromFile && file.loaded ? file.text() : null
        active: root.active && root.size <= root.maxBytes && !root.csv
        numbered: root.numbered
        wrap: root.wrap
        scrolls: true
        pixelSize: Theme.font.body

        Flea.FastScrollHandler {
            flickable: root.codeList
        }

        Flea.ViewportScrollBar {
            anchors { top: parent.top; right: parent.right }
            flickable: root.codeList
        }
    }

    Loader {
        id: csvLoader
        anchors.fill: parent
        anchors.topMargin: bar.height
        active: root.active && root.csv
        visible: csvLoader.item !== null && root.csv && !root.tooLarge && !root.failed && !root.binary
        source: "CsvTable.qml"
        onLoaded: {
            item.look = true
            item.slot = "look"
            item.pixelSize = Qt.binding(function () { return Theme.font.caption })
            item.backend = Qt.binding(function () { return root.backend })
            item.path = Qt.binding(function () { return root.path })
            item.fromText = Qt.binding(function () { return root.fromFile })
            item.text = Qt.binding(function () { return root.fromFile && file.loaded ? file.text() : null })
            item.active = Qt.binding(function () { return root.active && root.size <= root.maxBytes })
        }
    }

    Flea.CodeRefusal {
        anchors.fill: parent
        topInset: bar.height
        binary: root.binary
        tooLarge: root.tooLarge
        failed: root.failed
        markName: root.markName
    }
}
