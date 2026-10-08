import QtQuick
import qs.Commons
import "." as Flea
import "js/Crumbs.js" as Crumbs
import "js/PathBar.js" as PathBar

// The window's top chrome, per the canvas: where you are on the left, how you are looking at it on
// the right. The path lives here rather than in the status bar, which the design gives to counts.
Item {
    id: root

    property string path: ""
    // True only when the drawn path's listing failed, so retyping it retries and all else is a no-op.
    property bool pathFailed: false
    property string home: ""
    property bool canGoBack: false
    property bool canGoUp: false
    // "list", "columns" or "grid"; the button naming the current one takes the accent.
    property string viewMode: "list"
    property bool showPath: true
    // Read by the path bar alone, so a Tab on a dotted leaf peeks the way the listing is set to look.
    property bool showHidden: false
    property bool canFilter: false
    property bool canSort: false
    // False while Quick Look is open: Qt 6 still hands a press beneath an accepted shield, so only handlers stop, never Item.enabled.
    property bool inputLive: true

    signal backRequested()
    signal upRequested()
    signal searchRequested()
    signal filterRequested()
    signal sortRequested()
    signal viewChosen(string mode)
    // The path bar's four. ui/shell.qml navigates, hands the keyboard back, runs the peek behind Tab
    // and carries what the bar says to the status line, because this file draws the chrome and knows
    // nothing about the backend, the pane or the bar below it.
    signal pathEntered(string path)
    signal editClosed()
    signal completeRequested(string dir, bool hidden)
    signal said(string text)
    // The folder jump's one read per open, carried to the pane's backend like Tab's peek; see ui/PathJump.qml.
    signal jumpRequested(int id, int ranking, var favourites, var recent)
    // Built by the first edit and kept for the window's life, so a bar never opened compiles none of it.
    property bool jumpBuilt: false
    readonly property var jump: jumpLoader.item
    readonly property bool jumpShown: jumpLoader.item !== null && jumpLoader.item.shown
    // Its dropdown hangs below the strip, so the strip rises over the tab bar and the listing while it shows.
    z: root.jumpShown ? 1 : 0
    // The settings panel's pointer door, beside the comma key; see ui/shell.qml for the third.
    signal settingsRequested()

    // The path bar: the same strip, typed instead of drawn. ":" and Ctrl+L open it, so does a double
    // click on the path, and it is where the whole of keys.toml's pathBar action lands.
    property bool editing: false
    // What the seam reads: the line as it stands, and the box a test double-clicks to open the bar.
    readonly property alias editText: field.text
    readonly property alias pathArea: pathArea
    // The open field and its frame, so a test measures where they lie in the strip.
    readonly property alias pathFrame: editFrame
    readonly property alias pathField: field
    // The collapsed middle's own crumb, so a test can press the one segment that names no directory. Its index moves with the room, because the crumbs nearest the root are put back before it.
    readonly property int elisionIndex: {
        for (var i = 0; i < crumbs.model.length; i++)
            if (crumbs.model[i].elided)
                return i
        return -1
    }
    // itemAt is a call and not a property, so the model is named here too: without it a width that keeps the same index hands back the item the model before it built, which is by then destroyed.
    readonly property var elisionMarker: root.elisionIndex >= 0 && crumbs.model ? crumbs.itemAt(root.elisionIndex) : null
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
        // The Loader below is synchronous, so the jump stands before this returns.
        root.jumpBuilt = true
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
        // An empty line closes the bar, and a settled path stays a no-op to keep the selection, while a failed or locked one retries.
        if (PathBar.shouldNavigate(target, root.path, root.pathFailed)) {
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
        color: Glass.surfacePlane
    }

    // A test drives these by coordinate, because a glyph button carries no text to find on screen.
    function buttonFor(glyph) {
        var groups = [nav, views, modes]
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
            inputLive: root.inputLive
            glyph: "arrow-left"
            enabled: root.canGoBack
            onActivated: root.backRequested()
        }

        Flea.ChromeButton {
            inputLive: root.inputLive
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
        // The strip draws its own rule along that bottom edge, so the content stops above it: without
        // this the rename editor's frame sat two pixels clear of the strip's top and one of its rule,
        // which is the same unequal margin the Trash chrome's control had.
        anchors.bottomMargin: Theme.spacing.hairline

        // The tail identifies the directory, so a path too long for the bar loses its head: the row
        // slides left inside a clipped slot, which is the left elision the single Text drew, made of
        // pieces a click can land on.
        Text {
            anchors.fill: parent
            visible: !root.editing && root.showPath && ViewState.addressBar === "path"
            text: root.home && (root.path === root.home || root.path.indexOf(root.home + "/") === 0)
                  ? "~" + root.path.substring(root.home.length) : root.path
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideLeft
            textFormat: Text.PlainText
            TapHandler { enabled: root.inputLive; onDoubleTapped: root.startEdit() }
        }

        // One glyph's advance is every glyph's advance in this face, so the crumbs are fitted by
        // character count rather than by a layout pass that would feed its own width back in.
        TextMetrics {
            id: crumbMetrics
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            text: "0"
        }

        Item {
            id: crumbSlot
            visible: !root.editing && root.showPath && ViewState.addressBar === "breadcrumb"
            anchors.fill: parent
            clip: true

            Row {
                id: crumbRow
                anchors.verticalCenter: parent.verticalCenter

                Repeater {
                    id: crumbs
                    model: Crumbs.fitCrumbs(Crumbs.crumbs(root.path, root.home),
                                         Math.floor(crumbSlot.width / crumbMetrics.advanceWidth))

                    delegate: Flea.Crumb {
                        inputLive: root.inputLive
                        // The strip's height with the glyphs centred, because the crumb's handlers are the path area's
                        // whole gesture and a text-tall box left 11 of the strip's 27 px dead, measured at the window.
                        height: crumbSlot.height
                        onChosen: function (path) { root.pathEntered(path) }
                        onEditRequested: root.startEdit()
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
                    enabled: root.inputLive
                    cursorShape: Qt.IBeamCursor
                }

                TapHandler {
                    enabled: root.inputLive
                    acceptedButtons: Qt.LeftButton
                    onDoubleTapped: root.startEdit()
                }
            }

        }

        // The rename editor's own frame, at chrome scale: the accent marks the strip with the keyboard, and its fill covers the two Texts beneath.
        Rectangle {
            id: editFrame
            visible: root.editing
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.hairline * 2
            // One below against two above centres the field in the strip, which is the flush dropdown the Jump board draws.
            anchors.bottomMargin: Theme.spacing.hairline
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
            // The jump's dropdown takes the arrows and Enter first, and only while it is showing rows.
            Keys.forwardTo: root.jump !== null ? [root.jump] : []

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

        // The Jump board's dropdown, flush under this field at its width from the path slot its Loader fills.
        Loader {
            id: jumpLoader
            anchors.fill: parent
            active: root.jumpBuilt
            asynchronous: false
            source: "PathJump.qml"
        }
        // Live bindings onto the lazy item, applied once it stands and kept after the bar closes.
        Binding {
            target: jumpLoader.item
            property: "editing"
            value: root.editing
            when: jumpLoader.item !== null
        }
        Binding {
            target: jumpLoader.item
            property: "query"
            value: field.text
            when: jumpLoader.item !== null
        }
        Binding {
            target: jumpLoader.item
            property: "home"
            value: root.home
            when: jumpLoader.item !== null
        }
        Connections {
            target: jumpLoader.item
            function onRequested(id, ranking, favourites, recent) { root.jumpRequested(id, ranking, favourites, recent) }
            function onDeclined() { root.commitEdit() }
            function onDismissed() { root.closeEdit() }
            function onChosen(path) { root.closeEdit(); if (PathBar.shouldNavigate(path, root.path, root.pathFailed)) root.pathEntered(path) }
        }
    }

    Row {
        id: views
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.spacing.gap

        Flea.ChromeButton {
            inputLive: root.inputLive
            visible: root.viewMode !== "grid"
            glyph: "search"
            onActivated: root.searchRequested()
        }

        Flea.ChromeButton {
            inputLive: root.inputLive
            visible: root.viewMode === "grid"
            enabled: root.canFilter
            glyph: "filter"
            accessName: "Filter"
            onActivated: root.filterRequested()
        }

        Flea.ChromeButton {
            inputLive: root.inputLive
            visible: root.viewMode === "grid"
            enabled: root.canSort
            glyph: "sort"
            accessName: "Sort"
            onActivated: root.sortRequested()
        }

        Flea.ChromeModes {
            id: modes
            inputLive: root.inputLive
            viewMode: root.viewMode
            onChosen: function (mode) { root.viewChosen(mode) }
        }

        // Row owns horizontal placement; the short Settings divider stays vertically centered.
        Rectangle {
            width: Theme.spacing.hairline
            height: Theme.chromeMarkSize - Theme.spacing.hairline
            y: (parent.height - height) / 2
            color: Theme.color.foreground
            opacity: 0.12
        }

        Flea.ChromeButton {
            inputLive: root.inputLive
            glyph: "sliders"
            onActivated: root.settingsRequested()
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

    // The pointer's title bar over the whole strip; the controls under it keep their own taps.
    Flea.WindowDrag { anchors.fill: parent; editing: root.editing; inputLive: root.inputLive }
}
