import QtQuick

// One rendered figure: the helper's SVG at its aspect ratio within the pane width, or fenced source when rendering fails.
import "js/FigureDate.js" as FigureDate

Item {
    id: root

    property string kind: "math"
    property string source: ""
    property bool display: true
    property bool inline: false
    // A bare figure asks and settles but draws nothing: its host puts the SVG in its own text.
    property bool bare: false
    property string bgHex: Theme.color.background
    property string fgHex: Theme.color.foreground
    property string accentHex: Theme.color.accent
    property string mutedHex: ""
    property string surfaceHex: ""
    // The failed figure's fence sits on the code surface of the pane that holds it.
    property color fallbackColor: Theme.color.surface
    property string fontFamily: Theme.font.family
    property int bodyPx: Theme.font.body
    // The body font's x-height at bodyPx: a formula draws its ex at this height, so maths and prose letters stand level.
    property real xHeight: regularMetrics.xHeight
    // The helper receives the x-height in hundredths of a pixel, so a font that settles late changes the request.
    readonly property int xHeightRounding: 100

    // A diagram sizes each label from the font's advance of every printable ASCII character, regular and bold; maths sends none.
    readonly property int advanceFirst: 32
    readonly property int advanceLast: 126
    // The helper receives the advances in thousandths of an em, so a font that settles late changes the request.
    readonly property int advanceRounding: 1000
    // The advance of each character from the first to the last, read through the metrics' own font so a font change re-runs the table.
    function advanceTable(metrics) {
        var table = [];
        var px = metrics.font.pixelSize;
        for (var code = root.advanceFirst; code <= root.advanceLast; code++)
            table.push(Math.round(metrics.advanceWidth(String.fromCharCode(code)) / px * root.advanceRounding));
        return table;
    }
    readonly property var advances: root.kind !== "mermaid" ? [] : root.advanceTable(regularMetrics)
    readonly property var boldAdvances: root.kind !== "mermaid" ? [] : root.advanceTable(boldMetrics)
    FontMetrics {
        id: regularMetrics
        font.family: root.fontFamily
        font.pixelSize: root.bodyPx
    }
    FontMetrics {
        id: boldMetrics
        font.family: root.fontFamily
        font.pixelSize: root.bodyPx
        font.bold: true
    }

    readonly property bool failed: root.error !== ""
    readonly property bool ready: root.svg !== ""
    readonly property bool working: root.ticket > 0
    property string svg: ""
    property string error: ""
    property int ticket: 0
    // The Source view sends no figure requests.
    property bool askArmed: true
    // Offscreen delegates retain settled figures and send no requests.
    property bool inView: true

    // The theme an ask carries, given the label advances its kind sends; the preview's warm query builds the same one, so its cache keys match.
    function themeWith(advances, boldAdvances) {
        return { bg: root.bgHex, fg: root.fgHex, accent: root.accentHex,
            font: root.fontFamily, bodyPx: root.bodyPx,
            exPx: Math.round(root.xHeight * root.xHeightRounding) / root.xHeightRounding,
            advances: advances, boldAdvances: boldAdvances,
            muted: root.mutedHex, surface: root.surfaceHex };
    }
    function hexTheme() {
        return root.themeWith(root.advances, root.boldAdvances);
    }
    // The theme a figure of `kind` is asked under here, whatever this item's own kind.
    function themeOfKind(kind) {
        return kind === "mermaid" ? root.themeWith(root.advanceTable(regularMetrics), root.advanceTable(boldMetrics)) : root.themeWith([], []);
    }
    // Nothing is asked until the figure is created, so its construction-time assignments cost no request.
    property bool created: false
    // The request this figure last sent; an ask for the same state is dropped, so a burst of changes renders once.
    property string lastRequest: ""
    function ask() {
        root.askRuns++;
        // Offscreen property changes invalidate the settled figure until it enters the viewport.
        if (!root.askArmed || root.source === "" || !root.inView) {
            root.ticket = 0;
            root.svg = "";
            root.error = "";
            root.lastRequest = "";
            return;
        }
        // A Gantt chart is asked for today, so its key, its stored entry and its drawing name one day.
        var theme = FigureDate.datedTheme(root.kind, root.source, root.hexTheme());
        var request = JSON.stringify([root.kind, root.source, root.display, theme]);
        if (request === root.lastRequest)
            return;
        root.lastRequest = request;
        root.ticket = FigureService.ask(root.kind, root.source, root.display, theme);
    }
    // The wait, in ms, that lets the list lay the delegate out before the one ask reads its place.
    readonly property int askSettleMs: 50
    // The suite counts ask runs and reads the armed deferred ask, so it waits and asserts without timing.
    property int askRuns: 0
    readonly property bool askPending: askTimer.running
    // Every property change lands on the one deferred ask, so the changes of one burst send one request.
    function schedule() {
        if (root.created && !root.takeRemembered())
            askTimer.restart();
    }

    onKindChanged: root.schedule()
    onSourceChanged: root.schedule()
    onDisplayChanged: root.schedule()
    onBgHexChanged: root.schedule()
    onFgHexChanged: root.schedule()
    onAccentHexChanged: root.schedule()
    onMutedHexChanged: root.schedule()
    onSurfaceHexChanged: root.schedule()
    onFontFamilyChanged: root.schedule()
    onBodyPxChanged: root.schedule()
    onXHeightChanged: root.schedule()
    onAdvancesChanged: root.schedule()
    onBoldAdvancesChanged: root.schedule()
    onAskArmedChanged: root.schedule()
    // Entering the viewport requests an unsettled figure after layout.
    onInViewChanged: if (root.created && root.inView && root.askArmed && root.source !== "" && root.ticket === 0 && root.svg === "" && root.error === "") root.ask()
    // The service's held answer for this exact request, taken on the change itself, so a formula typeset ahead is drawn in the first frame.
    function takeRemembered() {
        if (!root.askArmed || root.source === "" || !root.inView)
            return false;
        var theme = FigureDate.datedTheme(root.kind, root.source, root.hexTheme());
        var hit = FigureService.cached(root.kind, root.source, theme, root.display);
        if (hit === undefined)
            return false;
        // A held answer drops any older request, so nothing in flight lands over it.
        root.ticket = 0;
        askTimer.stop();
        root.lastRequest = JSON.stringify([root.kind, root.source, root.display, theme]);
        root.svg = hit;
        root.error = "";
        return true;
    }
    Component.onCompleted: {
        root.created = true;
        root.schedule();
    }

    // ListView places the delegate after it completes, so the one ask waits out layout and inView reads the placed position.
    Timer {
        id: askTimer
        interval: root.askSettleMs
        onTriggered: root.ask()
    }

    Connections {
        target: FigureService
        function onDone(ticket, svg, error) {
            if (ticket !== root.ticket)
                return;
            root.ticket = 0;
            if (svg !== "") {
                root.svg = svg;
                root.error = "";
            } else {
                root.svg = "";
                root.error = error;
            }
        }
    }

    // Data URLs deliver figures to Qt SVG without filesystem writes.
    readonly property string dataUrl: root.svg === "" ? ""
        : "data:image/svg+xml," + encodeURIComponent(root.svg)

    Image {
        id: figure
        visible: !root.inline && root.svg !== ""
        anchors.top: parent.top
        anchors.left: root.centred ? undefined : parent.left
        anchors.horizontalCenter: root.centred ? parent.horizontalCenter : undefined
        // A display figure alone decodes here; an inline one draws through inlineFigure below or in its host's own text.
        source: root.inline ? "" : root.dataUrl
        cache: false
        asynchronous: true
        width: root.fitWidth
        height: root.fitHeight
        fillMode: Image.PreserveAspectFit
    }

    // An inline formula beside text: line height tall, aspect kept.
    Image {
        id: inlineFigure
        visible: root.inline && root.svg !== ""
        anchors.top: parent.top
        source: root.inline && !root.bare ? root.dataUrl : ""
        cache: false
        asynchronous: true
        height: root.bodyPx
        width: root.inlineWidth
        fillMode: Image.PreserveAspectFit
    }

    // A failed figure draws bounded source on the code surface.
    readonly property int fallbackChars: 2000
    readonly property string fallbackBody: root.source.length > root.fallbackChars
        ? root.source.slice(0, root.fallbackChars) + "… (" + (root.source.length - root.fallbackChars) + " more)"
        : root.source
    // The fence recipe, so fallback text sits where a fenced block does: the host hands down the preview's own fencePadX and fencePadY.
    property int fencePadX: 0
    property int fencePadY: 0
    Rectangle {
        id: fallback
        visible: root.failed
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: fallbackItem.implicitHeight + 2 * root.fencePadY
        color: root.fallbackColor
        Text {
            id: fallbackItem
            anchors.fill: parent
            anchors.leftMargin: root.fencePadX
            anchors.rightMargin: root.fencePadX
            anchors.topMargin: root.fencePadY
            anchors.bottomMargin: root.fencePadY
            text: root.failed ? root.fallbackBody : ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.color.foreground
            font.family: root.fontFamily
            font.pixelSize: root.bodyPx
        }
    }

    readonly property bool centred: root.kind === "math" && root.display && !root.inline
    readonly property real naturalWidth: !root.inline && figure.implicitWidth > 0 ? figure.implicitWidth : 0
    readonly property real naturalHeight: !root.inline && figure.implicitHeight > 0 ? figure.implicitHeight : 0
    // Integer geometry keeps the raster 1:1: a fractional item size would resample the whole figure and blend every flat fill.
    readonly property real fitWidth: root.naturalWidth <= 0 ? 0 : Math.round(Math.min(root.naturalWidth, root.width))
    readonly property real fitHeight: root.naturalWidth <= 0 ? 0 : Math.round(root.naturalHeight * (root.fitWidth / root.naturalWidth))
    readonly property real inlineWidth: root.inline && inlineFigure.implicitHeight > 0
        ? Math.round(inlineFigure.implicitWidth * (root.bodyPx / inlineFigure.implicitHeight)) : 0

    implicitWidth: root.inline ? (root.failed ? fallbackItem.implicitWidth + 2 * root.fencePadX : root.inlineWidth) : root.width
    // A failed inline still draws its fence, so it sizes to the fence rather than the line it never became.
    implicitHeight: root.inline ? (root.failed ? fallback.height : root.bodyPx)
        : root.failed ? fallback.height : root.fitHeight
    height: root.implicitHeight
}
