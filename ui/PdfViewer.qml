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
    // Containers Tier A: a keyboard walk says where it is by brightness, so the control it is on keeps the foreground and the rest of the strip dims.
    readonly property color controlRest: root.activeFocus && root.pdfControlIndex >= 0
        ? Theme.color.muted : Theme.color.foreground
    // The page count signal arrives before the control bindings settle.
    function focusInitialControl() {
        if (root.activeFocus && root.pageCount > 0 && root.pdfControlIndex < 0)
            PreviewKeys.pdfAction("focusNext", root)
    }
    onActiveFocusChanged: Qt.callLater(root.focusInitialControl)
    onPageCountChanged: Qt.callLater(root.focusInitialControl)
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
    signal nextFileRequested()

    // A new document is a new subject, so it opens fitted however the last one was left.
    onPathChanged: { root.zoom = root.minZoom; root.pdfControlIndex = -1 }
    function turn(delta) {
        if (delta > 0 && root.pageCount > 0 && root.page >= root.pageCount - 1) {
            root.nextFileRequested()
            return
        }
        pdf.turn(delta)
    }
    function turnPage(delta) { root.turn(delta) }
    function scrollPage(delta) {
        pageFlick.contentY = Math.max(0, Math.min(pageFlick.contentHeight - pageFlick.height,
            pageFlick.contentY + delta * Theme.rowHeight))
    }

    function zoomBy(steps) {
        root.zoom = Math.max(root.minZoom, Math.min(root.maxZoom, root.zoom + steps * root.zoomStep))
    }

    function commitPinch() {
        root.zoom = Math.max(root.minZoom, Math.min(root.maxZoom, root.zoom * pdfPinch.persistentScale))
        pdfPinch.persistentScale = 1
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
            width: Math.min(implicitWidth, Math.max(0, tools.x - x - pager.implicitWidth - 2 * Theme.spacing.gap))
            text: root.path.substring(root.path.lastIndexOf("/") + 1)
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }

        // MediaPdf rule 3: the count and the turn are one control, so they share the header the
        // filename is already on, and rule 4 leaves a one-page document with the count alone, where
        // two chevrons dead for the life of the document would be worse than absent.
        Row {
            id: pager
            anchors.left: nameText.right
            anchors.leftMargin: Theme.spacing.gap
            anchors.verticalCenter: parent.verticalCenter
            visible: root.pageCount > 0
            spacing: Theme.spacing.gap

            Flea.ChromeButton {
                gesturePolicy: TapHandler.ReleaseWithinBounds
                id: previous
                glyph: "chevron-left"
                accessName: "Previous page"
                visible: root.pageCount > 1
                width: visible ? implicitWidth : 0
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 0
                restingColor: root.controlRest
                enabled: root.page > 0
                onActivated: root.turn(-1)
            }

            Text {
                id: counter
                height: previous.implicitHeight
                verticalAlignment: Text.AlignVCenter
                text: (pdf.drawnPage + 1) + " / " + root.pageCount
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            Flea.ChromeButton {
                gesturePolicy: TapHandler.ReleaseWithinBounds
                id: next
                glyph: "chevron-right"
                accessName: "Next page"
                visible: root.pageCount > 1
                width: visible ? implicitWidth : 0
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 1
                restingColor: root.controlRest
                enabled: root.page + 1 < root.pageCount
                onActivated: root.turn(1)
            }
        }

        Row {
            id: tools
            anchors.right: parent.right
            anchors.rightMargin: Theme.spacing.rowPaddingX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.spacing.gap

            Flea.ChromeButton {
                gesturePolicy: TapHandler.ReleaseWithinBounds
                id: zoomOut
                glyph: "minus"
                accessName: "Zoom out"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 2
                restingColor: root.controlRest
                enabled: root.pageCount > 0 && root.zoom > root.minZoom
                onActivated: root.zoomBy(-1)
            }

            Flea.ChromeButton {
                gesturePolicy: TapHandler.ReleaseWithinBounds
                id: zoomIn
                glyph: "plus"
                accessName: "Zoom in"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 3
                restingColor: root.controlRest
                enabled: root.pageCount > 0 && root.zoom < root.maxZoom
                onActivated: root.zoomBy(1)
            }

            Flea.ChromeButton {
                gesturePolicy: TapHandler.ReleaseWithinBounds
                id: expand
                glyph: "maximize"
                accessName: "Expand"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 4
                restingColor: root.controlRest
                active: root.expanded
                onActivated: root.toggleExpand()
            }

            Flea.ChromeButton {
                gesturePolicy: TapHandler.ReleaseWithinBounds
                id: close
                glyph: "x"
                accessName: "Close"
                keyboardFocused: root.activeFocus && root.pdfControlIndex === 5
                restingColor: root.controlRest
                onActivated: root.closed()
            }
        }
    }

    Flickable {
        id: pageFlick
        anchors.top: topBar.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        // At zoom 1 the content is exactly the viewport, so a fitted page cannot be dragged at all.
        contentWidth: width * root.zoom * pdfPinch.persistentScale
        contentHeight: height * root.zoom * pdfPinch.persistentScale
        boundsBehavior: Flickable.StopAtBounds

        Flea.FastScrollHandler {
            parent: pageFlick
            flickable: pageFlick
        }

        Flea.ViewportScrollBars {
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
                viewport: pageFlick
                path: root.path
                active: root.active
            }
        }
    }

    PinchHandler {
        id: pdfPinch
        enabled: root.pageCount > 0
        target: null
        minimumScale: 0.5
        maximumScale: 4
        minimumRotation: 0
        maximumRotation: 0
        persistentScale: 1
        onActiveChanged: if (!active) root.commitPinch()
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
}
