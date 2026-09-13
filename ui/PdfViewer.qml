import QtQuick
import "." as Flea
import "js/Keymap.js" as Keymap
import "js/PreviewKeys.js" as PreviewKeys

// The canvas's PdfViewer: the Quick Look's own PDF surface. Hairline chrome above and below, and
// between them the page, which is the only light thing in the app. The document itself stays in
// PreviewPdf.qml, so this file is chrome, a zoom and a pan, and nothing else.
Item {
    id: root

    property string path: ""
    property bool active: false
    // Expand fills the window; the overlay that hosts this reads the flag and drops its own inset.
    property bool expanded: false
    property int pdfControlIndex: -1
    readonly property var pdfControls: [previous, next, zoomOut, zoomIn, expand, close]
    onActiveFocusChanged: if (root.activeFocus && root.pageCount > 0 && root.pdfControlIndex < 0)
        PreviewKeys.pdfAction("focusNext", root)
    onPageCountChanged: if (root.activeFocus && root.pageCount > 0 && root.pdfControlIndex < 0)
        PreviewKeys.pdfAction("focusNext", root)
    Keys.onPressed: function(event) {
        var action = Keymap.lookup(event.key, event.text, event.modifiers, "pdf")
        if (action === "escape" || action === "focusPreview") root.closed()
        else PreviewKeys.pdfAction(action, root)
        event.accepted = true
    }

    readonly property int page: pdf.page
    readonly property int pageCount: pdf.pageCount
    readonly property bool failed: pdf.failed
    readonly property real pdfScrollY: pageFlick.contentY

    // The canvas draws no scale readout, so the ladder is the whole zoom contract: one step a press,
    // and a bottom rung that always fits the frame, which is what makes the pan below reachable.
    readonly property real minZoom: 1
    readonly property real maxZoom: 4
    readonly property real zoomStep: 0.25
    property real zoom: root.minZoom

    signal closed()

    // A new document is a new subject, so it opens fitted however the last one was left.
    onPathChanged: { root.zoom = root.minZoom; root.pdfControlIndex = -1 }
    function turn(delta) { pdf.turn(delta) }
    function turnPage(delta) { root.turn(delta) }
    function scrollPage(delta) {
        pageFlick.contentY = Math.max(0, Math.min(pageFlick.contentHeight - pageFlick.height,
            pageFlick.contentY + delta * Theme.rowHeight))
    }

    function zoomBy(steps) {
        root.zoom = Math.max(root.minZoom, Math.min(root.maxZoom, root.zoom + steps * root.zoomStep))
    }

    function toggleExpand() { root.expanded = !root.expanded }
    function expandFrom(page, zoom) {
        pdf.page = Math.max(0, pdf.pageCount > 0 ? Math.min(pdf.pageCount - 1, page) : page)
        root.zoom = Math.max(root.minZoom, Math.min(root.maxZoom, zoom))
        root.expanded = true
    }

    // A test drives these by coordinate, the same seam ChromeBar.buttonFor already opens.
    function buttonFor(glyph) {
        var groups = [tools, pager]
        for (var g = 0; g < groups.length; g++) {
            var kids = groups[g].children
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].glyph === glyph)
                    return kids[i]
            }
        }
        return null
    }

    Rectangle {
        id: topBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.chromeHeight
        color: Theme.color.surface

        Rectangle {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: Theme.spacing.hairline
            color: Theme.color.foreground
            opacity: 0.12
        }

        Flea.Glyph {
            id: kindMark
            anchors.left: parent.left
            anchors.leftMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            // The chrome mark token, so the leading mark matches the buttons at the other end.
            width: Theme.chromeMarkSize
            height: Theme.chromeMarkSize
            name: "file-text"
            color: Theme.color.foreground
        }

        // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
        Text {
            id: nameText
            anchors.left: kindMark.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.max(0, tools.x - x - counter.implicitWidth - 2 * Theme.spacing.gap))
            text: root.path.substring(root.path.lastIndexOf("/") + 1)
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }

        Text {
            id: counter
            anchors.left: nameText.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            visible: root.pageCount > 0
            text: (root.page + 1) + " / " + root.pageCount
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }

        Row {
            id: tools
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            Flea.ChromeButton {
                id: zoomOut
                glyph: "minus"
                accessName: "Zoom out"
                restingColor: Theme.color.foreground
                disabledOpacity: 0.55
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 2
                enabled: root.pageCount > 0 && root.zoom > root.minZoom
                onActivated: root.zoomBy(-1)
            }

            Flea.ChromeButton {
                id: zoomIn
                glyph: "plus"
                accessName: "Zoom in"
                restingColor: Theme.color.foreground
                disabledOpacity: 0.55
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 3
                enabled: root.pageCount > 0 && root.zoom < root.maxZoom
                onActivated: root.zoomBy(1)
            }

            Flea.ChromeButton {
                id: expand
                glyph: "maximize"
                accessName: "Expand"
                restingColor: Theme.color.foreground
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 4
                active: root.expanded
                onActivated: root.toggleExpand()
            }

            Flea.ChromeButton {
                id: close
                glyph: "x"
                accessName: "Close"
                restingColor: Theme.color.foreground
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 5
                onActivated: root.closed()
            }
        }
    }

    Flickable {
        id: pageFlick
        anchors.top: topBar.bottom
        anchors.bottom: bottomBar.top
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        // At zoom 1 the content is exactly the viewport, so a fitted page cannot be dragged at all.
        contentWidth: width * root.zoom
        contentHeight: height * root.zoom
        boundsBehavior: Flickable.StopAtBounds

        Flea.FastScrollHandler {
            parent: pageFlick
            flickable: pageFlick
        }

        Rectangle {
            width: pageFlick.contentWidth
            height: pageFlick.contentHeight
            color: Theme.color.background

            Flea.PreviewPdf {
                id: pdf
                anchors.fill: parent
                anchors.margins: 2 * Theme.spacing.rowPaddingX
                path: root.path
                active: root.active
            }
        }
    }

    // The one sentence an unreadable document gets; without it the surface is simply blank.
    Column {
        anchors.centerIn: pageFlick
        width: pageFlick.width - 2 * Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap
        visible: root.failed

        Flea.Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.iconSize
            height: Theme.iconSize
            name: "alert"
            color: Theme.color.error
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "This file could not be read."
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }

    Rectangle {
        id: bottomBar
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.hitMin + 2 * Theme.space(8) + Theme.spacing.hairline
        color: Theme.color.surface

        Rectangle {
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: Theme.spacing.hairline
            color: Theme.color.foreground
            opacity: 0.12
        }

        Row {
            id: pager
            anchors.centerIn: parent
            spacing: Theme.space(20)
            visible: root.pageCount > 0

            Flea.ChromeButton {
                id: previous
                glyph: "chevron-left"
                accessName: "Previous page"
                implicitHeight: Theme.hitMin
                restingColor: Theme.color.foreground
                disabledOpacity: 0.55
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 0
                enabled: root.page > 0
                onActivated: root.turn(-1)
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "page " + (root.page + 1)
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Flea.ChromeButton {
                id: next
                glyph: "chevron-right"
                accessName: "Next page"
                implicitHeight: Theme.hitMin
                restingColor: Theme.color.foreground
                disabledOpacity: 0.55
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 1
                enabled: root.page + 1 < root.pageCount
                onActivated: root.turn(1)
            }
        }
    }
}
