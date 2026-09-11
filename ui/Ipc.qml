import QtQuick
import Quickshell.Io
import qs.Commons
import "js/Tabs.js" as Tabs

// The seam the tests drive, see AGENTS.md "Testing". Read-only: it reports, never acts.
QtObject {
    id: root

    property var fleaWindow: null
    property var pane: null
    property var panes: []
    property var bar: null
    property var backend: null
    property var chrome: null
    property var tabBar: null
    property var convertDialog: null
    property var keymapSheet: null
    property var settingsPanel: null
    property var networkDialog: null
    property var shareBrowser: null
    property var emptyState: null
    property var permissionsDialog: null
    // Overlays and the columns view are built by their first open, see ui/shell.qml, so until then
    // each reader below answers the empty value its type has: "", false or -1, never a throw.
    readonly property var columns: root.pane ? root.pane.columnsArea : null
    function controlState(name, item) {
        return {name: name, visible: !!item && item.visible, enabled: !!item && item.enabled
            && (item.available === undefined || item.available), focused: !!item && item.activeFocus,
            centre: item ? root.fleaWindow.centreOf(item) : "", rect: item ? root.fleaWindow.rectOf(item) : ""}
    }
    function confirmationState(item) {
        return item ? {opened: item.opened, token: item.snapshot.token || 0, count: item.snapshot.count || 0,
            all: item.snapshot.all === true, destructiveFocus: item.destructiveFocus, title: item.titleText,
            rect: root.fleaWindow.rectOf(item.cardItem), cancel: root.controlState("Cancel", item.cancelItem),
            danger: root.controlState("Delete", item.dangerItem)} : {opened: false}
    }

    // The wrapper holds the references because an IpcHandler marshals every property it owns.
    property IpcHandler seam: IpcHandler {
        target: "flea"
        function ready(): bool { return true }
        function themeLoaded(): bool { return Theme.ready }
        function themeForeground(): string { return String(Theme.color.foreground) }
        function selectedFill(): string { return String(Style.selectedFill) }
        function palette(): string {
            var c = Theme.color;
            return [c.background, c.surface, c.foreground, c.muted, c.accent, c.error, c.symlink, c.executable].join(" ");
        }
        // The size running text really draws at, and a row name's own: the settings case pins both to the stop.
        function bodyPx(): int { return Theme.font.body }
        function rowNamePx(i: int): int { var item = root.pane.itemFor(i); return item ? item.namePx : -1 }
        function metrics(): string { return Theme.font.bodySmall + " " + Theme.font.caption + " " + Theme.spacing.rowPaddingX + " " + Theme.rowHeight }
        // Every token the Blueprint board states, one key=value per line; tools/flea-metrics-gate diffs it. metrics() above stays positional for tests/ui.sh.
        function tokens(): string { return Theme.tokens() }
        function cursor(): int { return root.pane.cursorIndex }
        function gridColumns(): int { return root.pane.cursorStride }
        function gridCaptionState(i: int): string {
            var item = root.pane.visibleItemFor(i)
            if (!item || !item.captionItem) return "{}"
            var caption = item.captionItem
            return JSON.stringify({name: caption.text, lines: caption.lineCount, truncated: caption.truncated,
                textHeight: caption.contentHeight, slotHeight: caption.height,
                bottom: caption.y + caption.height, tileHeight: item.height})
        }
        function drawnCount(): int { return root.pane.listArea.count }
        function total(): int { return root.pane.total }
        function selectionCount(): int { return root.pane.selectionCount() }
        function favouritesSaving(): bool { return Favourites.busy || Favourites.operationActive }
        function selectedIndices(): string { return root.pane.selectedIndices().join(",") }
        function focusView(): string { return root.pane.focusView }
        function keyDeliveryState(): string {
            var pane = root.pane
            var window = pane.Window.window
            return JSON.stringify({keySequence: pane.keySequence, keySequenceIdentity: pane.keySequenceIdentity,
                trashArmedAt: pane.trashArmedAt, clipboard: {paths: pane.clipboard.paths, cut: pane.clipboard.moving},
                searchMode: pane.searchMode, filterTyping: pane.filterTyping, filterQuery: pane.filterQuery,
                searchRunning: pane.searchRunning, searchQuery: pane.searchQuery,
                searchScanned: pane.searchScanned, searchCancelled: pane.searchCancelled,
                paneFocus: pane.activeFocus, listFocus: pane.listArea.activeFocus,
                activeFocusItem: window ? String(window.activeFocusItem) : "",
                preview: {index: pane.previewIndex, focused: !!pane.previewColumnItem && pane.previewColumnItem.activeFocus},
                history: {back: pane.history, forward: pane.forwardHistory}})
        }
        function visibleRowName(i: int): string {
            var item = root.pane.visibleItemFor(i)
            return item && item.visible && item.row ? item.row.n : ""
        }
        function railCursor(): int { return root.pane.railCursor }
        function railCount(): int { return root.pane.railCount }
        function path(): string { return root.pane.path }
        function dualState(): string {
            return JSON.stringify({active: ViewState.state.view === "dual",
                focused: root.panes.indexOf(root.pane), panes: root.panes.map(function(pane) {
                    return pane ? {path: pane.path, total: pane.total, cursor: pane.cursorIndex,
                        loading: pane.listInFlight, selected: pane.selectedIndices(),
                        focused: pane.activeFocus || pane.listArea.activeFocus,
                        listRequests: pane.backend.listRequests} : null
                })})
        }
        function lastMessage(): string { return root.bar.transient_ }
        function statusPrimary(): string { return root.bar.rightText() }
        function statusColor(): string { return String(root.bar.rightColor()) }
        function statusSecondary(): string { return root.bar.secondaryText }
        function statusError(): bool { return root.bar.transientIsError }
        function statusDetail(): string { return root.bar.errorDetail }
        function statusActivityState(): string {
            var card = root.bar.transferCard
            return JSON.stringify({activities: root.bar.activities.map(function(activity) {
                var owner = activity.owner === root.bar.dragFeedbackOwner ? activity.owner.pane : activity.owner
                return {id: activity.transfer.id, text: activity.text, running: activity.transfer.running,
                    cancelling: activity.cancelling, ownerPath: owner.path, ownerFocused: owner.paneFocused}
            }), errors: root.bar.errors.length, notice: root.bar.notice,
                undoAvailable: root.bar.hasUndo,
                transferCard: {visible: !!card && card.visible, cancelling: !!card && card.cancelling,
                    rect: root.fleaWindow.rectOf(card), cancel: root.controlState("Cancel", card ? card.cancelItem : null)}})
        }
        function statusFooterState(): string {
            function textState(item) {
                return {text: item.text, visible: item.visible, x: item.x, width: item.width,
                    implicitWidth: item.implicitWidth, truncated: item.truncated,
                    color: String(item.color), fontSize: item.font.pixelSize}
            }
            return JSON.stringify({path: root.bar.path, total: root.bar.total, selected: root.bar.selectionCount,
                listingState: root.bar.listingState, filesystem: root.bar.fsText(), counts: root.bar.countText(),
                frame: root.fleaWindow.rectOf(root.bar.stripItem), borderWidth: root.bar.stripItem.border.width,
                slotWidth: root.bar.slotWidth, hintWidth: root.bar.hintWidth,
                left: textState(root.bar.countsItem), right: textState(root.bar.primaryItem),
                secondary: textState(root.bar.secondaryItem)})
        }
        function selectionBandState(): string {
            var band = root.pane.selectionBand
            return JSON.stringify(band ? {tracking: band.tracking, active: band.bandActive,
                columns: band.columns, contentY: band.flickable.contentY, scrollRate: band.scrollRate,
                anchor: {x: band.anchor.x, y: band.anchor.y}, end: {x: band.end.x, y: band.end.y}}
                : {tracking: false, active: false})
        }
        function listingWindowState(): string {
            return JSON.stringify({held: root.pane.held, loaded: root.pane.rows.length,
                windowSize: root.pane.windowSize, total: root.pane.total, shownTotal: root.pane.shownTotal})
        }
        function menuState(): string {
            var menu = root.pane.contextMenu()
            return JSON.stringify({opened: menu.visible, entries: menu.entries, cursor: menu.cursor,
                snapshotReady: root.pane.menuActions.ready, snapshotId: root.pane.menuActions.requestId,
                submenu: menu.submenuOpen, submenuCursor: menu.submenuCursor, submenuEntries: menu.submenuEntries,
                frame: root.fleaWindow.rectOf(menu.frameItem), flyout: root.fleaWindow.rectOf(menu.submenuFrameItem),
                workArea: menu.workArea, forHeader: menu.forHeader, forRail: menu.forRail, hasRow: menu.hasRow})
        }
        function contextMenuModel(): string { return JSON.stringify(root.pane.contextMenu().entries) }
        function providerState(): string {
            var pane = root.pane, actions = pane.menuActions, taildrop = pane.taildropService, dropbox = pane.dropboxService
            return JSON.stringify({facts: pane.backend.providers, formatsRequests: pane.backend.formatsToken,
                refreshing: actions.providersRefreshing,
                pendingActivation: actions.pendingActivation, menuFocus: pane.contextMenu().keyboardFocused,
                listFocus: pane.listArea.activeFocus, path: pane.path, cursor: pane.cursorIndex,
                cursorPath: pane.cursorRow ? pane.join(pane.path, pane.cursorRow.n) : "",
                selected: pane.selectedIndices(), selectedPaths: pane.selectedIndices().map(function(index) {
                    var row = pane.rowFor(index)
                    return row ? pane.join(pane.path, row.n) : null
                }),
                taildrop: {checking: taildrop.checking, reason: taildrop.reason, peers: taildrop.peers,
                    timeoutSeconds: taildrop.statusTimeoutSeconds},
                dropbox: dropbox ? {checking: dropbox.dropboxChecking, ready: dropbox.dropboxReady,
                    metadataBusy: dropbox._dropboxMetadataRequest !== 0 || dropbox._dropboxMetadataAgain,
                    reason: dropbox.dropboxReason, path: dropbox.dropboxPath,
                    timeoutSeconds: dropbox.dropboxStatusTimeoutSeconds} : null})
        }
        function contextMenuSubmenuRowCentre(index: int): string { return root.fleaWindow.centreOf(root.pane.contextMenu().submenuItemFor(index)) }
        function menuDialogState(): string {
            var dialog = root.pane.menuActions.item
            if (!dialog) return JSON.stringify({opened: false})
            // Two dialogs load into that one Loader, and the application rows belong to Open with alone.
            var applications = dialog.applications !== undefined ? dialog.applications : []
            var cursor = dialog.cursor !== undefined ? dialog.cursor : 0
            return JSON.stringify({opened: dialog.opened, action: dialog.action, busy: dialog.busy,
                committing: dialog.committing, error: dialog.errorText, facts: dialog.facts,
                applications: applications, cursor: cursor, rect: root.fleaWindow.rectOf(dialog.cardItem),
                controls: [Object.assign(root.controlState("Cancel", dialog.closeItem), {enabled: dialog.closeItem.activeFocusOnTab}),
                    Object.assign(root.controlState("Submit", dialog.submitItem), {enabled: dialog.canSubmit}),
                    root.controlState("Field", dialog.fieldItem), root.controlState("Applications", dialog.applicationsItem)],
                confirmation: root.confirmationState(dialog.confirmationItem)})
        }
        // OpenWith.html's own card: the two groups it draws, the seat the cursor holds, and the
        // geometry a matched-size check measures against the board.
        function openWithState(): string {
            var dialog = root.pane.menuActions.item
            if (!dialog || dialog.action !== "openWith") return JSON.stringify({opened: false})
            return JSON.stringify({opened: dialog.opened, busy: dialog.busy, committing: dialog.committing,
                kind: dialog.kind, mime: dialog.mime, name: dialog.name, always: dialog.always,
                error: dialog.errorText, search: dialog.fieldItem.text, cursor: dialog.cursor,
                rows: dialog.rows.map(function (row) {
                    return row.eyebrow !== undefined ? {eyebrow: row.eyebrow, rule: row.rule === true}
                         : {id: row.id, label: row.label, icon: row.icon, isDefault: row.default === true}
                }),
                rect: root.fleaWindow.rectOf(dialog.cardItem),
                rowHeight: Theme.rowHeight, eyebrowHeight: dialog.eyebrowHeight,
                listRect: root.fleaWindow.rectOf(dialog.applicationsItem),
                rowRect: root.fleaWindow.rectOf(dialog.applicationItem(dialog.cursor)),
                controls: [root.controlState("Field", dialog.fieldItem), root.controlState("Applications", dialog.applicationsItem),
                    root.controlState("Always", dialog.alwaysItem), root.controlState("Cancel", dialog.closeItem),
                    Object.assign(root.controlState("Open", dialog.submitItem), {enabled: dialog.canSubmit})]})
        }
        function permissionsState(): string {
            var dialog = root.permissionsDialog
            if (!dialog) return JSON.stringify({opened: false})
            return JSON.stringify({opened: dialog.opened, facts: dialog.facts, path: dialog.path, mode: dialog.modeText,
                displayedError: dialog.displayedError, displayedSummary: dialog.displayedSummary,
                bodyRect: root.fleaWindow.rectOf(dialog.bodyItem),
                editable: dialog.editable, busy: dialog.busy, error: dialog.errorText, rect: root.fleaWindow.rectOf(dialog.cardItem),
                controls: dialog.controls().map(function(control) {
                    return Object.assign(root.controlState(control.name, control.item), {checked: control.checked, bit: control.bit,
                        enabled: control.enabled === undefined ? control.item.enabled : control.enabled})
                })})
        }
        function trashState(): string {
            var view = root.pane.trash.item
            var rail = null
            for (var index = 0; index < root.pane.railCount; index++) {
                var row = root.pane.sidebar.railItemFor(index)
                if (row && row.modelData.kind === "trash") { rail = {current: row.cursor, countText: row.detail}; break }
            }
            if (!view) return JSON.stringify({opened: false, count: root.pane.sidebar.trashCount, rail: rail})
            return JSON.stringify({opened: view.opened, total: view.total, count: root.pane.sidebar.trashCount, countText: view.countText, busy: view.busy,
                operationActive: view.operationActive, cursor: view.cursor, first: view.first,
                selectedCount: view.selectedCount, selectionToken: view.selectionToken, selectionCount: view.selectionCount,
                headerLabels: view.headerLabels, upEnabled: view.upItem.enabled, rail: rail,
                rows: view.rows.map(function(row) { return Object.assign({}, row, {selected: view.isSelected(row.uri)}) }),
                confirmation: root.confirmationState(view.confirmationItem)})
        }
        function trashRowCentre(index: int): string { return root.pane.trash.item ? root.fleaWindow.centreOf(root.pane.trash.item.rowItemFor(index)) : "" }
        function trashControlCentre(name: string): string {
            var view = root.pane.trash.item
            if (!view) return ""
            var item = ({back: view.backItem, up: view.upItem, cancel: view.confirmationItem.cancelItem, danger: view.confirmationItem.dangerItem})[name]
            return item ? root.fleaWindow.centreOf(item) : ""
        }

        // The sticky slot an operation holds while it runs, so a test can name the verb in flight.
        function stickyMessage(): string { return root.bar.sticky }
        function firstRowsAt(): string { return String(root.backend.firstRowsAt) }
        // Both instants in one read, so no IPC call of a harness ever lands inside the interval it is timing.
        function inputToRows(): string { return String(root.pane.inputAt) + " " + String(root.pane.rowsAt) }
        function mode(): string { return "browse" }
        function state(): string { return root.pane.listingState }
        function listInFlight(): bool { return root.pane.listInFlight }
        function stateMessage(): string { return root.pane.stateMessage }
        function contextMenuVisible(): bool { return root.pane.menuVisible }
        function showHidden(): bool { return root.pane.showHidden }
        // One label per current menu row, joined so a test can assert contents without OCR. A
        // separator has no label of its own and reads as "-", which is what makes the grouping assertable.
        function contextMenuEntries(): string {
            var entries = root.pane.contextMenu().entries
            var out = []
            for (var i = 0; i < entries.length; i++) {
                out.push(entries[i].separator === true ? "-" : entries[i].label)
            }
            return out.join("|")
        }
        // The key each row prints beside its label, in the same order and read off the drawn rows,
        // so an unbound row's empty slot is assertable and not only the map behind it.
        function contextMenuHints(): string {
            var menu = root.pane.contextMenu()
            var out = []
            for (var i = 0; i < menu.entries.length; i++) {
                var item = menu.itemFor(i)
                out.push(item ? String(item.hint) : "")
            }
            return out.join("|")
        }
        // The glyph each row draws, in the same order, so the "every row is marked" rule is assertable.
        function contextMenuGlyphs(): string {
            var entries = root.pane.contextMenu().entries
            var out = []
            for (var i = 0; i < entries.length; i++) {
                // A brand mark has no glyph name, so the reader names the mark instead; a row draws
                // exactly one of the two and a test asserts the same string either way.
                out.push(entries[i].separator === true ? "-"
                         : (entries[i].mark || entries[i].glyph || ""))
            }
            return out.join("|")
        }
        // A peer row names a machine and an archive row a file, so the flyout's mark is its own.
        function contextMenuSubmenuGlyphs(): string { return root.pane.contextMenu().submenuGlyphs() }
        // A peer is named by whoever is on the tailnet, so a test reads the name here rather than
        // knowing it. No separator branch: every flyout in the tree builds {id, label} rows only,
        // whether it came from Archive.formatEntries, Menu.sortEntries or the Taildrop peer list.
        function contextMenuSubmenuEntries(): string {
            return root.pane.contextMenu().submenuEntries.map(function (e) { return e.label }).join("|")
        }
        function contextMenuCursor(): int { return root.pane.menuCursor }
        function settingsOpen(): bool { return root.settingsPanel ? root.settingsPanel.opened : false }
        function settingsSection(): string { return root.settingsPanel ? root.settingsPanel.section : "" }
        function settingsSide(): string { return root.settingsPanel ? root.settingsPanel.side : "" }
        function settingsCursor(): int { return root.settingsPanel ? root.settingsPanel.cursor : -1 }
        // One row per line, kind|label|value, so a test reads what the panel draws without OCR and
        // the stored value behind each control is assertable from the same string.
        function settingsRows(): string { return root.settingsPanel ? root.settingsPanel.rowsText() : "" }
        function settingsModel(): string { return root.settingsPanel ? JSON.stringify(root.settingsPanel.rows) : "[]" }
        function settingsSections(): string { return root.settingsPanel ? root.settingsPanel.sectionsText() : "[]" }
        function settingsRowCentre(id: string): string { return root.settingsPanel ? root.fleaWindow.centreOf(root.settingsPanel.rowItemForId(id)) : "" }
        function settingsFavouriteControlCentre(id: string, part: string): string {
            var row = root.settingsPanel ? root.settingsPanel.rowItemForId(id) : null
            if (!row || !row.isFavourite) return ""
            var item = part === "drag" ? row.favouriteItem.dragItem
                     : part === "add" ? row.favouriteItem.actionItem(0)
                     : part === "remove" ? row.favouriteItem.actionItem(1) : null
            return item && item.visible ? root.fleaWindow.centreOf(item) : ""
        }
        function uiSettings(): string { return JSON.stringify(ViewState.state) }
        function keymapPreset(): string { return ViewState.keysPreset }
        function railEntries(): string { return JSON.stringify(root.pane.sidebar.entries) }
        function railDetails(): string {
            var sidebar = root.pane.sidebar
            function box(item) {
                var point = item.mapToItem(null, 0, 0)
                return {x: point.x, y: point.y, width: item.width, height: item.height}
            }
            return JSON.stringify({headers: sidebar.headingItems().map(function(item) {
                return {text: item.text, visible: item.visible, rect: box(item),
                    topPadding: item.topPadding, fontSize: item.font.pixelSize}
            }), rows: sidebar.entries.map(function(entry, index) {
                var row = sidebar.railItemFor(index)
                if (!row) return {index: index, missing: true}
                var detail = row.detailItem
                return {index: index, label: entry.label, kind: entry.kind, group: entry.group,
                    size: entry.size, detail: detail.text, detailColor: String(detail.color),
                    fontSize: detail.font.pixelSize, tabular: detail.font.features.tnum === 1,
                    rect: box(row), detailRect: box(detail),
                    indicatorRect: box(row.indicatorSlot), indicatorVisible: row.indicatorVisible}
            })})
        }

        // The panel's own title, a spot on the card with no control under it: a click there must leave the panel open.
        function settingsTitleCentre(): string { return root.settingsPanel ? root.fleaWindow.centreOf(root.settingsPanel.titleItem) : "" }
        // A rail row's centre, clicked over the list by tests/ui.sh clickthrough to prove the press stops at the panel.
        function settingsRailRowCentre(id: string): string { return root.settingsPanel ? root.fleaWindow.centreOf(root.settingsPanel.railItemFor(id)) : "" }
        // Observe the compact card's actual content and viewport without changing its scroll position.
        function settingsScroll(): string { return root.settingsPanel ? root.settingsPanel.paneScroll() : "" }
        function settingsScrollState(): string { return root.settingsPanel ? root.settingsPanel.scrollState() : "{}" }
        // Each card's rectangle, so the window-size battery asserts every overlay stays on screen.
        function settingsCardRect(): string { return root.settingsPanel ? root.fleaWindow.rectOf(root.settingsPanel.cardItem) : "" }
        function networkCardRect(): string { return root.networkDialog ? root.fleaWindow.rectOf(root.networkDialog.cardItem) : "" }
        function networkScroll(): string { return root.networkDialog ? root.networkDialog.bodyScroll() : "" }
        // The clipping viewport inside the card, so a field's on-screen check is against what the body shows.
        function networkBodyRect(): string { return root.networkDialog ? root.fleaWindow.rectOf(root.networkDialog.bodyItem) : "" }
        function keymapCardRect(): string { return root.keymapSheet ? root.fleaWindow.rectOf(root.keymapSheet.cardItem) : "" }
        function convertCardRect(): string { return root.convertDialog ? root.fleaWindow.rectOf(root.convertDialog.cardItem) : "" }
        // A menu row's own centre, so a driven click lands on the row a test named rather than on a
        // pixel derived from a row count the Menus settings section can change under it.
        function contextMenuRowCentre(i: int): string { return root.fleaWindow.centreOf(root.pane.contextMenu().itemFor(i)) }
        function contextMenuRect(): string { return root.fleaWindow.rectOf(root.pane.contextMenu().frameItem) }
        function contextMenuFocusState(): string {
            var menu = root.pane.contextMenu()
            var window = menu.Window.window
            return JSON.stringify({opened: menu.opened, view: root.pane.focusView,
                menu: menu.keyboardFocused, pane: root.pane.activeFocus, list: root.pane.listArea.activeFocus,
                origin: !!menu.focusHolder && menu.focusHolder.activeFocus,
                activeItem: window ? String(window.activeFocusItem) : ""})
        }
        function contextMenuRowProbe(i: int): string { var item = root.pane.contextMenu().itemFor(i); return item ? item.probe() : "" }
        // Where a driven right click reaches the background menu: the centre of the surface that
        // answers for the directory being shown, which in the columns view is the pane's own column
        // and not the peek beside it. An empty directory has no row to aim from, and it is the case
        // that menu matters most in, so no reader here may derive the point from a row.
        function listingBackgroundCentre(): string {
            var area = root.pane.viewMode === "columns" ? root.pane.columnsArea.activeColumn()
                                                        : root.pane.listArea
            return root.fleaWindow.centreOf(area)
        }
        // The row that is its own rename editor, or -1; drives the States artboard's inline rename.
        function renamingIndex(): int { return root.pane.renamingIndex }
        function renameEditorLive(): bool { return root.pane.renameEditor() !== null }
        function renameEditorText(): string { var e = root.pane.renameEditor(); return e ? e.editorText : "" }
        function renameState(): string {
            var row = root.pane.renameEditor()
            var editor = row ? row.editorField : null
            var current = root.pane.visibleItemFor(root.pane.cursorIndex)
            var area = root.pane.viewMode === "columns" ? root.pane.columnsArea.activeColumn() : root.pane.listArea
            var point = editor ? editor.mapToItem(area, 0, 0) : null
            return JSON.stringify({index: root.pane.renamingIndex, pending: root.pane.renamePending,
                error: root.pane.renameError, text: editor ? editor.current : "",
                cursor: root.pane.cursorIndex, cursorName: root.pane.cursorRow ? root.pane.cursorRow.n : "",
                loading: root.pane.listInFlight, listingState: root.pane.listingState, currentRowHeight: current ? current.height : 0,
                focused: !!editor && editor.inputItem.activeFocus,
                rowHeight: row ? row.height : 0, normalRowHeight: Theme.fileRowHeight,
                fieldHeight: editor ? editor.fieldHeight : 0, errorHeight: editor ? editor.errorHeight : 0,
                editorTop: point ? point.y : 0, editorBottom: point ? point.y + editor.height : 0, viewportHeight: area.height,
                selectedText: editor ? editor.inputItem.selectedText : "",
                centre: editor ? root.fleaWindow.centreOf(editor.inputItem) : ""})
        }
        function railRenameEditorLive(): bool { return root.pane.sidebar.renameEditor() !== null }
        function railRenameEditorText(): string { var e = root.pane.sidebar.renameEditor(); return e ? e.editorText : "" }
        function railRenameFieldShown(): bool { var e = root.pane.sidebar.renameEditor(); return e ? e.editorShown : false }
        function previewOpen(): bool { return root.pane.preview.active }
        function previewKind(): string { return root.pane.preview.kind }
        function previewState(): string { return root.pane.preview.status }
        function previewPosition(): int { return root.pane.preview.position }
        function previewDuration(): int { return root.pane.preview.duration }
        // Fix round 1: what the strip actually draws, not a re-derived guess at its visible: expression.
        function previewStripVisible(): bool { return root.pane.preview.stripVisible }
        // A 0.25 zoom step and an expand flag are not legible off a screenshot, so the seam is the
        // only honest answer for either; "" means no PDF is loaded, which is not zoom 1 or false.
        function previewPdfPage(): int { var p = root.pane.preview.pdfItem; return p ? p.page : -1 }
        function previewPdfZoom(): string { var p = root.pane.preview.pdfItem; return p ? String(p.zoom) : "" }
        function previewPdfFocus(): int { var p = root.pane.preview.pdfItem; return p ? p.pdfControlIndex : -1 }
        function pdfState(overlay: bool): string {
            var p = overlay ? root.pane.preview.pdfItem : root.pane.previewColumnItem
            if (!p) return "null"
            return JSON.stringify({ page: overlay ? p.page : p.pdfPage(), pages: overlay ? p.pageCount : p.pdfPages,
                frame: overlay ? "" : root.fleaWindow.rectOf(p.pdfFrameItem),
                toolbar: overlay ? "" : root.fleaWindow.rectOf(p.pdfToolbarItem),
                zoom: overlay ? p.zoom : p.pdfZoom, scrollY: p.pdfScrollY, focused: p.activeFocus, control: p.pdfControlIndex,
                controls: p.pdfControls.map(function (control) { return { name: control.accessName, enabled: control.enabled,
                    visible: control.visible, centre: root.fleaWindow.centreOf(control) } }) })
        }
        function previewExpanded(): string { var p = root.pane.preview.pdfItem; return p ? String(p.expanded) : "" }
        function previewSelectionState(): string {
            var column = root.pane.previewColumnItem
            return JSON.stringify({view: root.pane.viewMode, width: root.pane.listSlot.width,
                available: root.pane.width - root.pane.sidebarWidth,
                inlineVisible: column !== null && column.visible,
                index: root.pane.previewIndex, path: column ? column.path : "",
                focused: column !== null && column.activeFocus,
                pending: column ? column.pendingToken : 0})
        }
        function rowNameColor(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? String(item.nameColor()) : ""
        }
        function rowCellColor(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? String(item.cellColor()) : ""
        }
        // Binds the actual defect: an eliding cell's content stays inside width; a broken one does not.
        function rowCellOverflow(i: int): string {
            var item = root.pane.itemFor(i)
            if (!item)
                return ""
            var keys = ["mode", "size", "date", "kind"]
            var flags = []
            for (var k = 0; k < keys.length; k++) {
                var cell = item.cell(keys[k])
                flags.push(cell && cell.contentWidth > cell.width ? "1" : "0")
            }
            return flags.join("|")
        }
        // The rendered Size cell text; describe() keeps the row's raw s so existing rowAt assertions keep their meaning.
        function rowSizeText(i: int): string {
            var item = root.pane.itemFor(i)
            var cell = item ? item.cell("size") : null
            return cell ? cell.text : ""
        }
        function headerTitles(): string { return root.pane.header.titles() }
        function sortMark(): string { return root.pane.header.sortBy + ":" + (root.pane.header.sortDesc ? "desc" : "asc") }
        // Four siblings share one parent, so plain x/width already agree; itemRect reads a Text's painted bounds, not its anchored box.
        function headerCellRect(name: string): string {
            var item = root.pane.header.cell(name)
            if (!item)
                return ""
            return Math.round(item.x) + "|" + Math.round(item.width)
        }
        // The header's drawn columns beside a row's. Both resolve theirs from their own width
        // through Theme.columns, so a disagreement shows up here rather than as a stray column.
        function columnSet(i: int): string {
            var item = root.pane.itemFor(i)
            return root.pane.header.columnSet() + "|" + (item ? item.columnSet() : "")
        }
        // The three below read the view on screen, where rowIcon and rowAt read the list's own delegates whatever the view.
        function rowHovered(i: int): bool { var item = root.pane.visibleItemFor(i); return item ? item.hovered === true : false }
        function rowThumb(i: int): string { var item = root.pane.visibleItemFor(i); return item && item.thumb !== undefined ? item.thumb : "" }
        function thumbnailPolicyState(): string {
            var pane = root.pane, column = pane.previewColumnItem
            return JSON.stringify({view: pane.viewMode, mode: ViewState.thumbnailMode, files: pane.thumbState.file,
                pending: Object.keys(pane.thumbState.file).filter(function(index) { return pane.thumbState.file[index] === null }).map(Number),
                contentY: pane.viewMode === "columns" && root.columns ? root.columns.activeContentY() : pane.listArea.contentY,
                cursor: pane.cursorIndex, previewIndex: pane.previewIndex, previewPath: column ? column.path : "",
                previewReady: column !== null && column.frameStatus === Image.Ready})
        }
        function viewContentY(): int { return Math.round(root.pane.viewMode === "columns" && root.columns ? root.columns.activeContentY() : root.pane.listArea.contentY) }
        function listAreaRect(): string { return root.fleaWindow.rectOf(root.pane.listArea) }
        function rowRect(i: int): string { return root.fleaWindow.rectOf(root.pane.visibleItemFor(i)) }
        function dragPaneGeometry(side: int, index: int): string {
            var pane = side >= 0 && side < root.panes.length ? root.panes[side] : null
            if (!pane || index < 0 || index >= pane.total) return "{}"
            var folder = pane.rowFor(index), last = pane.rowFor(pane.total - 1)
            return JSON.stringify({side: side, active: ViewState.state.view === "dual", focused: root.pane === pane,
                path: pane.path, view: pane.viewMode, loading: pane.listInFlight, total: pane.total,
                area: root.fleaWindow.rectOf(pane.listArea),
                folder: {index: index, name: folder ? folder.n : "", directory: !!folder && folder.d === true,
                    rect: root.fleaWindow.rectOf(pane.visibleItemFor(index))},
                last: {index: pane.total - 1, name: last ? last.n : "",
                    rect: root.fleaWindow.rectOf(pane.visibleItemFor(pane.total - 1))}})
        }
        // Ready is the decoded image on screen; a path alone is not a thumbnail, see GridTile.thumbDrawn.
        function rowThumbReady(i: int): bool { var item = root.pane.visibleItemFor(i); return item && item.iconStatus !== undefined ? item.iconStatus === Image.Ready : false }
        function columnPlayerLoaded(): bool { return root.columns ? root.columns.playerLoaded() : false }
        function columnThumbShown(): bool { return root.columns ? root.columns.thumbShown() : false }
        function columnFrameReady(): bool { return root.columns ? root.columns.frameReady() : false }
        function columnTextLines(): string { return root.columns ? root.columns.textLines() : "" }
        function columnLinesRect(): string { return root.columns ? root.fleaWindow.rectOf(root.columns.linesItem()) : "" }
        function columnArchiveRect(): string { return root.columns ? root.fleaWindow.rectOf(root.columns.archiveItem()) : "" }
        function previewSurfaceRect(): string { return root.fleaWindow.rectOf(root.pane.preview.surfaceItem()) }
        function previewMediaLoaded(): bool { return root.pane.preview.mediaLoaded() }
        function previewText(): string { return root.pane.preview.textShown() }
        function previewArchiveNames(): string { return root.pane.preview.archiveNames() }
        function columnArchiveNames(): string { return root.columns ? root.columns.archiveNames() : "" }
        function columnFailure(): string { return root.columns ? root.columns.failureText() : "" }
        function rowThumbRect(i: int): string { var item = root.pane.visibleItemFor(i); return item && item.thumbItem ? root.fleaWindow.rectOf(item.thumbItem) : "" }
        function columnFrameRect(): string { return root.columns ? root.fleaWindow.rectOf(root.columns.frameItem()) : "" }
        function columnChildEmpty(): string { var e = root.columns ? root.columns.childEmptyItem() : null; return e ? e.visible + " " + e.opacity.toFixed(2) + " " + e.markItem.opacity.toFixed(2) : "" }
        function columnChildMarkRect(): string { var e = root.columns ? root.columns.childEmptyItem() : null; return e ? root.fleaWindow.rectOf(e.markItem) : "" }
        function columnChildRowCentre(i: int): string { return root.columns ? root.fleaWindow.centreOf(root.columns.childItemAt(i)) : "" }
        function columnParentRowCentre(i: int): string { return root.columns ? root.fleaWindow.centreOf(root.columns.parentItemAt(i)) : "" }
        function rowIcon(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? String(item.iconUrl) : ""
        }
        function rowIconStatus(i: int): int {
            var item = root.pane.itemFor(i)
            return item ? item.iconStatus : -1
        }
        // The name cell's drawn text: the row name plus the surface's own decorations, where
        // describe() keeps the raw n so every existing rowAt assertion keeps its meaning.
        function rowNameText(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? item.decoratedName : ""
        }
        function rowGlyph(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? String(item.glyphName) : ""
        }
        // The empty-directory mark's own visibility, off the same listingState the overlay binds to.
        function emptyShown(): bool { return root.pane.listingState === "empty" }
        function stateLayers(): string {
            return JSON.stringify({empty: root.pane.emptyState.visible, message: root.pane.stateMessageItem.visible})
        }
        // "x y width height" of the empty mark in window pixels, for a painted-pixel count: the state
        // flag above cannot see a mark drawn under its own parent's paint.
        function emptyMarkRect(): string { return root.emptyState ? root.fleaWindow.rectOf(root.emptyState.markItem) : "" }
        function emptyHeroState(): string {
            var empty = root.emptyState
            if (!empty) return "{}"
            return JSON.stringify({visible: empty.visible, settled: empty.markItem.settled && empty.opacity === 1,
                opacity: empty.opacity, offset: empty.captionItem.parent.anchors.verticalCenterOffset,
                caption: empty.captionItem.text, captionOpacity: empty.captionItem.opacity,
                captionColor: String(empty.captionItem.color), markColor: String(empty.markItem.color),
                foreground: String(Theme.color.foreground), muted: String(Theme.color.muted),
                reducedMotion: Theme.reducedMotion, rotateMs: empty.rotateMs})
        }
        // The whole hero box, so a test can hold it to the listing slot exactly rather than merely inside it.
        function emptyStateRect(): string { return root.emptyState ? root.fleaWindow.rectOf(root.emptyState) : "" }
        function rowAt(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? item.describe() : "loading"
        }
        function visibleRows(): int { return root.pane.visibleRows }
        // The list's scroll position and the platform's lines per notch, for tests/ui.sh scroll.
        function listContentY(): int { return Math.round(root.pane.listArea.contentY) }
        function wheelLines(): int { return Application.styleHints.wheelScrollLines }
        function thumbRequests(): int { return root.backend.thumbRequests }
        function dirSizeRequests(): int { return root.backend.dirSizeRequests }
        function listRequests(): int { return root.backend.listRequests }
        function thumbFile(i: int): string { return root.pane.thumbFor(i) }
        function rowCentre(i: int): string { return root.pane.rowFor(i) ? root.fleaWindow.centreOf(root.pane.visibleItemFor(i)) : "" }
        // The same lookup as rowCentre, but for the preview's own seek slider, so a test can drive
        // a real wheel event over it without hardcoding the strip's layout.
        function previewSliderCentre(): string {
            return root.pane.preview.active && root.pane.preview.isMedia ? root.fleaWindow.centreOf(root.pane.preview.seekSlider) : ""
        }
        // The same lookup as rowCentre, but for a rail row: the rail has no ListView, so Sidebar.railItemFor(i) walks its own two Repeaters instead.
        function railRowCentre(i: int): string { return root.fleaWindow.centreOf(root.pane.sidebar.railItemFor(i)) }
        function railLabel(i: int): string { var item = root.pane.sidebar.railItemFor(i); return item ? item.modelData.label : "" }
        function railLabels(): string { var out = []; for (var i = 0; i < root.pane.railCount; i++) { var item = root.pane.sidebar.railItemFor(i); out.push(item ? item.modelData.label : "") } return out.join("|") }
        // The sidebar pushes the row and the header right by its own width, so a pixel-crop test needs this rather than assuming x=0.
        function rowLeft(i: int): string {
            var item = root.pane.itemFor(i)
            if (!item || !root.pane.rowFor(i))
                return ""
            return String(Math.round(root.fleaWindow.itemRect(item).x))
        }
        function headerLeft(): string { return String(Math.round(root.fleaWindow.itemRect(root.pane.header).x)) }
        function viewMode(): string { return root.pane.viewMode }
        // What this box probed: the compress submenu is exactly this and never a fixed list.
        function archiveFormats(): string { return root.backend.archiveFormats.join("|") }
        function canConvert(): bool { return root.backend.canConvert }
        // The one popup in the design, so a test can assert it opened and what it would write.
        function convertOpen(): bool { return root.convertDialog ? root.convertDialog.opened : false }
        function keymapSheetOpen(): bool { return root.keymapSheet ? root.keymapSheet.opened : false }
        // One row per line, "<cap> <wording>", so a test asserts the sheet without OCR.
        function keymapSheetRows(): string { return root.keymapSheet ? root.keymapSheet.rows() : "" }
        function convertFormat(): string { return root.convertDialog ? root.convertDialog.format : "" }
        function convertStrip(): bool { return root.convertDialog ? root.convertDialog.strip : false }
        function convertState(): string {
            var dialog = root.convertDialog
            if (!dialog) return JSON.stringify({opened: false})
            return JSON.stringify({opened: dialog.opened, source: dialog.source, format: dialog.format, strip: dialog.strip,
                checking: dialog.checking, busy: dialog.busy, unavailable: dialog.unavailable, collision: dialog.collision, error: dialog.errorText,
                output: dialog.outputPath, outputText: dialog.outputItem.text, outputLines: dialog.outputItem.lineCount,
                outputRect: root.fleaWindow.rectOf(dialog.outputItem),
                bodyRect: root.fleaWindow.rectOf(dialog.bodyItem), scrollY: dialog.bodyItem.contentY,
                requestId: dialog.requestId, operationId: dialog.operationId, cursor: dialog.cursor, focusPart: dialog.focusPart, canConvert: dialog.canConvert,
                rect: root.fleaWindow.rectOf(dialog.cardItem),
                formats: dialog.formats.map(function(format, index) {
                    return Object.assign(root.controlState(format, dialog.formatItem(index)), {selected: dialog.format === format,
                        current: dialog.formatItem(index).current, labelColor: String(dialog.formatItem(index).labelColor),
                        markColor: String(dialog.formatItem(index).markColor), pointerProbe: dialog.formatItem(index).probe()})
                }), controls: [root.controlState("Remove metadata", dialog.metadataItem),
                    root.controlState("Cancel", dialog.cancelItem), root.controlState("Convert", dialog.submitItem)]})
        }
        function convertTitleCentre(): string { return root.convertDialog ? root.fleaWindow.centreOf(root.convertDialog.titleItem) : "" }
        // The preview column's own table and state, so a test asserts the canvas's rows without OCR.
        function previewFacts(): string { return root.columns ? root.columns.factsLine() : "" }
        function previewColumnState(): string { return root.columns ? root.columns.previewStateName() : "" }
        // The preview column's transport, so a test can prove it plays rather than eyeball a glyph.
        function columnMediaPlaying(): bool { return root.columns ? root.columns.mediaPlaying() : false }
        function columnMediaPosition(): int { return root.columns ? root.columns.mediaPosition() : -1 }
        function columnPlayCentre(): string {
            var strip = root.columns ? root.columns.mediaStrip() : null
            return strip ? root.fleaWindow.centreOf(strip.playItem) : ""
        }
        function columnStripCentre(): string { return root.columns ? root.fleaWindow.centreOf(root.columns.mediaStrip()) : "" }
        // The preview column's PDF page position, so a test proves a page turned rather than
        // eyeballing a render. Both readers are pure, like every other one on this handler.
        function columnPdfPage(): int { return root.columns ? root.columns.pdfPage() : -1 }
        function columnPdfPages(): int { return root.columns ? root.columns.pdfPages() : -1 }
        function columnPdfLoaded(): bool { return root.columns ? root.columns.pdfLoaded() : false }
        function columnChevronCentre(dir: string): string {
            var item = root.columns ? root.columns.pdfChevron(dir) : null
            return item && item.visible ? root.fleaWindow.centreOf(item) : ""
        }
        function chromeHeight(): int { return Math.round(Theme.chromeHeight) }
        function tabCount(): int { return Tabs.count(root.pane) }
        function tabIndex(): int { return Tabs.currentIndex(root.pane) }
        function tabLabels(): string { return Tabs.labels(root.pane).join("|") }
        function tabBarVisible(): bool { return root.tabBar ? root.tabBar.visible : false }
        function tabCentre(i: int): string {
            if (!root.tabBar)
                return ""
            return root.fleaWindow.centreOf(root.tabBar.itemAt(i))
        }
        // The chrome's buttons carry a glyph and no text, so a test reaches one by name and clicks
        // its centre, exactly the way rowCentre already works for a row.
        function chromeButtonCentre(glyph: string): string { return root.fleaWindow.centreOf(root.chrome.buttonFor(glyph)) }
        function chromeButtonState(glyph: string): string {
            return JSON.stringify(root.controlState(glyph, root.chrome.buttonFor(glyph)))
        }
        // The path bar: whether it has the keyboard, what it is holding, and the box a double click
        // opens it on, which is the pointer's half of ":" and Ctrl+L.
        function pathBarOpen(): bool { return root.chrome.editing }
        function pathBarText(): string { return String(root.chrome.editText) }
        function pathCentre(): string { return root.fleaWindow.centreOf(root.chrome.pathArea) }
        // The elision marker, or "" while the whole path fits: the one spot the crumbs slide under.
        function elisionCentre(): string {
            return root.chrome.elisionMarker.visible ? root.fleaWindow.centreOf(root.chrome.elisionMarker) : ""
        }
        // Issue 45's segments, reached the way tabCentre reaches a tab: a driven press on a real
        // crumb is the only thing that can tell a bound TapHandler from an unbound one.
        function crumbCount(): int { return root.chrome.crumbItems.count }
        // Measured: with the bar open the slot is hidden and a crumb's box is still there, and a path
        // too long for the bar slides its head clean off the left, so a bare centre aims a driven
        // click at the desktop. Answering "" for both is what stops a test pressing nothing at all.
        function crumbCentre(i: int): string {
            var item = root.chrome.crumbItems.itemAt(i)
            if (!item || !item.visible)
                return ""
            var box = item.mapToItem(root.chrome.pathArea, 0, 0)
            var inside = box.x >= 0 && box.x + item.width <= root.chrome.pathArea.width
            return inside ? root.fleaWindow.centreOf(item) : ""
        }
        // The button's painted box as "WxH": the mark is Theme.chromeMarkSize wide and the hit area is the whole strip tall.
        function chromeButtonSize(glyph: string): string {
            var item = root.chrome.buttonFor(glyph)
            if (!item)
                return ""
            var rect = root.fleaWindow.itemRect(item)
            return Math.round(rect.width) + "x" + Math.round(rect.height)
        }
        function headerTop(): string { return String(Math.round(root.fleaWindow.itemRect(root.pane.header).y)) }
        function railRenamingIndex(): int { return root.pane.sidebar.renamingIndex }
        function dialogOpen(): bool { return root.networkDialog ? root.networkDialog.opened : false }
        // The network form's own state, so a test asserts the protocol swap and the URI it built.
        function networkProtocol(): string { return root.networkDialog ? root.networkDialog.formProtocol() : "" }
        function networkPort(): string { return root.networkDialog ? root.networkDialog.formPort() : "" }
        function networkUri(): string { return root.networkDialog ? root.networkDialog.formUri() : "" }
        function networkPathLabel(): string { return root.networkDialog ? root.networkDialog.formPathLabel() : "" }
        function networkTitle(): string { return root.networkDialog ? root.networkDialog.dialogTitle : "" }
        function networkFields(): string { return root.networkDialog ? root.networkDialog.formFields() : "" }
        function networkFocus(): string { return root.networkDialog ? root.networkDialog.formFocus() : "" }
        function networkFocusState(): string {
            var dialog = root.networkDialog
            var window = dialog ? dialog.Window.window : null
            var item = window ? window.activeFocusItem : null
            var inside = false
            for (var parent = item; parent; parent = parent.parent) {
                if (parent === dialog) { inside = true; break }
            }
            return JSON.stringify({inside: inside, busy: !!dialog && dialog.busy,
                activeItem: item ? String(item) : "", field: dialog ? dialog.formFocus() : ""})
        }
        function networkHostPortWidths(): string { return root.networkDialog ? root.networkDialog.formHostPortWidths() : "" }
        // Mask state and presence only: the seam never returns password content.
        function networkPasswordState(): string { return root.networkDialog ? root.networkDialog.formPasswordState() : "" }
        function networkPasswordEyeCentre(): string { return root.networkDialog ? root.networkDialog.formPasswordEyeCentre() : "" }
        function networkNote(): string { return root.networkDialog ? root.networkDialog.formNote() : "" }
        function networkAction(): string { return root.networkDialog ? root.networkDialog.formAction() : "" }
        function networkStatus(): string { return root.networkDialog ? root.networkDialog.statusText : "" }
        function networkDialogMetrics(): string { return root.networkDialog ? root.networkDialog.formMetrics() : "" }
        function networkDialogMetricTargets(): string { return root.networkDialog ? root.networkDialog.formMetricTargets() : "" }
        // Durable and non-secret, unlike the four-second status-bar transient.
        function networkResult(): string { return root.pane.sidebar.networkResult() }
        // The "+" ink, its hit target and the rail's own indicator dot, each "x width centre" in window
        // coordinates. Three measured rectangles, because a computed slot only restates the anchoring.
        function networkMarkGeometry(): string {
            var items = root.pane.sidebar.networkMarkItems()
            if (!items[0] || !items[1] || !items[2])
                return ""
            return [root.fleaWindow.boxOf(items[0]), root.fleaWindow.boxOf(items[1]),
                root.fleaWindow.boxOf(items[2])].join("|")
        }
        // Where a click probe aims: the hit target's own middle, so the probe varies only x.
        function networkMarkCentre(): string { return root.fleaWindow.centreOf(root.pane.sidebar.networkMarkItems()[1]) }
        // A protocol chip carries a label and no tree, so a test clicks its centre the way it does a row.
        function networkChipCentre(name: string): string { return root.networkDialog ? root.fleaWindow.centreOf(root.networkDialog.formChip(name)) : "" }
        function shareBrowserOpen(): bool { return root.shareBrowser ? root.shareBrowser.active : false }
        // One share name per line, in cursor order; empty when the overlay is shut.
        function shareBrowserEntries(): string { return root.shareBrowser ? root.shareBrowser.shares.join("\n") : "" }
        function shareBrowserCursor(): int { return root.shareBrowser ? root.shareBrowser.cursorIndex : -1 }
        function shareBrowserRect(): string { return root.shareBrowser ? root.fleaWindow.rectOf(root.shareBrowser) : "" }
        function shareBrowserState(): string {
            var browser = root.panes[0] ? root.panes[0].shareBrowser : null
            return JSON.stringify({active: !!browser && browser.active,
                owner: browser && browser.owner ? root.panes.indexOf(browser.owner) : -1,
                rect: browser ? root.fleaWindow.rectOf(browser) : "",
                baseUri: root.shareBrowser ? root.shareBrowser.baseUri : "",
                cursor: root.shareBrowser ? root.shareBrowser.cursorIndex : -1,
                paneRects: root.panes.map(function(pane) { return pane ? root.fleaWindow.rectOf(pane.listSlot) : "" })})
        }
        // One line per entry, "label|group|kind|mounted", so a test can assert count and shape without a screenshot.
        function networkEntries(): string {
            var out = []
            var entries = root.pane.sidebar.networkEntries
            for (var i = 0; i < entries.length; i++) {
                var e = entries[i]
                out.push(e.label + "|" + e.group + "|" + e.kind + "|" + e.mounted)
            }
            return out.join("\n")
        }
        function networkStartIndex(): int { return root.pane.sidebar.placesEntries.length }

        // One line per entry, "label|group|kind|mounted", the same shape networkEntries answers.
        function deviceEntries(): string {
            var out = []
            var entries = root.pane.sidebar.deviceEntries
            for (var i = 0; i < entries.length; i++) {
                var e = entries[i]
                out.push(e.label + "|" + e.group + "|" + e.kind + "|" + e.mounted)
            }
            return out.join("\n")
        }
    }
}
