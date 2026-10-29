pragma ComponentBehavior: Bound

import QtQuick
import qs.Commons
import "." as Flea
import "js/CodeRuns.js" as CodeRuns

// The lines of one file or text, coloured by the backend's code engine, asked a screen-and-margin at a time; a network file's `text` is the caller's.
Item {
    id: root

    property var backend: null
    // The backend's name for this view's job; a newer ask on it ends the older one.
    property string slot: "column"
    property string path: ""
    // true sends the bytes in `text` (null until the caller has read them) instead of the path.
    property bool fromText: false
    property var text: null
    property bool active: false
    property bool numbered: false
    property bool wrap: false
    // Enables wheel and drag scrolling in the line viewport.
    property bool scrolls: false
    readonly property alias viewport: view
    // A Component the list draws above its first line and scrolls with the rows; null for none.
    property Component header: null
    property real pixelSize: Theme.font.caption
    // The most lines drawn, 0 for every line of the file.
    property int limit: 0
    // Screens asked past the view on each side.
    property int margin: 1
    readonly property int lineCap: 400 // The most lines one ask names (docs/protocol.md "code"); a longer window is asked in parts.

    // A backend that cannot ask (a bare stub) leaves the caller's own text to be drawn plain.
    readonly property bool engine: root.backend !== null && typeof root.backend.askCode === "function"
    // "" until the lines in view are answered, then styled, plain, binary, large or failed.
    property string answer: ""
    property int total: 0
    // The lines held, by 1-based number, read through rowAt; stamp counts changes to it.
    property var held: ({})
    property int stamp: 0
    // The token of the ask in flight, 0 when none: an answer for any other token is dropped.
    property int token: 0
    property bool retiring: false
    // The window of that ask, so the same window is not asked for twice while it is in flight.
    property var asking: null
    readonly property bool settled: root.answer === "styled" || root.answer === "plain"
    readonly property int shown: root.limit > 0 ? Math.min(root.total, root.limit) : root.total
    // True when the lines in view are all held.
    readonly property bool firstScreen: root.settled && root.covered(root.needed().from, root.needed().to)
    // The plain text of the lines drawn, for the readers that check what is on screen.
    readonly property var lines: {
        var out = []
        for (var i = 1; i <= root.shown; i++)
            out.push(root.rowAt(i) ? CodeRuns.plain(root.rowAt(i)) : "")
        return out
    }
    // Indexed by the code reply's role: the foreground, then red, green, yellow, blue, magenta and cyan.
    readonly property var inks: CodeRuns.inks(Theme.palette, Theme.fallbackColor, String(Theme.color.foreground))
    readonly property real rowHeight: metrics.height
    // The gutter is as wide as the widest number it can show, so the text never shifts as it scrolls.
    readonly property real gutterWidth: (Math.max(2, String(root.shown).length) + 1) * metrics.advanceWidth

    // The runs of line n, undefined until the backend has answered for it; reading it through the stamp makes a binding follow every answer.
    function rowAt(n) {
        return root.stamp >= 0 ? root.held[n] : undefined
    }

    function reset() {
        root.held = {}
        root.stamp += 1
        root.total = 0
        root.answer = ""
        root.token = 0
        root.asking = null
    }

    // Starts a view of a new file or text; the first ask waits for the end of the turn so one change of several properties asks once.
    function fresh() {
        root.reset()
        if (!root.active) {
            askTimer.stop()
            return
        }
        if (!root.engine && root.fromText && root.text !== null)
            root.answerPlain()
        else
            root.schedule()
    }

    // The owned timer coalesces asks and is destroyed with the reader, including a pending ask during a preview handoff.
    function schedule() {
        if (root.active)
            askTimer.restart()
    }

    // Sample input: "a\nb\n" holds only lines 1 and 2, as [0, "a"] and [0, "b"], matching the engine's terminal-newline rule.
    function answerPlain() {
        var rows = root.text.length > 0 ? root.text.split("\n") : []
        if (rows.length > 0 && root.text.endsWith("\n"))
            rows.pop()
        var held = {}
        for (var i = 0; i < rows.length && (root.limit <= 0 || i < root.limit); i++)
            held[i + 1] = [0, rows[i].endsWith("\r") ? rows[i].slice(0, -1) : rows[i]]
        root.held = held
        root.stamp += 1
        root.total = rows.length
        root.answer = "plain"
    }

    // The lines the view needs held: those in it plus the margin, never past the file or the limit.
    function needed() {
        var rows = Math.max(1, Math.ceil(view.height / Math.max(1, root.rowHeight)))
        var top = view.contentHeight > 0 ? Math.max(0, view.indexAt(0, view.contentY + 1)) + 1 : 1
        var to = top + rows * (1 + root.margin) - 1
        if (root.limit > 0)
            to = Math.min(to, root.limit)
        if (root.settled)
            to = Math.min(to, root.total)
        return { from: Math.max(1, top - rows * root.margin), to: to }
    }

    function covered(from, to) {
        for (var i = from; i <= to; i++)
            if (!root.rowAt(i))
                return false
        return true
    }

    function ask() {
        if (root.retiring || !root.active || !root.engine || view.height <= 0)
            return
        if (root.answer === "binary" || root.answer === "large" || root.answer === "failed")
            return
        if (root.fromText && root.text === null)
            return
        if (root.fromText && root.text.length === 0) {
            root.answerPlain()
            return
        }
        if (!root.fromText && root.path.length === 0)
            return
        var want = root.needed()
        if (want.to < want.from || (root.settled && root.covered(want.from, want.to)))
            return
        if (root.token !== 0 && root.asking.from === want.from && root.asking.to === want.to)
            return
        root.asking = want
        var from = want.from
        while (from < want.to && root.rowAt(from)) from++
        var source = root.fromText ? { text: root.text, name: root.path.split("/").pop(), lang: "" } : { path: root.path }
        root.token = root.backend.askCode(root.slot, source, from, Math.min(root.lineCap, want.to - from + 1))
    }

    function answered(message) {
        if (root.token === 0 || message.token !== root.token)
            return
        root.token = 0
        root.answer = message.state
        root.total = message.total || 0
        var rows = message.lines || []
        for (var i = 0; i < rows.length; i++)
            root.held[message.first + i] = rows[i]
        root.stamp += 1
        // Binary, large and failed are the file's whole answer: nothing more is asked of it until the view starts a new one.
        if (root.settled)
            root.schedule()
    }

    // A pane handoff keeps the engine capability, but disconnects the old backend's request.
    function rebind() {
        root.fresh()
    }

    onBackendChanged: root.rebind()
    onPathChanged: root.fresh()
    onTextChanged: root.fresh()
    onFromTextChanged: root.fresh()
    onActiveChanged: root.fresh()
    Component.onDestruction: root.retiring = true
    onEngineChanged: root.fresh()
    onLimitChanged: root.schedule()

    Timer { id: askTimer; interval: 0; onTriggered: root.ask() }

    Connections {
        target: root.engine ? root.backend : null
        function onCodeResult(message) { root.answered(message) }
    }

    TextMetrics {
        id: metrics
        font.family: Theme.font.family
        font.pixelSize: root.pixelSize
        text: "0"
    }

    Flea.PreviewTextSelection {
        anchors.fill: parent
        document: root
        textRoot: view.contentItem
        visible: root.active
        z: 2
    }

    ListView {
        id: view
        anchors.fill: parent
        clip: true
        interactive: root.scrolls
        cacheBuffer: 0
        boundsBehavior: Flickable.StopAtBounds
        model: root.shown
        header: root.header
        onContentYChanged: root.schedule()
        onHeightChanged: root.schedule()

        delegate: Item {
            id: line
            required property int index
            readonly property var runs: root.rowAt(line.index + 1)
            width: view.width
            height: Math.max(body.implicitHeight, root.rowHeight)

            Text {
                visible: root.numbered
                width: root.gutterWidth
                text: String(line.index + 1)
                horizontalAlignment: Text.AlignRight
                color: Theme.color.muted
                opacity: 0.6
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }

            // corner: file contents are arbitrary text, so every run is escaped and the string holds only the font tags written here.
            Text {
                id: body
                property var selectionEditor: null
                x: root.numbered ? root.gutterWidth : 0
                width: line.width - x
                leftPadding: 2 * metrics.advanceWidth
                text: line.runs ? CodeRuns.styled(line.runs, root.inks) : ""
                color: Theme.color.foreground
                font.family: Theme.font.family
                font.pixelSize: root.pixelSize
                textFormat: Text.StyledText
                wrapMode: root.wrap ? Text.WrapAtWordBoundaryOrAnywhere : Text.NoWrap
                // One line per row unless wrapped, ending in an ellipsis at the view's edge.
                elide: root.wrap ? Text.ElideNone : Text.ElideRight
            }
        }
    }
}
