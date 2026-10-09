import QtQuick
import Quickshell
import qs.Commons
import "js/Columns.js" as Columns
import "js/Drag.js" as DragOps
import "js/Emblem.js" as Emblem
import "js/Format.js" as Format
import "js/Icons.js" as Icons
import "js/Match.js" as Match
import "js/Recent.js" as Recent
import "." as Flea

Item {
    id: root

    property var row: null
    function dateStamp() { return root.row ? (root.row.used === undefined ? root.row.m : root.row.used) : null }
    property bool cursor: false
    property bool paneFocused: true
    property bool dualMode: false
    readonly property real markSlot: root.dualMode ? Theme.markSize : Theme.iconSize
    readonly property real sizeWidth: root.dualMode ? Theme.dualColumn.size : Theme.column.size
    property bool hovered: false
    property string thumb: ""
    property bool selected: false
    // List.qml derives it per visible row, so an empty clipboard costs nothing.
    property string clipMark: ""
    // The mark's own size: 12 px in the muted role, 9 px after the name, per the board.
    readonly property int clipPx: 12
    // The per-response dictionary row.k indexes into; List.qml hands down the same array every row of one response shares.
    property var kindNames: []
    // The row is its own rename editor while this is true, per the States artboard.
    property bool renaming: false
    property var renamePane: null
    // The folder under a drag right now, per States.dc.html "Drop target"; List.qml's delegate binds it.
    property bool dropTarget: false
    // Whether that drop would copy, so the label can say which; the status bar says the rest.
    property bool dropCopying: false
    property bool dropLinking: false
    // A directory's recursive size, resolved by index in List.qml the same way thumb already is; null until it arrives.
    property var dirSize: null
    // The picker's rows start one slot further in for its check; the window's own leave this at zero.
    property real leadingSlot: 0
    // The picker's second difference: SendPicker.html's narrow date column and its compact form.
    property bool compactDate: false
    property bool foregroundMetadata: false
    readonly property real dateWidth: root.dualMode ? Theme.dualColumn.date : root.compactDate ? Theme.column.pickerDate : Theme.column.date
    // The picker's third: it hides the columns its own board does not draw, and the window's own set stays ViewState's.
    property var hiddenCols: ViewState.hiddenCols
    // Non-empty while a search or filter is narrowing the listing: the run to paint, and the switch to the search column set.
    property string searchQuery: ""
    // Which of the two is narrowing. A filter keeps the ordinary columns, because its rows are this directory's own and their names are plain names, not paths.
    property bool filtering: false
    // Search and Recent split the path into a base name and location; see docs/protocol.md "search".
    property bool recenting: false
    readonly property bool searching: !root.filtering && root.searchQuery.length > 0 && root.row !== null && root.row.n.length > 0
    readonly property bool locating: root.searching || root.recenting
    readonly property string displayName: root.row ? (root.locating ? Match.base(root.row.n) : root.row.n) : ""
    // FleaWindow.html and ThemeRoles.html both spell it "shell -> /usr/share/omarchy".
    readonly property string linkMark: root.row && root.row.l ? " -> " + root.row.l : ""
    // The name, then a link's target; a folder carries no slash, its glyph and the folders-first order already say it.
    readonly property string decoratedName: root.displayName + root.linkMark
    readonly property string locationText: root.locating && root.row ? (root.recenting ? Recent.locationUnder(root.row.n, Quickshell.env("HOME") || "") : Match.location(root.row.n)) : ""
    readonly property var nameRun: Match.run(root.displayName, root.searchQuery)
    // Assigned by List.qml's shared budgets; -2 keeps the local geometry default for PickerList and drop-target rows.
    property int assignedNameBudget: -2
    property real paintWidth: 0 // Caller view width; fills paint to it under the lane, content keeps rowWidth.
    // Local measured geometry for PickerList, drop targets and unlaid rows; a function so ordinary rows pay no floor.
    function localNameBudget() { return Theme.bodyAdvance > 0 && name.width > 0 ? Math.floor(name.width / Theme.bodyAdvance) : -1 }
    // Ordinary List rows share; drop targets keep local measured geometry with the label's own reserve.
    readonly property int nameBudget: root.dropTarget ? root.localNameBudget() : (root.assignedNameBudget > -2 ? root.assignedNameBudget : root.localNameBudget())
    readonly property string elidedName: root.nameRun.start < 0 && root.nameBudget >= 0 ? Format.middleElide(root.decoratedName, root.nameBudget) : root.decoratedName
    // A long name would otherwise hide the location entirely, and the location is what tells two matches apart.
    readonly property real nameShare: 0.66
    // What the name and location share: the row less its padding, the mark, their own gap, and the size column while it is drawn.
    readonly property real searchSlot: Math.max(0, root.width - 2 * Theme.spacing.rowPaddingX - root.markSlot - 2 * Theme.spacing.gap
                                                - (root.sizeShown ? root.sizeWidth + Theme.spacing.gap : 0))

    // The columns this row's width affords. A column that is not drawn takes neither its width nor its gap, so the chain collapses onto its right neighbour.
    property var assignedCols: null // Set by List.qml; null keeps the local default below.
    readonly property var cols: root.assignedCols !== null ? root.assignedCols : root.recenting ? Theme.columns(root.width, root.hiddenCols, root.dateWidth, true, root.dualMode) : root.dualMode ? Theme.dualColumns(root.width, root.hiddenCols) : Theme.columns(root.width, root.hiddenCols, root.dateWidth)
    readonly property bool modeShown: !root.locating && root.cols.mode
    readonly property bool sizeShown: root.cols.size
    readonly property bool dateShown: (!root.searching || root.recenting) && root.cols.date
    readonly property bool kindShown: !root.locating && root.cols.kind

    // A lifted row is the cursor, the pointer, or a selection member; all three take the same fill treatment, per qui Minimal.
    property bool lifted: root.cursor || root.hovered || root.selected || root.dropTarget
    // The OEM derives its secondary ink from the foreground rather than reading a separate palette key.
    readonly property color dim: Qt.darker(Theme.color.foreground, 1.4)
    // One ink serves four cells, lifts metadata to foreground and carries cut dim without child opacity.
    readonly property color cellInk: Util.alpha(root.lifted || root.foregroundMetadata ? Theme.color.foreground : root.dim, root.dimOpacity)
    // A cut row dims its content, never the state fills, and every dimmed child reads this one value.
    readonly property real dimOpacity: root.clipMark === "scissors" ? Theme.disabledOpacity : 1
    // One helper, so each dimmed colour multiplies the cut without a second binding.
    function dimmed(base) { return Util.alpha(base, root.dimOpacity) }
    // A thumbnail path is not a thumbnail: the cache file can be evicted between the pane's answer
    // and the decode, and a row whose Image failed to load has to be marked by its kind instead.
    readonly property bool thumbDrawn: root.thumb.length > 0 && thumbImage.status !== Image.Error

    // The editor sits inside the row's own height at every density; only an error line below it adds to it.
    implicitHeight: root.renaming && renameLoader.item ? Theme.fileRowHeight + renameLoader.item.errorHeight : Theme.fileRowHeight
    implicitWidth: parent ? parent.width : 0

    Accessible.role: Accessible.ListItem
    Accessible.name: root.displayName
    // The compact form drops the clock, so the picker's rows carry the whole stamp here instead; this tree has no tooltip.
    Accessible.description: root.compactDate && root.dateStamp() !== null ? Format.date(root.dateStamp()) : ""

    Rectangle {
        width: root.paintWidth > 0 ? root.paintWidth : parent.width
        height: parent.height
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

    // One Loader for the drop frame, the drop label and the clip mark, so a row at rest builds none.
    Loader {
        id: dropLoader
        active: root.dropTarget || (root.clipMark.length > 0 && !root.renaming && !root.searching)
        anchors.fill: parent
        sourceComponent: Item {
            property alias label: dropLabel
            property alias mark: clipGlyph
            anchors.fill: parent
            // The board's accent hairline over a faint wash at the hover rung's alpha: the token wins over the mock's own 0.07.
            Rectangle {
                visible: root.dropTarget
                width: root.paintWidth > 0 ? root.paintWidth : parent.width
                height: parent.height
                color: Util.alpha(Theme.color.accent, Style.hoverFillAlpha)
                border.width: Theme.spacing.hairline
                border.color: Theme.color.accent
            }
            // The board's own words in the columns' place: caption type in the accent, against the row padding.
            Text {
                id: dropLabel
                visible: root.dropTarget
                anchors.right: parent.right
                anchors.rightMargin: Theme.spacing.rowPaddingX
                anchors.verticalCenter: parent.verticalCenter
                text: DragOps.label(root.dropCopying, root.dropLinking)
                color: Theme.color.accent
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                textFormat: Text.PlainText
            }
            // A clipboard row under a drag keeps its mark beside the label, one pixel above the text centre line.
            Glyph {
                id: clipGlyph
                visible: root.clipMark.length > 0 && !root.renaming && !root.searching
                width: root.clipPx
                height: root.clipPx
                x: root.clipMark.length > 0 ? root.nameItem().x + Math.min(root.nameItem().implicitWidth, root.nameItem().width) + Theme.spacing.gap : 0
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: -1
                name: root.clipMark
                color: Theme.color.muted
            }
        }
    }

    // A thumbnail is a decoded image and stays one; the icon beside it is a native mark. The two
    // share this slot and exactly one is visible, chosen by whether the pane holds a thumbnail path.
    Image {
        id: thumbImage
        visible: root.thumbDrawn
        opacity: root.dimOpacity
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
        color: root.dimmed(root.dualMode ? (root.cursor && root.paneFocused ? Theme.color.accent : Theme.color.muted) : root.lifted ? Theme.color.foreground : root.dim)
    }

    // The sync badge over the icon, made only on a row a sync tool tagged, see js/Emblem.js.
    property Item emblemItem: null
    function syncEmblem() { root.emblemItem = Emblem.sync(root.emblemItem, Qt.resolvedUrl("EmblemBadge.qml"), root, { targetIcon: icon, status: root.row && root.row.e ? root.row.e : "" }) }
    onRowChanged: root.syncEmblem()
    Component.onCompleted: root.syncEmblem()

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
    function commitEditor() { return renameLoader.item ? renameLoader.item.commit() : false }

    // What the editor holds right now, for tests/ui.sh through ui/Ipc.qml's renameEditorText.
    readonly property string editorText: renameLoader.item ? renameLoader.item.current : ""
    readonly property Item editorField: renameLoader.item as Item

    signal renameCommitted(string newName)
    signal renameAbandoned()

    // Built only while renaming; a Loader destroys it without a hide, so RenameField's begun guard owns the abandon.
    Loader {
        id: renameLoader
        active: root.renaming
        // The label slot on whole pixels: a dual pane's width and mark slot are fractional, and the frame is a hairline.
        x: Math.round(icon.x + icon.width) + Theme.spacing.gap
        y: Math.round((root.height - renameLoader.height) / 2)
        width: Math.round(mode.x - (root.modeShown ? Theme.spacing.gap : 0)) - x
        sourceComponent: Flea.RenameField {
            height: implicitHeight
            pane: root.renamePane
            viewport: root.ListView.view
            name: root.displayName
            onCommitted: function (newName) { root.renameCommitted(newName) }
            onAbandoned: root.renameAbandoned()
        }
    }

    // corner: a filename is arbitrary text, so PlainText everywhere; MatchText draws its runs the same way.
    MatchText {
        id: name
        visible: !root.locating && !root.renaming
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: mode.left
        anchors.rightMargin: (root.modeShown ? Theme.spacing.gap : 0) + (root.clipMark.length > 0 ? Theme.spacing.gap + root.clipPx : 0) + (root.dropTarget ? root.dropExtra() : 0)
        anchors.verticalCenter: parent.verticalCenter
        text: root.elidedName
        matchStart: root.nameRun.start
        matchLength: root.nameRun.length
        color: root.dimmed(root.nameColor())
        accent: root.dimmed(Theme.color.accent)
        elideMiddle: true
    }

    // One lazy path split: Recent reserves Location; search keeps the inline name share.
    Loader {
        id: locatingLoader
        active: root.locating
        anchors.left: icon.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.right: size.left
        anchors.rightMargin: root.sizeShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        sourceComponent: Item {
            readonly property var owner: parent.parent
            property alias location: location
            MatchText {
                id: searchName
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                width: root.recenting ? Math.max(0, parent.width - (parent.owner.cols.location ? Theme.column.location + Theme.spacing.gap : 0)) : Math.min(implicitWidth, root.searchSlot * parent.owner.nameShare)
                text: root.decoratedName
                matchStart: root.nameRun.start
                matchLength: root.nameRun.length
                color: root.dimmed(root.nameColor())
                accent: root.dimmed(Theme.color.accent)
                // A search base name keeps its extension the same way a list name does.
                elideMiddle: true
            }

            Text {
                id: location
                visible: !parent.owner.recenting || parent.owner.cols.location
                anchors.left: searchName.right
                anchors.leftMargin: parent.owner.recenting && !parent.owner.cols.location ? 0 : Theme.spacing.gap
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: root.locationText
                color: root.cellInk
                font.family: Theme.font.family
                font.pixelSize: Theme.font.caption
                elide: Text.ElideLeft
                textFormat: Text.PlainText
            }
        }
    }

    // The four metadata cells bind straight to the row; a hidden column holds no text, because a laid-out Text costs memory drawn or not.
    Text {
        id: mode
        anchors.right: size.left
        anchors.rightMargin: root.sizeShown && !root.dualMode ? Theme.spacing.gap : 0
        anchors.verticalCenter: parent.verticalCenter
        visible: root.modeShown && !root.dropTarget
        width: root.modeShown ? Theme.column.mode : 0
        text: root.modeShown && root.row ? Format.permissions(root.row.p) : ""
        color: root.cellInk
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
        text: root.sizeShown && root.row ? root.sizeText() : ""
        color: root.cellInk
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
        text: root.dateShown ? root.dateText() : ""
        // Short-circuit on the switch first, so with the switch off no row enters the library.
        color: (root.dateShown && ViewState.highlightToday && Format.isRecent(root.dateStamp(), ViewState.todayStart)) ? root.dimmed(Theme.color.foreground) : root.cellInk
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
        text: root.kindShown && root.row ? root.kindText() : ""
        color: root.cellInk
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        elide: Text.ElideRight
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

    // The window's one stamp, or the picker's compact three; both are cell text and nothing more.
    function dateText() {
        if (!root.row) {
            return ""
        }
        var stamp = root.dateStamp()
        if (stamp === null) {
            return "--"
        }
        return root.compactDate ? Format.compactDate(stamp) : Format.date(stamp)
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

    // The drawn columns and cells, shared with Header's geometry seam.
    function columnSet() { return Columns.names(root.cols) }
    function cell(key) {
        switch (key) {
        case "location": return locatingLoader.item ? locatingLoader.item.location : null
        case "mode": return mode
        case "size": return size
        case "date": return modified
        case "kind": return kind
        }
        return null
    }

    // Test seams as functions, so no row at rest binds to drop geometry.
    function dropBuilt() { return root.dropTarget && dropLoader.item !== null }
    function dropLabelLeft() { return root.dropTarget && dropLoader.item ? dropLoader.item.label.x : -1 }
    function dropLabelText() { return root.dropTarget && dropLoader.item ? dropLoader.item.label.text : "" }
    function nameRight() { return name.x + name.width }
    // The clip mark's right edge, so the drop phase reads the drawn mark and not the name.
    function clipRight() { var m = dropLoader.item ? dropLoader.item.mark : null; if (!m || !m.visible || m.width <= 0) return -1; return m.x + m.width }
    // The drawn name and mark, so a dim check reads colours and not an opacity.
    function nameItem() { return name }
    function markColor() { return icon.color }
    // Extra beyond the column gap, so a fitting name keeps its width while a long one clears the label.
    function dropExtra() {
        if (!root.dropTarget || !dropLoader.item) return 0
        var labelW = dropLoader.item.label.implicitWidth
        var hidden = root.width - Theme.spacing.rowPaddingX - mode.x
        var base = root.modeShown ? Theme.spacing.gap : 0
        var need = labelW + Theme.spacing.gap - hidden - base
        return need > 0 ? need : 0
    }
}
