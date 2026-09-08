import QtQuick
import Quickshell.Io
import qs.Commons
import "js/Tabs.js" as Tabs

// The seam the tests drive, see AGENTS.md "Testing". Read-only: it reports, never acts.
QtObject {
    id: root

    property var fleaWindow: null
    property var pane: null
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
    // Overlays and the columns view are built by their first open, see ui/shell.qml, so until then
    // each reader below answers the empty value its type has: "", false or -1, never a throw.
    readonly property var columns: root.pane ? root.pane.columnsArea : null

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
        function total(): int { return root.pane.total }
        function selectionCount(): int { return root.pane.selectionCount() }
        function selectedIndices(): string { return root.pane.selectedIndices().join(",") }
        function focusView(): string { return root.pane.focusView }
        function railCursor(): int { return root.pane.railCursor }
        function railCount(): int { return root.pane.railCount }
        function path(): string { return root.pane.path }
        function lastMessage(): string { return root.bar.transient_ }
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
        // The panel's own title, a spot on the card with no control under it: a click there must leave the panel open.
        function settingsTitleCentre(): string { return root.settingsPanel ? root.fleaWindow.centreOf(root.settingsPanel.titleItem) : "" }
        // A rail row's centre, clicked over the list by tests/ui.sh clickthrough to prove the press stops at the panel.
        function settingsRailRowCentre(id: string): string { return root.settingsPanel ? root.fleaWindow.centreOf(root.settingsPanel.railItemFor(id)) : "" }
        // contentHeight|height of the settings pane, so the battery can require every section to fit the card whole at a tile.
        function settingsScroll(): string { return root.settingsPanel ? root.settingsPanel.paneScroll() : "" }
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
        function contextMenuRect(): string { return root.fleaWindow.rectOf(root.pane.contextMenu()) }
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
        function previewExpanded(): string { var p = root.pane.preview.pdfItem; return p ? String(p.expanded) : "" }
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
        function viewContentY(): int { return Math.round(root.pane.viewMode === "columns" && root.columns ? root.columns.activeContentY() : root.pane.listArea.contentY) }
        function listAreaRect(): string { return root.fleaWindow.rectOf(root.pane.listArea) }
        function rowRect(i: int): string { return root.fleaWindow.rectOf(root.pane.visibleItemFor(i)) }
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
        // "x y width height" of the empty mark in window pixels, for a painted-pixel count: the state
        // flag above cannot see a mark drawn under its own parent's paint.
        function emptyMarkRect(): string { return root.emptyState ? root.fleaWindow.rectOf(root.emptyState.markItem) : "" }
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
        function networkStartIndex(): int { return root.pane.sidebar.favoriteEntries.length }

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
