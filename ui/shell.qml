//@ pragma AppId com.thisisgm.flea
//@ pragma ShellId flea
//@ pragma NativeTextRendering
//@ pragma DefaultEnv QSG_RHI_BACKEND=vulkan
//@ pragma CacheDir $BASE/flea

import Quickshell
import QtQuick
import qs.Commons
import "."
import "." as Flea
import "js/Scale.js" as Scale
import "js/Nav.js" as Nav
import "js/Ops.js" as Ops
import "js/Search.js" as Search

ShellRoot {
    FloatingWindow {
        id: fleaWindow
        title: "Flea"
        implicitWidth: 900
        implicitHeight: 600
        // Quickshell 0.3.1 has no exit API and Qt.quit() is a no-op, so the shell signals itself.
        // The backend is told first and answers when it has drained: a quit cancels the operation in
        // flight, and a cancelled copy removes its own partial, so closing never leaves a half file.
        Connections { target: Quickshell; function onLastWindowClosed() { backend.quit() } }
        Connections { target: backend; function onQuitReady() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) } }

        // Every *Centre reader on the IPC seam is this: an item's painted box, reduced to the point a test clicks.
        function centreOf(item) {
            if (!item)
                return ""
            var rect = fleaWindow.itemRect(item)
            return Math.round(rect.x + rect.width / 2) + " " + Math.round(rect.y + rect.height / 2)
        }

        Rectangle {
            id: view
            anchors.fill: parent
            color: Theme.color.background

            Backend {
                id: backend

                // The warm product path ends when rows first reach the UI, and only this side can see that; see AGENTS.md "Testing".
                property real firstRowsAt: 0
                onRows: function (start, items, ms) {
                    if (backend.firstRowsAt === 0 && items.length > 0)
                        backend.firstRowsAt = Date.now()
                }
            }

            // The canvas's own top chrome: where you are on the left, how you are looking at it on
            // the right. The path lives here, which is why the status bar below carries counts instead.
            Flea.ChromeBar {
                id: chrome
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                path: pane.path
                home: pane.home
                canGoBack: pane.canGoBack
                canGoUp: pane.canGoUp
                viewMode: pane.viewMode
                showHidden: pane.showHidden
                onBackRequested: pane.goBack()
                onUpRequested: pane.openParent()
                onSearchRequested: pane.act("search")
                onViewChosen: function (mode) { pane.viewMode = mode }
                // The path bar's four. The pane navigates and answers for the keyboard exactly as it
                // does for every other route in, so a path typed and a row opened end the same way.
                onPathEntered: function (path) { pane.open(path) }
                onEditClosed: pane.forceActiveFocus()
                // Tab reads the directory with the same peek the columns view makes of an ancestor,
                // so completion adds no request type and lands in that view's own cache on the way past.
                onCompleteRequested: function (dir, hidden) { backend.peek(dir, pane.windowSize, hidden) }
                onSaid: function (text) { bar.say(text, false) }
            }

            // The peek behind Tab. Every peeked line carries the directory and the hidden flag it
            // answers for, so the bar takes the reply to its own request and the columns view, which
            // peeks the same wire for the pane's ancestors, goes on taking its own.
            Connections {
                target: backend
                function onPeeked(path, hidden, total, rows, readFailed, mode) { chrome.completeWith(path, hidden, rows) }
            }

            Flea.TabBar {
                id: tabBar
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: chrome.bottom
                pane: pane
            }

            Flea.Pane {
                id: pane
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: tabBar.bottom
                anchors.bottom: bar.top
                backend: backend
                preview: preview
                shareBrowser: shareBrowser
                keymapSheet: keymapSheet
                onMessage: function (text, isError) { bar.say(text, isError) }
                // A running operation's line, which stands until the operation replaces it; see ui/StatusBar.qml.
                onSticky: function (text) { bar.sticky = text; bar.transfer = pane.transfer }
                onConvertRequested: function (name) { convertDialog.open(name, pane) }
                onPathBarRequested: chrome.startEdit()
                // Issue 9. ViewState persists it and Theme multiplies its own tokens by it, so the
                // whole window follows without any surface reading the chord itself.
                onScaleRequested: function (direction) {
                    ViewState.uiScale = direction === 0 ? 1 : Scale.stepped(ViewState.uiScale, direction)
                    ViewState.save()
                    pane.message(Scale.announce(ViewState.uiScale), false)
                }
                onOpened: function (path) { shareBrowser.close() }
            }

            Flea.StatusBar {
                id: bar
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                path: pane.path
                total: pane.total
                cursorIndex: pane.cursorIndex
                listingState: pane.listingState
                selectionCount: pane.selectionCount()
                fsName: pane.fsName
                fsFree: pane.fsFree
                searchRunning: pane.searchRunning
                searchLine: pane.searchMode === "results"
                            ? Search.statusLine(pane.searchRunning, pane.total, pane.searchScanned, pane.searchMs)
                            : ""
                searchKeys: Search.statusKeys(pane.searchRunning)
                onTransferCancelRequested: function (id) { backend.transfercancel(id) }
            }

            Flea.Preview { id: preview; pane: pane }

            Flea.ConvertDialog {
                id: convertDialog
                anchors.fill: parent
                onAccepted: function (format, strip) { Ops.convert(pane, format, strip) }
            }

            // The keymap sheet ? opens, over the whole window as the convert popup is.
            Flea.KeymapSheet {
                id: keymapSheet
                anchors.fill: parent
            }

            Flea.NetworkDialog {
                id: networkDialog
                // FocusScope remembers its own last-focused child, list or rail, and restores it.
                onClosed: pane.forceActiveFocus()
                onSaved: pane.sidebar.reloadBookmarks()
            }

            Connections {
                target: pane.sidebar
                function onAddRequested() { networkDialog.open() }
                function onSharesListed(baseUri, baseLabel, names) { shareBrowser.open(baseUri, baseLabel, names) }
            }

            // The Omarchy mark's one placement: over the list area alone, so the rail stays live.
            // In the columns view that area is all three columns, so the mark takes the middle one:
            // an empty current directory is that column's answer, not the parent column's.
            // listArea is measured inside pane, which starts below the chrome bar, so pane's own y is added; pane.x is zero.
            Flea.EmptyState {
                id: emptyState
                x: pane.listArea.x + (pane.viewMode === "columns" ? pane.columnsArea.columnWidth : 0)
                y: pane.y + pane.listArea.y
                width: pane.viewMode === "columns" ? pane.columnsArea.columnWidth : pane.listArea.width
                height: pane.listArea.height
                visible: pane.listingState === "empty"
                // The design's no-match answer: the search mark over the query it could not find.
                caption: pane.searchMode === "results" ? "Nothing matches " + pane.searchQuery : ""
                mark: "search"
                hint: pane.searchMode === "results"
                      ? "Press Escape to clear."
                      : "Press Ctrl+Shift+N for a new folder."
            }

            // The loading crawl, same listArea placement; its own hold-off keeps fast listings clean.
            Flea.LoadingState {
                x: pane.listArea.x
                y: pane.y + pane.listArea.y
                width: pane.listArea.width
                height: pane.listArea.height
                visible: pane.listingState === "loading"
            }

            // A bare Network entry's own shares, same listArea placement as EmptyState above.
            Flea.ShareBrowser {
                id: shareBrowser
                x: pane.listArea.x
                y: pane.y + pane.listArea.y
                width: pane.listArea.width
                height: pane.listArea.height
                onClosed: pane.forceActiveFocus()
                onActivated: function (uri, label) { pane.sidebar.mountShare(uri, label) }
            }

            // Issue 20: the mouse's own back button, taken by the window because no row is being
            // clicked; ui/js/Nav.js mouseBack is what chooses between the history and the climb.
            // The menu's own refusal is in there rather than in the list below because that is the
            // only place a JavaScript suite can drive it; the list below is the other overlays a
            // back press must not act behind.
            TapHandler {
                acceptedButtons: Qt.BackButton
                onTapped: {
                    if (chrome.editing || convertDialog.opened || keymapSheet.opened
                            || networkDialog.opened || shareBrowser.active || preview.active
                            || pane.renameEditor() !== null || pane.sidebar.renameEditor() !== null)
                        return
                    Nav.mouseBack(pane)
                }
            }

            Component.onCompleted: {
                var start = Quickshell.env("FLEA_PATH") || Quickshell.env("HOME")
                // Read once: Pane.applyPendingSelect() forgets it after the first rows response.
                pane.pendingSelect = Quickshell.env("FLEA_SELECT") || ""
                pane.open(start)
            }
        }
    }

    // The seam the tests drive, see AGENTS.md "Testing". Every reader lives in ui/Ipc.qml.
    Flea.Ipc {
        fleaWindow: fleaWindow
        pane: pane
        bar: bar
        backend: backend
        chrome: chrome
        tabBar: tabBar
        convertDialog: convertDialog
        keymapSheet: keymapSheet
        networkDialog: networkDialog
        shareBrowser: shareBrowser
    }
}
