import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Kinds.js" as Kinds
import "js/PreviewKeys.js" as PreviewKeys
import "js/Scroll.js" as Scroll
import "js/Thumbs.js" as Thumbs

// The Quick Look's image pane, reached only through Preview.qml's Loader. It draws the file itself:
// the 256 px cache file the column draws would be an eightfold upscale on a surface this size.
Item {
    id: root

    // A zoomed picture runs past the surface's edges.
    clip: true

    property string path: ""

    // A camera RAW is drawn through the copy of its embedded JPEG that Preview.qml's meta reply names, never itself; its picture area stops above the caption band.
    readonly property bool raw: Kinds.isRaw(root.path)
    property string embedded: ""
    property bool embeddedAnswered: false
    readonly property string drawPath: root.raw ? root.embedded : root.path
    readonly property real areaHeight: root.raw ? root.height - Theme.chromeHeight : root.height

    // Times the fit size, on the PDF's ladder; 1 is fit.
    property real zoom: PreviewKeys.PDF_ZOOM_MIN
    readonly property bool zoomed: root.zoom > PreviewKeys.PDF_ZOOM_MIN
    // The zoom read against fit; fit itself carries no label.
    readonly property string zoomLabel: root.zoomed ? "%1%".arg(Math.round(root.zoom * 100)) : ""
    // The pannable viewport, the strip under it and the sharper layer, for Preview.qml's readers and the zoom suite.
    readonly property alias viewport: flick
    readonly property alias band: band
    readonly property alias fullPicture: sharp
    // Another file is a new subject: the next one opens at fit.
    onPathChanged: root.zoom = PreviewKeys.PDF_ZOOM_MIN

    // The viewport's centre point stays put across a step; a picture still decoding or refused does not zoom in.
    function zoomTo(next) {
        if (next === root.zoom || (next > PreviewKeys.PDF_ZOOM_MIN && picture.status !== Image.Ready)) return
        var fx = (flick.contentX + flick.width / 2) / flick.contentWidth
        var fy = (flick.contentY + flick.height / 2) / flick.contentHeight
        root.zoom = next
        flick.contentX = Scroll.bounded(fx * flick.contentWidth - flick.width / 2, flick.originX, flick.contentWidth, flick.width, flick.leftMargin, flick.rightMargin)
        flick.contentY = Scroll.bounded(fy * flick.contentHeight - flick.height / 2, flick.originY, flick.contentHeight, flick.height, flick.topMargin, flick.bottomMargin)
    }
    function zoomBy(steps) { root.zoomTo(PreviewKeys.zoomStep(root.zoom, steps)) }
    function zoomFit() { root.zoomTo(PreviewKeys.PDF_ZOOM_MIN) }

    // The row's cache file, held by Quick Look while this decodes: drawn between the
    // ground and the picture, so it is never hidden by one nor covers the other.
    property string interimThumb: ""
    property real interimX: 0
    property real interimY: 0
    property real interimWidth: 0
    property real interimHeight: 0
    property bool interimVisible: false
    // Set from open while this row's thumbnail may still be asked for; it only gates failed.
    property bool thumbPending: false
    readonly property bool interimReady: interimPicture.status === Image.Ready

    // The same name the media and PDF panes give their unreadable state, so Preview.qml tests one property; failed only with no thumbnail to stand in or coming, and a decoding one keeps loading.
    readonly property bool failed: root.pictureFailed && !root.thumbPending && interimPicture.status !== Image.Ready && interimPicture.status !== Image.Loading
    // Every state is terminal once an owed thumbnail has landed or been refused: a failed final waits on it first.
    readonly property string status: {
        if (root.failed) return "This image could not be read."
        return (picture.status === Image.Ready || root.fallbackShown) ? "image" : "loading"
    }
    // Frames in a GIF, from the same reply; the picture moves only past one.
    property int frames: 0
    // The original's pixels for the thumbnail fit, from Preview.qml's meta reply; 0 while unknown.
    property real origW: 0
    property real origH: 0
    // The turn of those pixels, so the caption names the picture as drawn.
    property int origOrient: 1
    // A 3D model is refused with no decode attempt: Qt has no plugin for it, and each attempt logs a warning.
    readonly property bool decodable: !Kinds.isModel(root.path)
    // The final refused, by a decode Error, at once for a file Qt cannot decode, or once a RAW answered with no copy; the thumbnail below stands in when it decoded.
    readonly property bool pictureFailed: picture.status === Image.Error || (root.interimSettled && !root.decodable) || (root.raw && root.embeddedAnswered && root.embedded.length === 0)
    readonly property bool fallbackShown: root.pictureFailed && root.interimReady
    readonly property string name: root.path.substring(root.path.lastIndexOf("/") + 1)

    // The pixmap reader decodes only the final, one request at a time; it waits a turn after the build because Preview.qml binds the path before the thumbnail.
    property bool armed: false
    readonly property bool interimSettled: root.armed && interimPicture.status !== Image.Loading
    Timer {
        running: true
        interval: 0
        onTriggered: root.armed = true
    }

    // The letterbox ground the video pane uses: a photo almost never matches the surface's proportions.
    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
    }

    // The cached thumbnail at once, sized to the final's rect by the caller; it stays as the picture at the column fit when the final cannot decode.
    Image {
        id: interimPicture
        readonly property real ownFit: Thumbs.fitScale(root.width, root.areaHeight, implicitWidth, implicitHeight,
            Thumbs.posterLimit(false, implicitWidth, implicitHeight, root.origW, root.origH))
        x: root.fallbackShown ? Math.round((root.width - width) / 2) : root.interimX
        y: root.fallbackShown ? Math.round((root.areaHeight - height) / 2) : root.interimY
        width: root.fallbackShown ? implicitWidth * ownFit : root.interimWidth
        height: root.fallbackShown ? implicitHeight * ownFit : root.interimHeight
        // Only while the decode runs: a final with transparent pixels would show it through.
        visible: root.interimVisible && root.status === "loading" || root.fallbackShown
        source: root.interimThumb.length > 0 ? Format.fileUri(root.interimThumb) : ""
        fillMode: Image.Stretch
        // A cache thumbnail decodes on the GUI thread in about a millisecond: the reader cannot drop a running decode.
        asynchronous: false
        cache: false
    }

    // Stretch, in an item sized to the fit of what was decoded: Qt then decodes the exact fit and never enlarges
    // in the decode, where Fit decoded a covering size (2099x3149 for a portrait photo, 34680x1156 for a 3000x100
    // banner, measured). A small picture draws at its own size, centred; only an SVG keeps Fit, which Stretch would distort.
    readonly property bool vector: /\.svgz?$/i.test(root.path)
    // The drawn picture itself, for ui/Ipc.qml's previewPictureRect: the item is sized to the
    // fit of what was decoded, so its rect is the picture and not the surface it is centred on.
    readonly property Item pictureItem: root.fallbackShown ? interimPicture : picture

    Image {
        id: picture
        readonly property real fit: Thumbs.fitScale(root.width, root.areaHeight, implicitWidth, implicitHeight, 1)
        x: Math.round((flick.contentWidth - width) / 2 - flick.contentX)
        y: Math.round((flick.contentHeight - height) / 2 - flick.contentY)
        width: root.vector ? root.width * root.zoom : implicitWidth * fit * root.zoom
        height: root.vector ? root.height * root.zoom : implicitHeight * fit * root.zoom
        // The sharper layer takes over once it has a frame, so a transparent picture never shows two.
        visible: picture.status === Image.Ready && !sharp.drawn && !(gif.item && gif.item.ready)
        // Format.fileUri, not a concatenation: a # or a ? in the name would truncate a hand-built URI.
        source: root.drawPath.length > 0 && root.decodable && root.interimSettled ? Format.fileUri(root.drawPath) : ""
        fillMode: root.vector ? Image.PreserveAspectFit : Image.Stretch
        // A phone keeps a portrait photo's turn in EXIF, and Qt leaves it unapplied unless asked.
        autoTransform: true
        asynchronous: true
        cache: false
        // Decode for the surface in physical pixels so scaled displays retain their detail.
        // Keep the fit geometry in logical pixels; only the decode target follows display scaling.
        // corner: a zero here means unbounded to Qt, so the floor is 1 and never 0.
        sourceSize.width: Math.max(1, Math.round(root.width * Screen.devicePixelRatio))
        sourceSize.height: Math.max(1, Math.round(root.areaHeight * Screen.devicePixelRatio))
    }

    // Past fit the fit picture is drawn scaled until this layer has a frame.
    Flea.PreviewImageSharp {
        id: sharp
        base: picture
        active: root.zoomed
        want: root.vector ? Qt.size(Math.max(1, Math.round(root.width * root.zoom * Screen.devicePixelRatio)), Math.max(1, Math.round(root.height * root.zoom * Screen.devicePixelRatio))) : Qt.size(0, 0)
    }

    // An animated GIF moves once the still is drawn, in the still's rect at any zoom; it is built for that file alone.
    Loader {
        id: gif
        x: picture.x
        y: picture.y
        width: picture.width
        height: picture.height
        active: root.frames > 1 && /\.gif$/i.test(root.path) && picture.status === Image.Ready
        source: "PreviewAnimation.qml"
        onLoaded: {
            item.path = Qt.binding(function () { return root.path })
            item.shown = Qt.binding(function () { return root.visible })
        }
    }

    // The pan: a drag, the wheel and the knobs move contentX and contentY, and the pictures above follow them. At fit it is the whole surface and holds still; zoomed, it is the surface less the band.
    Flickable {
        id: flick
        width: root.width
        height: root.zoomed ? root.height - band.height : root.areaHeight
        contentWidth: Math.max(flick.width, picture.width)
        contentHeight: Math.max(flick.height, picture.height)
        interactive: root.zoomed
        boundsBehavior: Flickable.StopAtBounds

        // The wheel and the knobs are built at the first step past fit and go at fit: the first frame at fit holds neither.
        Loader {
            parent: flick
            anchors.fill: parent
            active: root.zoomed
            sourceComponent: Flea.PreviewImagePan {}
        }
    }

    // The zoom as a number under the picture, a media strip tall with no divider; absent at fit, except that a RAW's fit says what is drawn.
    readonly property bool captioned: root.raw && !root.zoomed && picture.status === Image.Ready
    readonly property string caption: "Embedded preview \u00b7 %1 \u00d7 %2".arg(root.origOrient >= 5 ? root.origH : root.origW).arg(root.origOrient >= 5 ? root.origW : root.origH)
    Rectangle {
        id: band
        visible: root.zoomed || root.captioned
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: Theme.chromeHeight
        color: root.zoomed ? Theme.color.surface : Theme.color.background

        Text {
            anchors.centerIn: parent
            text: root.zoomed ? root.zoomLabel : root.caption
            color: root.zoomed ? Theme.color.foreground : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            font.features: { "tnum": 1 }
            textFormat: Text.PlainText
        }
    }

    // A failed decode is a mark and a sentence, never a bare ground: the blank-frame class again otherwise.
    Column {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap
        visible: root.failed

        Flea.Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            // A failure mark stands alone, so it takes the pane-state ceiling States.dc.html draws at 40.
            maxSize: Theme.stateMarkSize
            width: Theme.stateMarkSize
            height: Theme.stateMarkSize
            name: "alert"
            color: Theme.color.error
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.status
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.name
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
            elide: Text.ElideMiddle
        }
    }
}
