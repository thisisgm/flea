import QtQuick
import "." as Flea

// The canvas's PdfViewer: the Quick Look's own PDF surface. Hairline chrome above and below, and
// between them the page, which is the only light thing in the app. The document itself stays in
// PreviewPdf.qml, so this file is chrome, a zoom and a pan, and nothing else.
Item {
    id: root

    property string path: ""
    property bool active: false
    // Expand fills the window; the overlay that hosts this reads the flag and drops its own inset.
    property bool expanded: false

    readonly property int page: pdf.page
    readonly property int pageCount: pdf.pageCount
    readonly property bool failed: pdf.failed

    // The canvas draws no scale readout, so the ladder is the whole zoom contract: one step a press,
    // and a bottom rung that always fits the frame, which is what makes the pan below reachable.
    readonly property real minZoom: 1
    readonly property real maxZoom: 4
    readonly property real zoomStep: 0.25
    property real zoom: root.minZoom

    signal closed()

    // A new document is a new subject, so it opens fitted however the last one was left.
    onPathChanged: root.zoom = root.minZoom
    function turn(delta) { pdf.turn(delta) }

    function zoomBy(steps) {
        root.zoom = Math.max(root.minZoom, Math.min(root.maxZoom, root.zoom + steps * root.zoomStep))
    }

    function toggleExpand() { root.expanded = !root.expanded }

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
            color: Theme.color.muted
        }

        // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
        Text {
            id: nameText
            anchors.left: kindMark.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, counter.x - x - Theme.spacing.gap)
            text: root.path.substring(root.path.lastIndexOf("/") + 1)
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }

        Text {
            id: counter
            anchors.right: tools.left
            anchors.rightMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            visible: root.pageCount > 0
            text: (root.page + 1) + " / " + root.pageCount
            color: Theme.color.muted
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
                glyph: "minus"
                enabled: root.zoom > root.minZoom
                onActivated: root.zoomBy(-1)
            }

            Flea.ChromeButton {
                glyph: "plus"
                enabled: root.zoom < root.maxZoom
                onActivated: root.zoomBy(1)
            }

            Flea.ChromeButton {
                glyph: "maximize"
                active: root.expanded
                onActivated: root.toggleExpand()
            }

            Flea.ChromeButton {
                glyph: "x"
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
        height: Theme.chromeHeight
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
            spacing: Theme.spacing.gap
            visible: root.pageCount > 0

            Flea.ChromeButton {
                glyph: "chevron-left"
                enabled: root.page > 0
                onActivated: root.turn(-1)
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "page " + (root.page + 1)
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Flea.ChromeButton {
                glyph: "chevron-right"
                enabled: root.page + 1 < root.pageCount
                onActivated: root.turn(1)
            }
        }
    }
}
