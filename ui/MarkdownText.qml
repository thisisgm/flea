import QtQuick

// The padding sets the baseline as CSS does: on every wrapped line of rich text, on the first line of plain text.
Text {
    id: root

    // md_rendered: line-height 1.7 over the text's own pixel size.
    readonly property real boxRatio: 1.7
    readonly property bool rich: root.textFormat !== Text.PlainText
    readonly property real box: Math.round(root.font.pixelSize * root.boxRatio)
    // QTextLine rounds the font's height up, so the first line is ceil(height) tall.
    readonly property real lead: Math.max(0, root.box - Math.ceil(metrics.height))
    // Qt's FixedHeight line puts its baseline this share down the box whatever the font, so the padding moves it to the centred one.
    readonly property real fixedBaselineShare: 0.8
    readonly property int centredBaseline: Math.round((root.box - metrics.height) / 2 + metrics.ascent)
    // The hosts set the Markdown here; a text with a picture draws it with the pictures wider than the text scaled to its width.
    property string markdown: ""
    // A text whose picture is taller than the box draws on proportional lines, which grow with it; the ratio keeps every other line at the box.
    // Sample input: "Built with ![logo](file:///d/p.png) here" holds one picture; the parser writes every local inline picture this way.
    property bool growsForPictures: true
    readonly property bool holdsPicture: root.growsForPictures && root.rich && /!\[[^\]]*\]\(file:/i.test(root.markdown)
    // Built the first time a text holds a picture: a text without one owns no object for it, which the block cost gates count.
    property Item pictures: null
    function syncPictures() {
        if (root.pictures === null && root.holdsPicture)
            root.pictures = Qt.createComponent("MarkdownPictures.qml").createObject(root, { host: root })
    }
    onHoldsPictureChanged: root.syncPictures()
    Component.onCompleted: root.syncPictures()
    readonly property bool grown: root.pictures !== null && root.holdsPicture && root.pictures.tallest > root.box
    readonly property real naturalLine: Math.ceil(metrics.height)
    // Proportional lines keep the font's own ascent above the baseline and put the stretch below it.
    readonly property int naturalBaseline: Math.round(metrics.ascent)
    readonly property int restBaseline: root.grown ? root.naturalBaseline : Math.round(root.box * root.fixedBaselineShare)
    // The tallest picture's line rests on its bottom edge at the baseline, so it is the picture plus the font's own descent.
    readonly property real pictureLine: root.grown ? root.pictures.tallest + root.naturalLine - root.naturalBaseline : 0
    // A one-line text takes back the stretch the ratio put under its picture; a second line would be half a box taller at least.
    readonly property real stretch: root.grown && root.contentHeight < root.pictureLine * root.box / root.naturalLine + root.box / 2 ? root.contentHeight - root.pictureLine : 0
    // Rich text shifts by the gap between the two baselines (negative when Qt sits it low); plain text sits at the top of its natural line.
    readonly property int lift: root.rich ? root.centredBaseline - root.restBaseline : Math.floor(root.lead / 2)
    // The first line's drawn baseline: Qt reports a FixedHeight line's baselineOffset at the ascent, short of where it draws.
    readonly property real drawnBaseline: root.rich && !root.grown ? root.topPadding + root.restBaseline : root.baselineOffset
    // The document's body size; the preview column scales it down from Quick Look's.
    property int bodyPx: Theme.font.body
    // Rows of a table add their own cell padding on top of the box.
    property int cellPad: 0

    // The parser pads a code chip with a no-break space in a span carrying this mark (MdEscape.CHIP_PAD_MARK), as Qt reads no em there.
    readonly property string chipPadMark: '<span style="font-size:chippad">'
    // The board pads a chip 4 px a side at body 14; the span's px is that over the face's space advance, so the pad's advance is those 4 px.
    readonly property real chipPadBoardPx: 4
    readonly property real boardBodyPx: 14
    // advanceWidth is a call no binding tracks, so reading the metrics' font size and family measures the space again once the font settles.
    readonly property int chipPadPx: {
        metrics.font.pixelSize
        metrics.font.family
        return Math.max(1, Math.round(root.chipPadBoardPx * root.bodyPx / root.boardBodyPx * root.bodyPx / Math.max(1, metrics.advanceWidth(" "))))
    }
    function padSized(source) {
        return source.indexOf(root.chipPadMark) < 0 ? source : source.split(root.chipPadMark).join('<span style="font-size:' + root.chipPadPx + 'px">')
    }

    // The host supplies the scheme gate (Markdown.isExternalLink), as no import of the parser fits the standalone probe copy.
    property var linkGate: null
    // One handler for every Markdown text: a link the gate passes opens in the default application, all else opens nothing.
    onLinkActivated: function (link) {
        if (root.linkGate !== null && root.linkGate(link))
            Qt.openUrlExternally(link)
    }

    text: root.padSized(root.pictures !== null ? root.pictures.shown : root.markdown)
    textFormat: Text.MarkdownText
    wrapMode: Text.Wrap
    color: Theme.color.foreground
    linkColor: Theme.color.foreground
    font.family: Theme.font.family
    font.pixelSize: root.bodyPx
    lineHeight: root.grown ? root.box / root.naturalLine : root.rich ? root.box : 1
    lineHeightMode: root.rich && !root.grown ? Text.FixedHeight : Text.ProportionalHeight
    topPadding: root.cellPad + root.lift
    bottomPadding: root.cellPad + root.lead - root.lift - root.stretch

    // A native document is created only after a selection press in the rendered view.
    property var selectionEditor: null

    FontMetrics {
        id: metrics
        font: root.font
    }
}
