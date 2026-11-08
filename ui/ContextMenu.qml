import QtQuick
import qs.Commons
import "." as Flea
import "js/Keymap.js" as Keymap
import "js/LockedMenu.js" as LockedMenu
import "js/Menu.js" as Menu
import "js/MenuContext.js" as MenuContext
import "js/MenuFit.js" as MenuFit
import "js/MenuRefresh.js" as MenuRefresh

// A plain overlay, not a QQC Popup: the one Controls import cost 10 ms of warm startup.
Item {
    id: root

    // Row actions, provider leaves and header choices share this signal, routed by Pane.
    signal chosen(string action)
    signal refused(string reason)
    signal noticed(string text)
    signal snapshotRequested()

    property bool opened: false
    property bool preparing: false
    // Driven from ui/Pane.qml's own state, so this file owns no hidden-file logic itself.
    property bool showHidden: false
    // [{id, label}], the reachable Taildrop targets; installed providers keep their disabled reason.
    property var taildropPeers: []
    property bool taildropInstalled: false
    property var localSend: ({ installed: false, peers: [], checking: false })
    property string taildropReason: ""
    property string taildropCode: ""
    property bool providersRefreshing: false
    property bool refreshOwed: false
    property var lastProviderAnswer: null
    // The archive formats this box actually probed, and whether a converter is installed at all.
    property var archiveFormats: []
    property bool canConvert: false
    property bool canExtract: false
    // The cursor row's own name, which is what tells a disk image (lists only) from an archive.
    property string archiveName: ""
    property bool clipboardAvailable: false
    property bool clipboardWatchFailed: false
    // Whether the cursor row is a file, an archive, an image; all decided client-side. Only a file row takes send peers.
    property bool rowIsFile: false
    property bool rowIsArchive: false
    property bool rowIsImage: false
    // Flash to drive reads the name rather than the icon, and a zip's one member once named.
    property string rowName: ""
    property string flashZipMember: ""
    // MenuAdditions040: Show original is visible but only on a symlink.
    property bool rowIsSymlink: false
    // MenuAdditions040 callout 10: the two-byte shebang read of the cursor row at menu open.
    property bool rowHasShebang: false
    // ... and whether the menu's single target is the cursor row itself.
    property bool cursorIsTarget: false
    property int rowMode: 0
    property int selectionCount: 0
    // MenuAdditions040: Permissions takes the whole selection.
    property var selectionModes: []
    // Compare follows held metadata and the matching snapshot while the menu is open.
    property var compareKinds: []
    // OpenWith.html's flyout rows, filled by ui/PaneMenuActions.qml when the registry answers.
    property var openWithApps: []
    property bool openWithLoaded: false
    // The actual account directory is available only after a fresh provider status and identity check.
    property string dropboxPath: ""
    property bool dropboxInstalled: false
    property string dropboxReason: ""
    property string dropboxCode: ""
    // A row already inside the account directory offers a share link instead of another move.
    property bool rowInDropbox: false
    // False on a listing's empty space, where Menus.html's background column is what opens instead.
    // openBackground() is its only writer and openAt() puts it back, because one instance serves both.
    property bool hasRow: true
    // ExtThumbs: src/backend/extclass.rs's word for the directory this opening answers for.
    property string storageClass: ""
    // The Locked tile's folder while one is drawn, "" while none is; bound by ui/Pane.qml. A right
    // click landing on the tile reaches openBackground through the views, which routes it to the
    // locked folder's own menu rather than the parent's background one.
    property string tileTarget: ""
    property int tileMode: 0
    // The folder this opening answers for, captured at open and cleared at close.
    property string lockedPath: ""
    property int lockedMode: 0
    readonly property bool forLocked: root.lockedPath.length > 0
    // The owner's menuSelectionIdentity(), read at open and at a choice only; none, as on the rail, reads "".
    property var identitySource: null
    function selectionIdentity() { return root.identitySource ? root.identitySource.menuSelectionIdentity() : "" }
    property string openedIdentity: ""
    // The owner intersects the live monitor work area with this application's viewport.
    property rect workArea: Qt.rect(0, 0, root.width, root.height)
    readonly property real workAreaInset: Theme.space(4)

    // The rail's own rows when ui/Sidebar.qml raised this menu, empty when the listing did. One
    // instance serves both: a second one in this tree takes the keyboard from the list, see AGENTS.md.
    property var railEntries: []
    // The key naming the rail row those rows belong to, see ui/js/Mounts.js "railKey": the rail
    // rebuilds on a poll, so an index would name a different row by the time one is chosen.
    property string railKey: ""
    readonly property bool forRail: root.railEntries.length > 0
    signal railChosen(string action, string key)
    // The Locked tile's folder with the row chosen on it, read before close() the way railKey
    // is: close() clears lockedPath, so Pane.qml cannot read it after the menu is gone.
    signal lockedChosen(string action, string path)

    // ui/Header.qml's own entrance, the third face of this one instance: openForHeader() flips the
    // entries to ui/js/Menu.js headerEntries (the column toggles and the hidden toggle, built from
    // qs module ViewState's hidden columns and the pane's showHidden), and every row flows back
    // through chosen() like the listing's own. A row's chosen verb routes by prefix in ui/Pane.qml.
    property bool forHeader: false
    function openForHeader(scenePoint) {
        root.forHeader = true
        root.place(scenePoint)
    }

    // The pane keeps its Keys handler on the list, so the menu has to hand focus back on close.
    property Item focusHolder: null
    // The original focus item may disappear while the menu is open; the owning view is the fallback.
    property Item focusOwner: null
    readonly property bool keyboardFocused: keyCatcher.activeFocus

    // The keyboard-highlighted top-level row, and which row's flyout is open beside it, or -1.
    property int cursor: 0
    property int openSubmenuRow: -1
    property int submenuCursor: 0
    // A hidden row still opens its flyout from the action.
    property string loneFlyoutAction: ""
    readonly property bool submenuOpen: root.openSubmenuRow >= 0 || root.loneFlyoutAction.length > 0
    // What a closing flyout keeps drawing while it fades out; ui/FlyoutFade.qml holds them at their last open values.
    property var heldRows: []
    property real heldOffset: 0
    property string heldAction: ""
    property var flyoutFade: null
    function submenuAction() {
        return root.loneFlyoutAction.length > 0 ? root.loneFlyoutAction
             : root.entries[root.openSubmenuRow] ? root.entries[root.openSubmenuRow].action : ""
    }
    onSubmenuOpenChanged: if (!root.submenuOpen) MenuRefresh.pressChanged(root, false); else root.buildFlyoutFade()
    // Built by the first flyout, so a menu that never opens one holds neither the fade nor the held values.
    function buildFlyoutFade() {
        if (root.flyoutFade === null)
            root.flyoutFade = Qt.createComponent("FlyoutFade.qml").createObject(root, { menu: root })
    }
    // The glyph every open flyout row draws, read back so a test can name it without OCR.
    function submenuGlyphs() {
        if (!root.submenuOpen)
            return ""
        var mark = Menu.submenuGlyph(root.submenuAction())
        var out = []
        for (var i = 0; i < root.submenuEntries.length; i++) {
            // What the row draws, not what the flyout defaults to: an Open with row carries its own
            // glyph or an application icon, and reporting the default made a check measure nothing.
            var row = root.submenuEntries[i]
            out.push(row.separator === true ? "" : row.icon ? "icon"
                   : row.glyph !== undefined ? row.glyph : mark)
        }
        return out.join("|")
    }

    // The open flyout draws the row's entries, or the lone action's leaves.
    readonly property var submenuEntries: root.loneFlyoutAction.length > 0
        ? Menu.flyoutEntries(root.loneFlyoutAction)
        : (root.submenuOpen && root.entries[root.openSubmenuRow]
            ? root.entries[root.openSubmenuRow].submenu : [])

    // The row list this menu currently offers; a test reads this back through shell.qml's IPC.
    property var entries: []
    // While a card's rows build its fit reads none, then reads them once when they stand; a closed flyout holds nothing.
    property bool rowsBuilding: false
    property bool submenuBuilding: false
    function setEntries(next, flyoutRow) { try { root.rowsBuilding = true; root.submenuBuilding = root.submenuOpen; root.entries = next; if (flyoutRow !== undefined) root.openSubmenuRow = flyoutRow } finally { root.rowsBuilding = false; root.submenuBuilding = false } }
    property bool canTrash: true
    // False in a read-only folder, off the listing's own w flag; true until the pane knows better.
    property bool dirWritable: true
    // A filesystem that holds no links offers no Paste as rows; true until the pane knows better.
    property bool canLink: true
    // The Tags flyout's rows while a menu is live; null where tags cannot be stored, which drops the row.
    property var tagSubmenu: null
    // Issue 179: the background menu's Sort by flyout offers its forget row only for this folder.
    property bool hasFolderSort: false

    // Menu.js builds entries from the context shared by the row menu and query; includeHidden is the keymap sheet's.
    function buildEntries(flyoutAction, includeHidden) {
        // Rail rows arrive built with their own release verdicts.
        if (root.forRail)
            return root.railEntries
        if (root.forHeader)
            return Menu.headerEntries(ViewState.hiddenCols, root.showHidden)
        // Locked rows address the denied folder rather than its covered parent.
        if (root.forLocked)
            return LockedMenu.lockedEntries({ lockedMode: root.lockedMode, hiddenActions: ViewState.menuHidden })
        return Menu.listingEntries(root.listingContext(flyoutAction, includeHidden))
    }
    function listingContext(flyoutAction, includeHidden) {
        return MenuContext.listingContext(root, flyoutAction, includeHidden, {
            scripts: Flea.Scripts.entries, hiddenActions: ViewState.menuHidden,
            langMarks: ViewState.langMarks,
            thumbPreview: ViewState.preview, updateVersion: UpdateCheck.menuVersion
        })
    }

    // The row item at an index, for ui/Ipc.qml: a driven test clicks a menu row without deriving its geometry from a row count the Menus settings can now change under it.
    function itemFor(index) { return menuRows.itemAt(index) }
    function submenuItemFor(index) { return subRows.itemAt(index) }
    readonly property var frameItem: frame
    readonly property var submenuFrameItem: flyout

    // A separator is never the cursor, so both key steps and the opening cursor skip over one.
    function stepCursor(from, delta) { return Menu.stepRow(root.entries, from, delta) }
    // The same rule inside a flyout: OpenWith.html's tail row sits under its own separator.
    function stepSubmenu(from, delta) { return Menu.stepRow(root.submenuEntries, from, delta) }
    // A wheel over the main frame closes the flyout first, the same path moving the pointer
    // onto a plain row takes, and then steps; the highlight never moves where it is not drawn.
    function stepMain(delta) {
        root.openSubmenuRow = -1
        root.loneFlyoutAction = ""
        root.cursor = root.stepCursor(root.cursor, delta)
    }

    function firstRow() {
        return root.stepCursor(-1, 1)
    }

    anchors.fill: parent
    // The frames fade out after opened flips (ui/Reveal.qml), so the menu stays drawn until both are gone and takes no input meanwhile.
    visible: root.opened || frame.opacity > 0 || flyout.opacity > 0
    enabled: root.opened
    z: 1

    // Built by the first open, so a pane that never opens a menu holds no animation (the first frame's cost).
    property var fade: null
    onOpenedChanged: {
        if (root.opened && root.fade === null)
            root.fade = Qt.createComponent("Reveal.qml").createObject(root, { target: frame, shown: Qt.binding(function () { return root.opened }) })
    }

    // Takes a point in scene coordinates and keeps the whole menu inside the pane it belongs to.
    function openAt(scenePoint) {
        root.clearRail()
        root.forHeader = false
        root.hasRow = true
        root.place(scenePoint)
    }

    // Empty-space clicks draw background entries, or the Locked tile's own folder menu.
    function openBackground(scenePoint) {
        if (root.tileTarget.length > 0) {
            root.openLocked(root.tileTarget, root.tileMode, scenePoint)
            return
        }
        root.clearRail()
        root.forHeader = false
        root.hasRow = false
        root.place(scenePoint)
    }

    // The Locked tile's own entrance, capturing the folder this opening answers for; every row
    // below acts on that folder alone, through ui/Pane.qml's locked dispatch rather than the
    // snapshot the cursor rows take.
    function openLocked(path, mode, scenePoint) {
        // An empty locked menu opens no frame and closes a stale one, so a refusal leaves no actionable frame standing.
        var refusal = LockedMenu.lockedRefusal({ lockedMode: mode, hiddenActions: ViewState.menuHidden })
        if (refusal.length > 0) { root.close(); root.refused(refusal); return }
        root.clearRail()
        root.forHeader = false
        root.hasRow = false
        root.lockedPath = path
        root.lockedMode = mode
        root.place(scenePoint)
        var notice = LockedMenu.lockedNotice({ hiddenActions: ViewState.menuHidden })
        if (notice.length > 0) root.noticed(notice)
    }

    // ui/Sidebar.qml's own entrance to this same menu: the rail hands in its rows and the key that
    // names the row they came from, and a rail row with nothing to release opens no menu at all.
    function openForRail(key, entries, scenePoint) {
        if (!entries || entries.length === 0)
            return
        root.railKey = key
        root.railEntries = entries
        root.place(scenePoint)
    }

    // Cleared on both ends: a rail entry left standing would put Eject on a listing row's menu,
    // and a locked path left standing would put the old folder's rows on another directory's tile.
    function clearRail() {
        root.railEntries = []
        root.railKey = ""
        root.lockedPath = ""
        root.lockedMode = 0
    }

    // Where the menu was asked to open, in this item's own coordinates; clampFrame runs twice on it.
    property real placeX: 0
    property real placeY: 0

    // A Column hands its implicitHeight to the frame one polish after its model changes, so the
    // height place() reads is still the menu that was open before this one. Clamping again on the
    // real height lands before the first paint, so no menu is placed against another's size.
    function clampFrame() {
        frame.x = root.workArea.x + Menu.clamp(root.placeX - root.workArea.x, frame.width, root.workArea.width)
        frame.y = root.workArea.y + root.workAreaInset
                + Menu.clamp(root.placeY - root.workArea.y - root.workAreaInset,
                             frame.height, Math.max(0, root.workArea.height - 2 * root.workAreaInset))
    }

    // The scene point any row or the ground last saw, so a row tells a moving pointer from one it arrived under.
    property point pointerGlobal: Qt.point(-1, -1)
    // Placing to its first frame's afterAnimating: Qt hovers what that frame shows, kept rows too, so a "move" then is the rest point; Qt 6.12 can hover later, which ui/MenuRow.qml reads as rest.
    property bool pointerSettling: false
    Connections {
        target: root.pointerSettling ? root.Window.window : null
        function onAfterAnimating() { root.pointerSettling = false }
    }

    function place(scenePoint) {
        root.refreshOwed = false
        root.preparing = true
        if (!root.opened)
            root.focusHolder = root.Window.window ? root.Window.window.activeFocusItem : null
        root.pointerGlobal = Qt.point(-1, -1)
        root.pointerSettling = true
        // A place that changes nothing drawn schedules no frame, so it asks for the one that ends the settle.
        if (root.Window.window) root.Window.window.update()
        var point = root.mapFromItem(null, scenePoint)
        root.placeX = point.x
        root.placeY = point.y
        root.setEntries(root.buildEntries())
        root.openedIdentity = root.selectionIdentity()
        root.loneFlyoutAction = ""
        scroll.contentY = 0
        scroll.resetSteps()
        subScroll.resetSteps()
        root.clampFrame()
        root.cursor = root.firstRow()
        root.openSubmenuRow = -1
        root.submenuCursor = 0
        root.opened = true
        root.preparing = false
        if (root.hasRow && !root.forRail && !root.forHeader) root.snapshotRequested()
        keyCatcher.forceActiveFocus()
    }
    onWorkAreaChanged: if (root.opened) root.clampFrame()
    onCursorChanged: scroll.reveal(menuRows.itemAt(root.cursor))
    onSubmenuCursorChanged: subScroll.reveal(subRows.itemAt(root.submenuCursor))

    // Every wheel scroll calls this, so a shut menu costs nothing and never touches focus.
    function close() {
        root.refreshOwed = false
        if (!root.opened)
            return
        root.opened = false
        root.openSubmenuRow = -1
        root.loneFlyoutAction = ""
        root.clearRail()
        var holder = root.focusHolder && root.focusHolder.visible && root.focusHolder.enabled ? root.focusHolder : root.focusOwner
        if (holder)
            holder.forceActiveFocus()
    }

    // The menu closes before the action runs, so it never hangs over the listing that action opened.
    function choose(action) {
        if (!root.validateChoice(action, "")) return
        // Both read before close(), which is what clears the rail rows and the locked path.
        var key = root.railKey
        var rail = root.forRail
        var locked = root.lockedPath
        root.close()
        if (rail) {
            root.railChosen(action, key)
            return
        }
        if (locked.length > 0) {
            root.lockedChosen(action, locked)
            return
        }
        root.chosen(action)
    }

    // One signal covers every submenu: the row's own action, a colon, and the entry chosen inside it.
    function chooseSub(id) {
        var entry = root.entries[root.openSubmenuRow]
        var action = root.loneFlyoutAction || (entry ? entry.action : "")
        if (root.loneFlyoutAction.length > 0) {
            var lonePick = Menu.loneChoice(root.loneFlyoutAction, id, root.forRail, root.forHeader, root.hasRow, root.openedIdentity, root.selectionIdentity())
            if (lonePick.kind !== "fire") {
                root.refuseLone(lonePick.kind)
                return
            }
        }
        if (!action || !root.validateChoice(action, id)) return
        // A Tags choice changes the boxes this flyout draws, so the menu stays up and the answer redraws them in place.
        if (action !== "tags") root.close()
        root.chosen(action + ":" + id)
    }

    function openSubmenu(index) {
        root.loneFlyoutAction = ""
        if (!Menu.hasSubmenu(root.entries[index]) || root.entries[index].disabled === true) return
        root.submenuBuilding = true
        try { root.openSubmenuRow = index } finally { root.submenuBuilding = false }
        root.submenuCursor = 0
        subScroll.contentY = 0
    }

    // c and P open Copy as and Paste as with the flyout already open.
    function openSubmenuFor(action) {
        var leaves = Menu.flyoutEntries(action)
        if (!leaves.length) return false
        if (action === "pasteAs" && !root.clipboardAvailable) {
            root.close()
            root.refused(Menu.EMPTY_CLIPBOARD)
            return false
        }
        if (!root.validateChoice(action, leaves[0].id)) return false
        var pick = Menu.submenuFor(action, root.entries, root.clipboardAvailable)
        if (pick.kind === "row") {
            root.cursor = pick.index
            root.openSubmenu(pick.index)
            return true
        }
        if (pick.kind === "lone") {
            root.submenuBuilding = true
            try { root.openSubmenuRow = -1; root.loneFlyoutAction = action } finally { root.submenuBuilding = false }
            root.submenuCursor = 0
            subScroll.contentY = 0
            return true
        }
        if (pick.kind === "refuse") {
            root.close()
            root.refused(Menu.EMPTY_CLIPBOARD)
            return false
        }
        return false
    }

    // Fresh capabilities use the normal inventory; selection stays on its action and placement uses the existing clamp.
    function refreshProviderRows() {
        if (!root.opened || root.forRail || root.forHeader || root.forLocked) return
        // Replacing a pressed delegate destroys its grab before release can activate it.
        if (MenuRefresh.anyPressed(menuRows, root.submenuOpen ? subRows : null)) {
            root.refreshOwed = true
            return
        }
        root.refreshOwed = false
        var next = root.buildEntries()
        // An answer that changed nothing drawn leaves every row standing: no model reset, no cursor move.
        if (MenuRefresh.unchanged(root.entries, next)) return
        MenuRefresh.apply(root, next)
    }
    // A finished refresh: its answer is what later opens draw while their own refresh runs behind them, see ui/js/MenuRefresh.js.
    function providersSettled() {
        root.lastProviderAnswer = MenuRefresh.settle(MenuRefresh.live(root))
        root.refreshProviderRows()
    }

    function refuseLone(kind) {
        root.close()
        root.refused(kind === "moved" ? "Selected items changed; reopen the menu."
                                     : "That action is no longer available; reopen the menu.")
    }

    // Rebuild only to validate; rows stay fixed while the menu is open under the pointer.
    function validateChoice(action, subId, includeHidden) {
        return MenuContext.validateChoice(root, action, subId, includeHidden)
    }

    // Offsets are summed, not multiplied; a lone flyout stands at the top.
    function submenuOffset() {
        if (root.loneFlyoutAction.length > 0)
            return 0
        var y = 0
        for (var i = 0; i < root.openSubmenuRow; i++)
            y += root.entries[i].separator === true ? separatorProbe.separatorHeight : Theme.rowHeight
        return y - scroll.contentY
    }

    // One row off the model, only so the two heights above are read from MenuRow rather than repeated here.
    Flea.MenuRow {
        id: separatorProbe
        visible: false
        entry: ({ separator: true })
    }

    // The ground owns every pointer event outside the rows: hover stops here, the wheel is swallowed, and the click that closes is taken on release so the row beneath never sees a press the close would have handed it.
    MouseArea {
        id: ground
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        hoverEnabled: true
        onEntered: root.pointerGlobal = ground.mapToItem(null, ground.mouseX, ground.mouseY)
        onPositionChanged: function (mouse) { root.pointerGlobal = ground.mapToItem(null, mouse.x, mouse.y) }
        onClicked: root.close()
        onWheel: function (wheel) { wheel.accepted = true }
    }

    // How far an edge fade reaches past the card's padding, as a share of a row, so it covers the first cut row without washing a whole one.
    readonly property real fadeRowShare: 0.7
    readonly property real fadeReach: Math.round(Theme.rowHeight * root.fadeRowShare)
    // The widest row's wanted width in a card's column; a binding that calls it follows every row's own text.
    function widestRow(column) { return MenuFit.widestWanted(column.children) }

    Rectangle {
        id: frame
        opacity: 0
        // Theme.menuWidth, or the widest row's own width when a label and its hint need more, never past the work area.
        width: Math.max(0, Math.min(root.workArea.width, root.rowsBuilding ? Theme.menuWidth : Math.max(Theme.menuWidth, root.widestRow(rows))))
        onWidthChanged: if (root.opened) root.clampFrame()
        // The vertical inset keeps the first and last row's square highlight off the rounded corners.
        height: Math.max(0, Math.min(rows.implicitHeight + 2 * Theme.spacing.rowPaddingY,
                                    root.workArea.height - 2 * root.workAreaInset))
        // The height this menu is actually going to have, arriving after place() has already run.
        onHeightChanged: if (root.opened) root.clampFrame()
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        // Mirrors hyprland decoration:rounding, same as NetworkDialog; 0 on a stock box stays square.
        radius: Style.cornerRadius

        // The card takes every press no row takes (padding, separator, disabled row); a stepped body is no Flickable, so the closing ground would.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton }

        Flea.CardScroll {
            id: scroll
            // A menu steps the highlight, never pixel scrolls: one row a notch, one row per row height of touchpad travel. No bar, no lane.
            highlightSteps: true
            stepBy: function (delta) { root.stepMain(delta) }
            revealClearY: root.fadeReach
            // A rebuilt model keeps the flyout's row, else the cursor's, in view.
            onContentHeightChanged: if (root.opened) scroll.reveal(menuRows.itemAt(root.openSubmenuRow >= 0 ? root.openSubmenuRow : root.cursor))
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingY
            anchors.bottomMargin: Theme.spacing.rowPaddingY
        Column {
            id: rows
            width: parent.width

            Repeater {
                id: menuRows
                model: root.entries
                delegate: Flea.MenuRow {
                    id: row
                    required property var modelData
                    required property int index
                    width: rows.width
                    entry: row.modelData
                    onPressedChanged: MenuRefresh.pressChanged(root, row.pressed)
                    current: !root.submenuOpen && root.cursor === row.index
                    lastPointerGlobal: root.pointerGlobal
                    onPointerSeen: function (at) { root.pointerGlobal = at }
                    onPointerMoved: {
                        if (root.pointerSettling) return
                        root.cursor = row.index
                        if (Menu.hasSubmenu(row.modelData)) root.openSubmenu(row.index)
                        else { root.openSubmenuRow = -1; root.loneFlyoutAction = "" }
                    }
                    onActivated: {
                        if (Menu.hasSubmenu(row.modelData))
                            root.openSubmenu(row.index)
                        else
                            root.choose(row.modelData.action)
                    }
                }
            }
        }
        }
        Flea.MenuEdgeFade {
            anchors.top: parent.top
            inset: Theme.spacing.rowPaddingY
            reach: root.fadeReach
            visible: scroll.contentY > 0
        }
        Flea.MenuEdgeFade {
            anchors.bottom: parent.bottom
            inset: Theme.spacing.rowPaddingY
            reach: root.fadeReach
            visible: scroll.contentY + scroll.height < scroll.contentHeight
            rotation: 180
        }
    }

    // The flyout: a second frame beside whichever row opened it, only while one has, at least its parent's width.
    Rectangle {
        id: flyout
        visible: root.submenuOpen || flyout.opacity > 0
        enabled: root.submenuOpen
        opacity: 0
        x: frame.x + frame.width + width <= root.workArea.x + root.workArea.width
           ? frame.x + frame.width : Math.max(root.workArea.x, frame.x - width)
        // peers.y already carries the inset, so the flyout frame itself stays on the row grid.
        y: Math.max(root.workArea.y + root.workAreaInset,
                    Math.min(frame.y + root.heldOffset, root.workArea.y + root.workArea.height - root.workAreaInset - height))
        // Its own read, so a change of the parent's width does not read every peer's width again.
        readonly property real widestPeer: root.submenuBuilding ? 0 : root.widestRow(peers)
        width: Math.max(0, Math.min(Math.max(frame.width, Theme.menuWidth, flyout.widestPeer), root.workArea.width))
        height: Math.max(0, Math.min(peers.implicitHeight + 2 * Theme.spacing.rowPaddingY,
                                    root.workArea.height - 2 * root.workAreaInset))
        color: Theme.color.surface
        border.width: Theme.spacing.hairline
        border.color: Theme.color.mutedFrame
        radius: Style.cornerRadius

        // The flyout takes its dead presses the same way.
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton }

        Flea.CardScroll {
            id: subScroll
            // The flyout steps like the main frame, through its own cursor and reveal.
            highlightSteps: true
            stepBy: function (delta) { root.submenuCursor = root.stepSubmenu(root.submenuCursor, delta) }
            revealClearY: root.fadeReach
            anchors.fill: parent
            anchors.topMargin: Theme.spacing.rowPaddingY
            anchors.bottomMargin: Theme.spacing.rowPaddingY
        Column {
            id: peers
            width: parent.width

            Repeater {
                id: subRows
                model: root.heldRows
                delegate: Flea.MenuRow {
                    id: subRow
                    required property var modelData
                    required property int index
                    width: peers.width
                    // Which mark a whole flyout draws is ui/js/Menu.js submenuGlyph's to say, so the
                    // read-back submenuGlyphs() above and the drawn row cannot answer differently.
                    // A flyout row may carry its own mark and caption: OpenWith.html rides each
                    // application's own Icon in the mark slot and puts "default" in the hint slot,
                    // and its tail row sits under a separator. MenuAdditions040 leaves carry a
                    // keyHint letter instead, drawn only while key hints are on, like every hint.
                    entry: ({ label: subRow.modelData.label, action: "",
                              disabled: subRow.modelData.disabled === true,
                              separator: subRow.modelData.separator === true,
                              hint: subRow.modelData.hint !== undefined ? subRow.modelData.hint
                                  : subRow.modelData.keyHint !== undefined && ViewState.keyHints
                                  ? subRow.modelData.keyHint : undefined,
                              icon: subRow.modelData.icon,
                              tagRow: subRow.modelData.tagRow,
                              glyph: subRow.modelData.glyph !== undefined ? subRow.modelData.glyph
                                   : Menu.submenuGlyph(root.heldAction) })
                    current: root.submenuCursor === subRow.index
                    onPressedChanged: MenuRefresh.pressChanged(root, subRow.pressed)
                    // A flyout opened by key can land under the resting pointer too, so it reads the same point.
                    lastPointerGlobal: root.pointerGlobal
                    onPointerSeen: function (at) { root.pointerGlobal = at }
                    onPointerMoved: if (!root.pointerSettling && subRow.modelData.separator !== true) root.submenuCursor = subRow.index
                    onActivated: root.chooseSub(subRow.modelData.id)
                }
            }
        }
        }
        Flea.MenuEdgeFade {
            anchors.top: parent.top
            inset: Theme.spacing.rowPaddingY
            reach: root.fadeReach
            visible: subScroll.contentY > 0
        }
        Flea.MenuEdgeFade {
            anchors.bottom: parent.bottom
            inset: Theme.spacing.rowPaddingY
            reach: root.fadeReach
            visible: subScroll.contentY + subScroll.height < subScroll.contentHeight
            rotation: 180
        }
    }

    // One focus catcher for the whole menu: real QML focus never moves into the Repeater rows
    // themselves, so every key lands here regardless of which level is open. They arrive through
    // keys.toml's own table, so j and k step this list the way they step every other one.
    Item {
        id: keyCatcher
        anchors.fill: parent
        focus: true

        Keys.onPressed: function (event) {
            var action = Keymap.lookup(event.key, event.text, event.modifiers, "menu")
            event.accepted = true
            if (action === "escape") {
                if (root.submenuOpen) {
                    root.openSubmenuRow = -1
                    root.loneFlyoutAction = ""
                } else
                    root.close()
                event.accepted = true
                return
            }
            if (action === "cursorDown") {
                if (root.submenuOpen)
                    root.submenuCursor = root.stepSubmenu(root.submenuCursor, 1)
                else
                    root.cursor = root.stepCursor(root.cursor, 1)
                event.accepted = true
                return
            }
            if (action === "cursorUp") {
                if (root.submenuOpen)
                    root.submenuCursor = root.stepSubmenu(root.submenuCursor, -1)
                else
                    root.cursor = root.stepCursor(root.cursor, -1)
                event.accepted = true
                return
            }
            if (action === "parent") {
                root.openSubmenuRow = -1
                root.loneFlyoutAction = ""
                return
            }
            if (action === "menuRight") { root.openSubmenu(root.cursor); return }
            // A flyout letter answers only the flyout it was opened for.
            if (root.submenuOpen) {
                var opener = root.loneFlyoutAction.length > 0
                    ? { action: root.loneFlyoutAction } : root.entries[root.openSubmenuRow]
                if (opener && (opener.action === "copyAs" || opener.action === "pasteAs")) {
                    var leaves = root.submenuEntries
                    for (var l = 0; l < leaves.length; l++) {
                        if (leaves[l].separator === true) continue
                        if (leaves[l].id === action) { root.chooseSub(leaves[l].id); return }
                    }
                }
                // 1 to 7 toggle the Tags flyout row that carries the digit, like a click.
                if (opener && opener.action === "tags" && /^tag[1-7]$/.test(action)) { root.chooseSub(root.submenuEntries[Number(action.charAt(3)) - 1].id); return }
            }
            if (action === "open" || action === "preview") {
                if (root.submenuOpen) {
                    var sub = root.submenuEntries[root.submenuCursor]
                    if (sub)
                        root.chooseSub(sub.id)
                } else {
                    var entry = root.entries[root.cursor]
                    if (Menu.hasSubmenu(entry))
                        root.openSubmenu(root.cursor)
                    else if (entry && entry.separator !== true)
                        root.choose(entry.action)
                }
                event.accepted = true
            }
        }
    }
    onCompareKindsChanged: root.refreshProviderRows()
    readonly property bool langMarks: ViewState.langMarks
    onLangMarksChanged: root.refreshProviderRows()
}
