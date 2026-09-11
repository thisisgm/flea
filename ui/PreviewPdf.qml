import QtQuick
import QtQuick.Pdf
import qs.Commons
import "js/Format.js" as Format

// A page of a PDF, rendered into the preview column's frame. QtPdf ships inside the already
// installed qt6-webengine and needs no package of its own; proven to import and render under
// Quickshell itself on this box, not only under qml6.
Item {
    id: root

    property string path: ""
    property bool active: false
    // Which page the frame is showing, zero-based, always inside the document it belongs to.
    property int page: 0

    // What the facts table shows as Pages; 0 until the document is ready, or if it never becomes so.
    readonly property int pageCount: doc.status === PdfDocument.Ready ? doc.pageCount : 0
    readonly property bool failed: doc.status === PdfDocument.Error

    // The page's proportions come from the document, never from the rendered image. Reading the
    // image's implicit size closed a loop through the width binding below: sourceSize changed the
    // implicit size, which re-evaluated width, which re-set sourceSize. Qt broke that binding, and a
    // broken width binding is a page that never resizes when the frame or the zoom changes.
    readonly property real pageAspect: {
        if (doc.status !== PdfDocument.Ready || root.pageCount <= 0)
            return 1
        var box = doc.pagePointSize(Math.min(root.page, root.pageCount - 1))
        return box.height > 0 ? box.width / box.height : 1
    }

    // A new document starts at its first page, whatever page the last one was left on.
    onPathChanged: root.page = 0
    onPageCountChanged: if (root.pageCount > 0) root.page = Math.min(root.page, root.pageCount - 1)

    function turn(delta) {
        if (root.pageCount <= 0)
            return
        root.page = Math.max(0, Math.min(root.pageCount - 1, root.page + delta))
    }

    PdfDocument {
        id: doc
        // Format.fileUri, not a concatenation: a path can carry a # or a ? and either one truncates
        // a hand-built URI at that character. A document is only opened while the column shows one.
        source: root.active && root.path.length > 0 ? Format.fileUri(root.path) : ""
    }

    // The page's own paper under the raster: on this box a rendered page can arrive with text drawn
    // and no background, and the dark frame showed through it. Paper is the document's, not a theme
    // role, so the colour is a constant; visible only with the page, so a loading document draws none.
    Rectangle {
        anchors.centerIn: parent
        visible: page.visible
        width: page.width
        height: page.height
        color: "#ffffff"
    }

    // The page is the only light surface in the app, which is exactly what the canvas draws.
    PdfPageImage {
        id: page
        anchors.centerIn: parent
        visible: doc.status === PdfDocument.Ready
        document: doc
        currentFrame: Math.min(root.page, Math.max(0, root.pageCount - 1))
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        // Fit inside the frame without ever upscaling past the page's own resolution.
        width: Math.min(parent.width, parent.height * root.pageAspect)
        height: Math.min(parent.height, parent.width / root.pageAspect)

        // Rasterised at the size it is drawn at, so a zoomed page sharpens instead of scaling up a
        // page-point-sized bitmap; the deadband and the assignment (not a binding) are both Qt's
        // own PdfPageView.qml reRenderIfNecessary, without which a resize re-renders every frame.
        onWidthChanged: page.rerenderIfNeeded()
        // PdfPageImage copies the document's source into its own inherited source property only as
        // the document turns Ready, and a re-render that beats that copy makes Qt warn that the two
        // are in conflict; the guard below waits for the copy and this re-renders once it lands.
        onSourceChanged: page.rerenderIfNeeded()

        function rerenderIfNeeded() {
            // source is a url, so it is converted before comparing: a bare === against a
            // string is false forever and would stop this guard from ever firing.
            if (page.source.toString() === "")
                return
            var target = Math.round(page.width)
            if (target <= 0)
                return
            var ratio = page.sourceSize.width > 0 ? target / page.sourceSize.width : 0
            if (ratio > 1.1 || ratio < 0.9)
                page.sourceSize = Qt.size(target, 0)
        }
    }
}
