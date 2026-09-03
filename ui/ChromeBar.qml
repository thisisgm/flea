import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/PathBar.js" as PathBar

// The window's top chrome, per the canvas: where you are on the left, how you are looking at it on
// the right. The path lives here rather than in the status bar, which the design gives to counts.
Item {
    id: root

    property string path: ""
    property string home: ""
    property bool canGoBack: false
    property bool canGoUp: false
    // "list", "columns" or "grid"; the button naming the current one takes the accent.
    property string viewMode: "list"
    // Read by the path bar alone, so a Tab on a dotted leaf peeks the way the listing is set to look.
    property bool showHidden: false

    signal backRequested()
    signal upRequested()
    signal searchRequested()
    signal viewChosen(string mode)
    // The path bar's four. ui/shell.qml navigates, hands the keyboard back, runs the peek behind Tab
    // and carries what the bar says to the status line, because this file draws the chrome and knows
    // nothing about the backend, the pane or the bar below it.
    signal pathEntered(string path)
    signal editClosed()
    signal completeRequested(string dir, bool hidden)
    signal said(string text)

    // The path bar: the same strip, typed instead of drawn. ":" and Ctrl+L open it, so does a double
    // click on the path, and it is where the whole of keys.toml's pathBar action lands.
    property bool editing: false
    // What the seam reads: the line as it stands, and the box a test double-clicks to open the bar.
    readonly property alias editText: field.text
    readonly property alias pathArea: pathArea
    // The directory a Tab is waiting on, and the one that came back. Both are keyed by the hidden
    // flag as well as the path, or a Tab on ".conf" would answer off rows peeked without dotfiles in
    // them: the key is what the request asked for and never what the line happens to read later.
    property string pendingDir: ""
    property string pendingKey: ""
    property string cachedKey: ""
    property var cachedNames: []

    function startEdit() {
        if (root.editing) {
            return
        }
        root.editing = true
        // The trailing slash is what makes typing a child the natural next keystroke, and the line
        // opens selected, so a name typed straight away replaces it instead of joining onto it.
        field.text = root.path === "/" ? "/" : root.path + "/"
        field.forceActiveFocus()
        field.selectAll()
    }

    function closeEdit() {
        if (!root.editing) {
            return
        }
        root.editing = false
        root.pendingDir = ""
        root.pendingKey = ""
        root.cachedKey = ""
        root.cachedNames = []
        // The field is inside no focus scope of its own, so the keyboard goes nowhere until the pane
        // is told to take it back; ui/shell.qml is what does that.
        root.editClosed()
    }

    function commitEdit() {
        var target = PathBar.resolve(field.text, root.path, root.home)
        root.closeEdit()
        // An empty line closes the bar and nothing else, and so does the path already being shown:
        // re-listing the directory under the cursor would drop the selection for no navigation.
        if (target.length > 0 && target !== root.path) {
            root.pathEntered(target)
        }
    }

    // Tab. The rows come from the backend's peek, which is the read the columns view already makes of
    // a directory that is not the pane's, so completing costs no new request type.
    function completeEdit() {
        var dir = PathBar.completionDir(field.text, root.path, root.home)
        var hidden = PathBar.wantsHidden(field.text, root.showHidden)
        var key = dir + "\u0000" + hidden
        if (key === root.cachedKey) {
            root.applyCompletion(dir, root.cachedNames)
            return
        }
        root.pendingDir = dir
        root.pendingKey = key
        root.completeRequested(dir, hidden)
    }

    // A peek came back. The columns view peeks the pane's ancestors on the same wire, so a response
    // this bar did not ask for is dropped rather than cached under a key it cannot vouch for. Only
    // directories are kept: this bar goes to a directory, and a name that cannot be opened is not a
    // completion.
    function completeWith(dir, rows) {
        if (root.pendingDir !== dir) {
            return
        }
        var names = []
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].d) {
                names.push(rows[i].n)
            }
        }
        root.cachedKey = root.pendingKey
        root.cachedNames = names
        root.pendingDir = ""
        root.pendingKey = ""
        if (root.editing && PathBar.completionDir(field.text, root.path, root.home) === dir) {
            root.applyCompletion(dir, names)
        }
    }

    function applyCompletion(dir, names) {
        var before = field.text
        var after = PathBar.complete(before, names)
        field.text = after.text
        field.cursorPosition = field.text.length
        var say = PathBar.completionMessage(before, after, dir)
        if (say.length > 0) {
            root.said(say)
        }
    }

    // A path reads as the user writes it, so home comes back as a tilde; the leaf is the directory
    // you are actually in and takes full contrast, everything above it stays muted.
    readonly property string display: Format.tilde(root.path, root.home)

    // A chrome strip, not a data row; see Theme.qml's chromeHeight comment.
    implicitHeight: Theme.chromeHeight

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }

    // A test drives these by coordinate, because a glyph button carries no text to find on screen.
    function buttonFor(glyph) {
        var groups = [nav, views]
        for (var g = 0; g < groups.length; g++) {
            var kids = groups[g].children
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].glyph === glyph)
                    return kids[i]
            }
        }
        return null
    }

    Row {
        id: nav
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacing.gap

        Flea.ChromeButton {
            glyph: "arrow-left"
            enabled: root.canGoBack
            onActivated: root.backRequested()
        }

        Flea.ChromeButton {
            glyph: "arrow-up"
            enabled: root.canGoUp
            onActivated: root.upRequested()
        }
    }

    // corner: a path is arbitrary text, so PlainText, the same rule every filename on this surface follows.
    Text {
        id: pathText
        visible: !root.editing
        anchors.left: nav.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: views.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        // The tail identifies the directory, so a path too long for the bar elides from its left.
        elide: Text.ElideLeft
        text: Format.parentPart(root.display)
    }

    Text {
        id: leafText
        visible: !root.editing
        anchors.left: pathText.left
        anchors.leftMargin: Math.min(pathText.contentWidth, pathText.width)
        anchors.right: views.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        text: Format.leafPart(root.display)
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // Where the path is drawn is where it is typed. A double click opens the bar, which is the
    // pointer's half of ":" and Ctrl+L; a single click is left alone, because the path is a label
    // and not a control, and one that armed on a brush past would be in the way of every other click.
    Item {
        id: pathArea
        anchors.left: nav.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: views.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        HoverHandler {
            cursorShape: Qt.IBeamCursor
        }

        TapHandler {
            acceptedButtons: Qt.LeftButton
            onDoubleTapped: root.startEdit()
        }

        // The rename editor's own frame, at chrome scale: the accent says which strip has the
        // keyboard, and the fill covers the two Texts underneath rather than relying on their visible.
        Rectangle {
            visible: root.editing
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.hairline * 2
            anchors.bottomMargin: Theme.spacing.hairline * 2
            color: Theme.color.background
            radius: Style.cornerRadius
            border.width: Theme.spacing.hairline
            border.color: Theme.color.accent
        }

        // corner: a typed path is arbitrary text, so it is drawn at the same size the path it
        // replaces is, and the caret sits on the line rather than on a row of its own.
        TextInput {
            id: field
            visible: root.editing
            enabled: root.editing
            anchors.fill: parent
            anchors.leftMargin: Theme.spacing.gap
            anchors.rightMargin: Theme.spacing.gap
            verticalAlignment: TextInput.AlignVCenter
            color: Theme.color.foreground
            selectionColor: Theme.color.accent
            selectedTextColor: Theme.color.background
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            clip: true

            // Every one of these is handled and accepted here, for the reason ui/RenameField.qml
            // gives: an unaccepted key goes on to the pane's own handler, which would read Return as
            // "open the row under the cursor" and Tab as "move to the rail" while the bar is up.
            Keys.onPressed: function (event) {
                if (event.key === Qt.Key_Escape) {
                    root.closeEdit()
                    event.accepted = true
                    return
                }
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    root.commitEdit()
                    event.accepted = true
                    return
                }
                if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
                    root.completeEdit()
                    event.accepted = true
                }
            }

            // Losing the keyboard closes the bar, the rule the rename editor already follows: a bar
            // left standing over a window that has moved on would commit against the wrong directory.
            onActiveFocusChanged: if (!activeFocus && root.editing) root.closeEdit()
        }
    }

    Row {
        id: views
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacing.gap

        Flea.ChromeButton {
            glyph: "search"
            onActivated: root.searchRequested()
        }

        Repeater {
            model: ["list", "columns", "grid"]
            delegate: Flea.ChromeButton {
                required property string modelData
                glyph: modelData
                active: root.viewMode === modelData
                onActivated: root.viewChosen(modelData)
            }
        }
    }
}
