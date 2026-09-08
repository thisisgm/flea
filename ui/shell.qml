//@ pragma AppId com.thisisgm.flea
//@ pragma ShellId flea
//@ pragma NativeTextRendering
//@ pragma CacheDir $BASE/flea

import Quickshell
import QtQuick
import qs.Commons
import "."
import "." as Flea
import "js/TextSize.js" as TextSize
import "js/Nav.js" as Nav
import "js/Ops.js" as Ops
import "js/Renderer.js" as Renderer
import "js/Search.js" as Search

ShellRoot {
    FloatingWindow {
        id: fleaWindow
        title: "Flea"
        implicitWidth: 900
        implicitHeight: 600
        property bool rendererFallbackStarted: false

        function handleSceneGraphError(error, message) {
            var backendName = Quickshell.env("QSG_RHI_BACKEND")
            console.warn("graphics backend " + backendName + " failed (" + error + "): " + message)
            var retry = Renderer.fallbackCommand(backendName, Quickshell.env("FLEA_RENDERER_AUTOMATIC"),
                                                 Quickshell.env("FLEA_BIN"))
            if (retry && !rendererFallbackStarted) {
                rendererFallbackStarted = true
                Quickshell.execDetached(retry)
            }
            backend.quit()
        }

        // Null while this loads and the QQuickWindow once it exists, which is before the scene graph starts.
        Connections {
            target: view.Window.window
            function onSceneGraphError(error, message) { fleaWindow.handleSceneGraphError(error, message) }
        }

        // Quickshell 0.3.1 has no exit API and Qt.quit() is a no-op, so the shell signals itself.
        // The backend is told first and answers when it has drained: a quit cancels the operation in
        // flight, and a cancelled copy removes its own partial, so closing never leaves a half file.
        Connections { target: Quickshell; function onLastWindowClosed() { backend.quit() } }
        Connections { target: backend; function onQuitReady() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) } }

        // Issue 9's chords, aliased by keys.toml onto the Display section's own text size. The
        // panel writes ui/ViewState.qml directly and shows the value in the row; a chord has no
        // readout of its own with the panel shut, so this one adds the status line.
        function applyTextSize(direction) {
            if (direction === 0)
                ViewState.followTextSize()
            else
                ViewState.stepTextSize(direction)
            pane.message(TextSize.announce(ViewState.textSize, ViewState.omarchyBase), false)
        }

        // Every *Centre reader on the IPC seam is this: an item's painted box, reduced to the point a test clicks.
        function centreOf(item) {
            if (!item)
                return ""
            var rect = fleaWindow.itemRect(item)
            return Math.round(rect.x + rect.width / 2) + " " + Math.round(rect.y + rect.height / 2)
        }
        // "x y width height" in window pixels, for a test that asserts a card stays inside the window.
        function rectOf(item) {
            if (!item)
                return ""
            var rect = fleaWindow.itemRect(item)
            return Math.round(rect.x) + " " + Math.round(rect.y) + " " + Math.round(rect.width) + " " + Math.round(rect.height)
        }
        // centreOf's sibling, "x width centre": the edges round because a click needs a whole pixel, the centre keeps three decimals because the misalignment it reads is half of one.
        function boxOf(item) {
            if (!item)
                return ""
            var rect = fleaWindow.itemRect(item)
            return Math.round(rect.x) + " " + Math.round(rect.width) + " " + (rect.x + rect.width / 2).toFixed(3)
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
                onSettingsRequested: settingsPanel.open(pane)
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

            // The mark over the list area alone (the middle column in the columns view), declared before
            // the pane so it paints under the pane's context menu and over this Rectangle's ground: a
            // negative z put it under that ground and hid it. listArea is pane-relative, so pane.y is added.
            Flea.EmptyState {
                id: emptyState
                // listSlot, not listArea: a lazy view's item sits at its Loader's local origin, and only the slot carries the sidebar and filter offsets.
                x: pane.listSlot.x + (pane.viewMode === "columns" && pane.columnsArea ? pane.columnsArea.columnWidth : 0)
                y: pane.y + pane.listSlot.y
                width: pane.viewMode === "columns" && pane.columnsArea ? pane.columnsArea.columnWidth : pane.listSlot.width
                height: pane.listSlot.height
                visible: pane.listingState === "empty"
                // The design's no-match answer: the search mark over the query it could not find.
                caption: pane.searchMode === "results" ? "Nothing matches " + pane.searchQuery : ""
                mark: "search"
                // A search that found nothing keeps its own way out, because that sentence is the
                // state's answer and not an advertisement. The empty directory's next move is a
                // shortcut, so it draws only with the Menus section's hints row on.
                hint: pane.searchMode === "results" ? "Press Escape to clear."
                    : ViewState.keyHints ? "Press Ctrl+Shift+N for a new folder." : ""
            }

            // The loading crawl, same listArea placement; its own hold-off keeps fast listings clean.
            Flea.LoadingState {
                x: pane.listSlot.x
                y: pane.y + pane.listSlot.y
                width: pane.listSlot.width
                height: pane.listSlot.height
                visible: pane.listingState === "loading"
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
                settingsPanel: settingsPanel
                onMessage: function (text, isError) { bar.say(text, isError) }
                // A running operation's line, which stands until the operation replaces it; see ui/StatusBar.qml.
                onSticky: function (text) { bar.sticky = text; bar.transfer = pane.transfer }
                onConvertRequested: function (name) { convertDialog.open(name, pane) }
                onPathBarRequested: chrome.startEdit()
                // Issue 9. ViewState persists the stop and Theme derives its own tokens from it, so
                // the whole window follows without any surface reading the chord itself.
                onTextSizeRequested: function (direction) { fleaWindow.applyTextSize(direction) }
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

            // Every overlay below is built by its first open and kept, see AGENTS.md rule 6: a launch
            // that never opens one pays neither its compile nor its objects. Each Loader carries the
            // one or two members its callers read, and ui/Ipc.qml reads the built item or null.
            // Each Loader carries its item's z, because a z set inside the item orders it only within
            // the Loader, and the share browser below would otherwise paint over an open card.
            Loader {
                id: convertDialog
                z: 2
                anchors.fill: parent
                active: false
                source: "ConvertDialog.qml"
                readonly property bool opened: item !== null && item.opened
                function open(name, holder) { active = true; item.open(name, holder) }
            }
            Connections {
                target: convertDialog.item
                function onAccepted(format, strip) { Ops.convert(pane, format, strip) }
            }

            // The keymap sheet ? opens, over the whole window as the convert popup is.
            Loader {
                id: keymapSheet
                z: 2
                anchors.fill: parent
                active: false
                source: "KeymapSheet.qml"
                readonly property bool opened: item !== null && item.opened
                function open(holder) { active = true; item.open(holder) }
            }

            // The settings panel, reached by the comma key from either view, by the toolbar's sliders
            // button, and by the third door the Settings board draws: the background menu's own
            // Settings row, which ui/js/Menu.js backgroundEntries builds and ui/Pane.qml act routes.
            Loader {
                id: settingsPanel
                z: 3
                anchors.fill: parent
                active: false
                source: "SettingsPanel.qml"
                readonly property bool opened: item !== null && item.opened
                function open(holder) { active = true; item.open(holder) }
            }

            Loader {
                id: networkDialog
                z: 2
                anchors.fill: parent
                active: false
                source: "NetworkDialog.qml"
                readonly property bool opened: item !== null && item.opened
                function open() { active = true; item.open() }
                function openLocation(uri, label, password, reason, failedConnect) {
                    active = true
                    item.openLocation(uri, label, password, reason, failedConnect)
                }
            }
            Connections {
                target: networkDialog.item
                // FocusScope remembers its own last-focused child, list or rail, and restores it.
                function onClosed() { pane.forceActiveFocus() }
                function onSaved() { pane.sidebar.reloadBookmarks() }
                function onMountRequested(uri, label, password) { pane.sidebar.saveNetwork(uri, label, password) }
            }

            Connections {
                target: pane.sidebar
                function onAddRequested() { networkDialog.open() }
                function onSharesListed(baseUri, baseLabel, names) { shareBrowser.open(baseUri, baseLabel, names) }
                function onNetworkRetryRequested(uri, label, password, reason, failedConnect) {
                    networkDialog.openLocation(uri, label, password, reason, failedConnect)
                }
            }

            // A bare Network entry's own shares, same listArea placement as EmptyState above.
            // An Item fronts this Loader because its callers read active, which is a Loader's own load switch.
            Item {
                id: shareBrowser
                x: pane.listSlot.x
                y: pane.y + pane.listSlot.y
                width: pane.listSlot.width
                height: pane.listSlot.height
                readonly property bool active: shareLoader.item !== null && shareLoader.item.active
                function open(uri, label, names) { shareLoader.active = true; shareLoader.item.open(uri, label, names) }
                function close() { if (shareLoader.item) shareLoader.item.close() }
                // ui/js/Focus.js shareBrowserAct's two other verbs, reached only while the overlay is up.
                function moveCursor(delta) { if (shareLoader.item) shareLoader.item.moveCursor(delta) }
                function activateCursor() { if (shareLoader.item) shareLoader.item.activateCursor() }
                Loader { id: shareLoader; anchors.fill: parent; active: false; source: "ShareBrowser.qml" }
            }
            Connections {
                target: shareLoader.item
                function onClosed() { pane.forceActiveFocus() }
                function onActivated(uri, label) { pane.sidebar.mountShare(uri, label) }
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
        convertDialog: convertDialog.item
        keymapSheet: keymapSheet.item
        settingsPanel: settingsPanel.item
        networkDialog: networkDialog.item
        shareBrowser: shareLoader.item
        emptyState: emptyState
    }
}
