import QtQuick
import qs.Commons
import "." as Flea
import "js/Nav.js" as Nav
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
    // The elided head's own marker, so a test can click the one spot the crumbs slide underneath.
    readonly property alias elisionMarker: elision
    // Issue 45's segments as items, so tests/ui.sh can press one the way it presses a tab.
    readonly property alias crumbItems: crumbs
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
        var typed = field.text
        var target = PathBar.resolve(typed, root.path, root.home)
        root.closeEdit()
        // An empty line closes the bar and nothing else, and so does the path already being shown:
        // re-listing the directory under the cursor would drop the selection for no navigation.
        if (target.length > 0 && target !== root.path) {
            root.pathEntered(target)
            return
        }
        // A line that named something and still resolved to nothing is a file:// URI on another
        // host. Silence there would read as a broken Enter, so it gets the sentence the rest of
        // this application gives a key that cannot do what was asked.
        if (PathBar.refused(typed)) {
            root.said("That URI names a file on another host, not a path on this machine.")
        }
    }

    // Tab. The rows come from the backend's peek, which is the read the columns view already makes of
    // a directory that is not the pane's, so completing costs no new request type.
    function completeEdit() {
        var dir = PathBar.completionDir(field.text, root.path, root.home)
        var hidden = PathBar.wantsHidden(field.text, root.showHidden)
        var key = PathBar.requestKey(dir, hidden)
        if (key === root.cachedKey) {
            root.applyCompletion(dir, root.cachedNames)
            return
        }
        root.pendingDir = dir
        root.pendingKey = key
        root.completeRequested(dir, hidden)
    }

    // A peek came back. The columns view peeks the pane's ancestors on the same wire, and a Tab that
    // gained a dot asks the same directory again with the other hidden flag, so the reply is matched
    // on the pair the backend echoes rather than on the path alone: a line completing ".conf" was
    // otherwise free to answer off rows that carried no dotfiles at all. Only directories are kept:
    // this bar goes to a directory, and a name that cannot be opened is not a completion.
    function completeWith(dir, hidden, rows) {
        if (PathBar.requestKey(dir, hidden) !== root.pendingKey) {
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

    // A chrome strip, not a data row; see Theme.qml's chromeHeight comment.
    implicitHeight: Theme.chromeHeight

    Rectangle {
        anchors.fill: parent
        color: Theme.color.surface
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

    // Where the path is drawn is where it is typed. A double click opens the bar, which is the
    // pointer's half of ":" and Ctrl+L. Issue 45 made the segments above the leaf a control as well
    // as a label: one tap on any of them opens that directory, and the leaf is where the pane
    // already is, so it stays a label and only the bar answers a click on it.
    Item {
        id: pathArea
        anchors.left: nav.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: views.left
        anchors.rightMargin: Theme.spacing.gap
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        // The tail identifies the directory, so a path too long for the bar loses its head: the row
        // slides left inside a clipped slot, which is the left elision the single Text drew, made of
        // pieces a click can land on.
        Item {
            id: crumbSlot
            visible: !root.editing
            anchors.fill: parent
            clip: true

            Row {
                id: crumbRow
                anchors.verticalCenter: parent.verticalCenter
                x: Math.min(0, crumbSlot.width - crumbRow.width)

                Repeater {
                    id: crumbs
                    model: Nav.crumbs(root.path, root.home)

                    // corner: a path is arbitrary text, so PlainText, the same rule every filename on this surface follows.
                    delegate: Text {
                        id: crumb
                        required property var modelData
                        text: crumb.modelData.text
                        color: crumb.modelData.last ? Theme.color.foreground : Theme.color.muted
                        font.family: Theme.font.family
                        font.pixelSize: Theme.font.caption
                        textFormat: Text.PlainText
                        // The box is the strip's height with the glyphs centred in it, because the
                        // handlers below are the path area's whole gesture and a text-tall box left
                        // 11 of the strip's 27 px dead, measured at the window.
                        height: crumbSlot.height
                        verticalAlignment: Text.AlignVCenter

                        HoverHandler {
                            cursorShape: crumb.modelData.last ? Qt.IBeamCursor : Qt.PointingHandCursor
                        }

                        // Both flags together, measured on Qt 6.11.2: one of them alone suppresses
                        // the other signal instead of waiting, and only the pair makes the tap count
                        // decide, so a double click types the path rather than also navigating.
                        // The gesture is on the crumb and not on the strip because a TapHandler on a
                        // parent item takes the second tap away from the child under the pointer.
                        TapHandler {
                            acceptedButtons: Qt.LeftButton
                            exclusiveSignals: TapHandler.SingleTap | TapHandler.DoubleTap
                            onSingleTapped: if (!crumb.modelData.last) root.pathEntered(crumb.modelData.path)
                            onDoubleTapped: root.startEdit()
                        }
                    }
                }
            }

            // The rest of the line, which names no directory and so keeps the plain caret and the
            // one gesture the whole strip used to carry. It is empty once the path fills the bar.
            Item {
                id: typeArea
                anchors.left: crumbRow.right
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom

                HoverHandler {
                    cursorShape: Qt.IBeamCursor
                }

                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    onDoubleTapped: root.startEdit()
                }
            }

            // The head that ran off the left, marked where the elided Text drew its own ellipsis; the
            // fill behind it is the chrome's own colour, because the crumbs slide underneath it. It
            // is the marker's box, and that box is the strip's height for the same reason a crumb's is.
            Rectangle {
                visible: elision.visible
                anchors.fill: elision
                color: Theme.color.surface

                // The crumbs slide under this fill, so without a gesture of its own a press here
                // opened whichever one had scrolled behind it, a directory nobody could see. A
                // MouseArea and not a TapHandler: the default DragThreshold policy takes a passive
                // grab, so the crumb underneath still tapped, measured on the box.
                MouseArea {
                    anchors.fill: parent
                    onDoubleClicked: root.startEdit()
                }
            }

            Text {
                id: elision
                visible: crumbRow.width > crumbSlot.width
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                verticalAlignment: Text.AlignVCenter
                text: "\u2026"
                color: Theme.color.muted
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }
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

    // The strip's own bottom edge, declared last so it draws over the path area: the elided head's
    // opaque fill reaches the same row and used to leave a seven pixel gap in it.
    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }
}
