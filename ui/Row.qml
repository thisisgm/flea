import QtQuick
import qs.Commons
import "js/Drag.js" as DragOps
import "js/Format.js" as Format
import "js/Icons.js" as Icons
import "js/Match.js" as Match
import "." as Flea

Item {
    id: root

    property var row: null
    property bool cursor: false
    property bool paneFocused: true
    property bool dualMode: false
    readonly property real markSlot: root.dualMode ? Theme.markSize : Theme.iconSize
    readonly property real sizeWidth: root.dualMode ? Theme.dualColumn.size : Theme.column.size
    property bool hovered: false
    property string thumb: ""
    property bool selected: false
    // The per-response dictionary row.k indexes into; List.qml hands down the same array every row of one response shares.
    property var kindNames: []
    // The row is its own rename editor while this is true, per the States artboard.
    property bool renaming: false
    property var renamePane: null
    // The folder under a drag right now, per States.dc.html "Drop target"; List.qml's delegate binds it.
    property bool dropTarget: false
    // Whether that drop would copy, so the label can say which; the status bar says the rest.
    property bool dropCopying: false
    // A directory's recursive size, resolved by index in List.qml the same way thumb already is; null until it arrives.
    property var dirSize: null
    // The picker draws a check in front of every row, so its rows start one slot further in; the
    // window's own rows leave this at zero and are laid out exactly as before.
    property real leadingSlot: 0
    // The picker's second difference: SendPicker.html's narrow date column and its compact form.
    property bool compactDate: false
    property bool foregroundMetadata: false
    // The window's own third: only FleaWindow.html and Search.html end a directory name with a slash.
    property bool dirSuffix: false
    readonly property real dateWidth: root.dualMode ? Theme.dualColumn.date : root.compactDate ? Theme.column.pickerDate : Theme.column.date
    // The picker's third: it hides the columns its own board does not draw, and the window's own set stays ViewState's.
    property var hiddenCols: ViewState.hiddenCols
    // Non-empty while a search or filter is narrowing the listing: the run to paint, and the switch to the search column set.
    property string searchQuery: ""
    // Which of the two is narrowing. A filter keeps the ordinary columns, because its rows are this directory's own and their names are plain names, not paths.
    property bool filtering: false
    // A search row's name is its path relative to the search root, so the name and location split here; see docs/protocol.md "search".
    readonly property bool searching: !root.filtering && root.searchQuery.length > 0 && root.row !== null && root.row.n.length > 0
    readonly property string displayName: root.row ? (root.searching ? Match.base(root.row.n) : root.row.n) : ""
    // The name, then this surface's directory slash, then a link's target; never both, a link's d is false.
    readonly property string dirMark: root.dirSuffix && root.row && root.row.d ? "/" : ""
    // FleaWindow.html and ThemeRoles.html both spell it "shell -> /usr/share/omarchy".
    readonly property string linkMark: root.row && root.row.l ? " -> " + root.row.l : ""
    readonly property string decoratedName: root.displayName + root.dirMark + root.linkMark
    readonly property string locationText: root.searching ? Match.location(root.row.n) : ""
    readonly property var nameRun: Match.run(root.displayName, root.searchQuery)
    // A long name would otherwise hide the location entirely, and the location is what tells two matches apart.
    readonly property real nameShare: 0.66
    // What the name and location share: the row minus its padding, the mark, the gap between the
    // two of them, and the size column while it is still being drawn.
    readonly property real searchSlot: Math.max(0, root.width - 2 * Theme.spacing.rowPaddingX - root.markSlot - 2 * Theme.spacing.gap
                                                - (root.sizeShown ? root.sizeWidth + Theme.spacing.gap : 0))

    // The columns this row's width affords, and which of them this row is drawing. A column that
    // is not drawn takes neither its width nor its gap, so the chain collapses onto the one to its
    // right and the name takes back the whole of it.
    readonly property var cols: root.dualMode ? Theme.dualColumns(root.width, root.hiddenCols) : Theme.columns(root.width, root.hiddenCols, root.dateWidth)
    readonly property bool modeShown: !root.searching && root.cols.mode
    // The search column set keeps Size and drops the other three, so only this one ignores searching.
    readonly property bool sizeShown: root.cols.size
    readonly property bool dateShown: !root.searching && root.cols.date
    readonly property bool kindShown: !root.searching && root.cols.kind

    // A lifted row is the cursor, the pointer, or a selection member; all three take the same fill treatment, per qui Minimal.
    property bool lifted: root.cursor || root.hovered || root.selected || root.dropTarget
    // The OEM derives its secondary ink from the foreground rather than reading a separate palette key.
    readonly property color dim: Qt.darker(Theme.color.foreground, 1.4)
    // A thumbnail path is not a thumbnail: the cache file can be evicted between the pane's answer
    // and the decode, and a row whose Image failed to load has to be marked by its kind instead.
    readonly property bool thumbDrawn: root.thumb.length > 0 && thumbImage.status !== Image.Error

    implicitHeight: root.renaming ? Math.max(Theme.fileRowHeight, editor.implicitHeight + 2 * Theme.spacing.rowPaddingY) : Theme.fileRowHeight
    implicitWidth: parent ? parent.width : 0

    Accessible.role: Accessible.ListItem
    Accessible.name: root.displayName
    // The compact form drops the clock, so the picker's rows carry the whole stamp here instead; this tree has no tooltip.
    Accessible.description: root.compactDate && root.row && root.row.m !== null ? Format.date(root.row.m, Date.now()) : ""

    Rectangle {
        anchors.fill: parent
        // The cursor uses accent ink; marked rows keep the OEM's distinct selection rung.
        color: root.cursor && root.paneFocused ? Style.selectedAccentFill
             : root.selected && root.paneFocused ? Style.selectionFill
             : root.hovered ? Style.hoverFill
             : "transparent"
    }

    // Twice the hairline, thick enough to read as a cursor mark against the row edge.
    Rectangle {
        visible: root.cursor
        width: Theme.spacing.hairline * 2
        height: parent.height
        color: root.paneFocused ? Theme.color.accent : Theme.color.muted
    }

    // The drop frame: the board's hairline of accent inset in the row over a faint accent wash, the
    // wash at the hover rung's alpha because the token wins over the mock's own 0.07.
    Rectangle {
        visible: root.dropTarget
        anchors.fill: parent
        color: Util.alpha(Theme.color.accent, Style.hoverFillAlpha)
        border.width: Theme.spacing.hairline
        border.color: Theme.color.accent
    }

    // A thumbnail is a decoded image and stays one; the icon beside it is a native mark. The two
    // share this slot and exactly one is visible, chosen by whether the pane holds a thumbnail path.
    Image {
        id: thumbImage
        visible: root.thumbDrawn
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX + root.leadingSlot
        anchors.verticalCenter: parent.verticalCenter
        width: root.markSlot
        height: root.markSlot
        // Sized on purpose, see AGENTS.md "The thumbnail decode arm": it caps the themed icon and saves 158 KB a thumbnail.
        sourceSize.width: root.markSlot
        sourceSize.height: root.markSlot
        fillMode: Image.PreserveAspectFit
        // A synchronous decode on the UI thread would land inside a scrolled frame.
        asynchronous: true
        source: root.iconSource()
    }

    Glyph {
        id: icon
        visible: !root.thumbDrawn
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX + root.leadingSlot
        anchors.verticalCenter: parent.verticalCenter
        width: root.markSlot
        height: root.markSlot
        name: root.row ? Icons.glyphForRow(root.row.i, root.row.p) : Icons.FALLBACK
        color: root.dualMode ? (root.cursor && root.paneFocused ? Theme.color.accent : Theme.color.muted)
            : root.lifted ? Theme.color.foreground : root.dim
    }

    // What the row actually draws, so a test catches the binding being cut and not only the lookup.
    readonly property alias iconUrl: thumbImage.source
    // Whether it actually opened, because a URL a test can read is not proof that Qt could load it.
    readonly property alias iconStatus: thumbImage.status
    readonly property int namePx: name.pixelSize
    // The thumbnail's own box, for ui/Ipc.qml's rowThumbRect: pixels are counted inside it and not in the name.
    readonly property Item thumbItem: thumbImage
    // The glyph name actually bound, the same alias idiom as iconUrl, for the icon-path test case.
    readonly property alias glyphName: icon.name

    // ui/List.qml's click-away commit reaches the open editor through this, and reads back whether
    // a commit really happened: an abandon must not leave renameKeepsPointerRow standing.
    function commitEditor() { return editor.commit() }

    // What the editor holds right now, for tests/ui.sh through ui/Ipc.qml's renameEditorText.
    readonly property string editorText: editor.current
    readonly property Item editorField: editor

    signal renameCommitted(string newName)
    signal renameAbandoned()

    // The editor takes the name column's own box, so the row does not change shape when it opens.
    Flea.RenameField {
        id: editor
        visible: root.renaming
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: mode.left
        anchors.rightMargin: root.modeShown ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        height: implicitHeight
        pane: root.renamePane
        name: root.displayName
        onCommitted: function (newName) { root.renameCommitted(newName) }
        onAbandoned: root.renameAbandoned()
    }

    // corner: a filename is arbitrary text, so PlainText everywhere; MatchText draws its runs the same way.
    MatchText {
        id: name
        visible: !root.searching && !root.renaming
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: mode.left
        anchors.rightMargin: root.modeShown ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        text: root.decoratedName
        matchStart: root.nameRun.start
        matchLength: root.nameRun.length
        color: root.nameColor()
        accent: Theme.color.accent
    }

    // The search column set: the name shrinks to its content so the location beside it has room.
    MatchText {
        id: searchName
        visible: root.searching
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, root.searchSlot * root.nameShare)
        text: root.decoratedName
        matchStart: root.nameRun.start
        matchLength: root.nameRun.length
        color: root.nameColor()
        accent: Theme.color.accent
    }

    Text {
        id: location
        visible: root.searching
        anchors.left: searchName.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: size.left
        anchors.rightMargin: root.sizeShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        text: root.locationText
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideLeft
        textFormat: Text.PlainText
    }

    Text {
        id: mode
        anchors.right: size.left
        anchors.rightMargin: root.sizeShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.modeShown && !root.dropTarget
        width: root.modeShown ? Theme.column.mode : 0
        text: root.row ? Format.permissions(root.row.p) : ""
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: size
        anchors.right: modified.left
        anchors.rightMargin: root.dateShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.sizeShown && !root.dropTarget
        width: root.sizeShown ? root.sizeWidth : 0
        text: root.row ? root.sizeText() : ""
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: modified
        anchors.right: kind.left
        anchors.rightMargin: root.kindShown ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.dateShown && !root.dropTarget
        width: root.dateShown ? root.dateWidth : 0
        text: root.dateText()
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    Text {
        id: kind
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        visible: root.kindShown && !root.dropTarget
        width: root.kindShown ? Theme.column.kind : 0
        text: root.row ? root.kindText() : ""
        color: root.cellColor()
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
        textFormat: Text.PlainText
    }

    // The board's own words in the columns' place: caption type in the accent, against the row padding.
    Text {
        visible: root.dropTarget
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: DragOps.label(root.dropCopying)
        color: Theme.color.accent
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    // A row not yet fetched is dimmed rather than blank, so scrolling reads as loading.
    Rectangle {
        visible: root.row === null
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width * 0.3
        height: Theme.font.caption
        color: Theme.color.muted
        opacity: 0.2
    }

    // What the tests read, built from the same values the row renders, see AGENTS.md "Testing".
    function describe() {
        return root.row
            ? root.row.n + "|" + (root.row.d ? "dir" : "file") + "|" + Format.size(root.row.s)
            : "loading"
    }

    // The thumbnail Image's source; empty means the row draws its Glyph instead, see the slot above.
    function iconSource() {
        if (!root.row || root.thumb.length === 0) {
            return ""
        }
        // A regenerated thumbnail keeps its path, so the source mtime is the only thing that moves Qt's cache key.
        return Format.fileUri(root.thumb) + "?m=" + root.row.m
    }

    // A lifted row is a surface the theme never modelled, so its text takes the strongest ink; see the plan's Task 2 table.
    function cellColor() {
        return root.lifted || root.foregroundMetadata ? Theme.color.foreground : root.dim
    }

    // A directory's own row.s is its dirent size, not the walk's, so this reads root.dirSize instead, see docs/protocol.md "dirsized".
    function sizeText() {
        // A link's own st_size is the length of its target path, which is not a size anyone means.
        if (Format.isSymlink(root.row.p))
            return "link"
        if (!root.row.d) {
            return Format.size(root.row.s)
        }
        if (!root.dirSize) {
            return "·"
        }
        return (root.dirSize.partial ? ">" : "") + Format.size(root.dirSize.bytes)
    }

    // The window's own four forms, or the picker's compact three; both are cell text and nothing more.
    function dateText() {
        if (!root.row) {
            return ""
        }
        // null marks a row with no real mtime yet (ui/ShareBrowser.qml's share rows).
        if (root.row.m === null) {
            return "--"
        }
        return root.compactDate ? Format.compactDate(root.row.m, Date.now()) : Format.date(root.row.m, Date.now())
    }

    // row.k indexes root.kindNames; an index past its bounds (a row held over from an older listing) reads as empty, never a crash.
    function kindText() {
        if (!root.row || root.row.k === undefined) {
            return ""
        }
        var text = root.kindNames[root.row.k]
        return text !== undefined ? text : ""
    }

    // Semantic colour is reserved for symlink and executable, and a lifted row gives it up for contrast.
    function nameColor() {
        if (!root.row) {
            return Theme.color.muted
        }
        if (root.lifted) {
            return Theme.color.foreground
        }
        if (Format.isSymlink(root.row.p)) {
            return Theme.color.symlink
        }
        // corner: a directory carries the execute bits, so it claims foreground before that test.
        if (root.row.d) {
            return Theme.color.foreground
        }
        if (Format.isExecutable(root.row.p)) {
            return Theme.color.executable
        }
        return Theme.color.foreground
    }

    // What this row is drawing right now, for the seam that reads it beside the header's.
    function columnSet() { return Theme.columnNames(root.width, root.hiddenCols, root.dateWidth) }

    // The same by-key idiom Header.cell uses, so the overflow reader can reach a specific cell.
    function cell(key) {
        switch (key) {
        case "mode": return mode
        case "size": return size
        case "date": return modified
        case "kind": return kind
        }
        return null
    }
}
