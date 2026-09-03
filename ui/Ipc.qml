import QtQuick
import Quickshell.Io
import qs.Commons

// The seam the tests drive, see AGENTS.md "Testing". Read-only: it reports, never acts.
QtObject {
    id: root

    property var fleaWindow: null
    property var pane: null
    property var bar: null
    property var backend: null
    property var chrome: null
    property var convertDialog: null
    property var keymapSheet: null
    property var networkDialog: null
    property var shareBrowser: null

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
        function stateMessage(): string { return root.pane.stateMessage }
        function contextMenuVisible(): bool { return root.pane.menuVisible }
        function showHidden(): bool { return root.pane.showHidden }
        // One label per current menu row, joined so a test can assert contents without OCR. A
        // separator has no label of its own and reads as "-", which is what makes the grouping assertable.
        function contextMenuEntries(): string {
            var entries = root.pane.menuEntries()
            var out = []
            for (var i = 0; i < entries.length; i++) {
                out.push(entries[i].separator === true ? "-" : entries[i].label)
            }
            return out.join("|")
        }
        // The glyph each row draws, in the same order, so the "every row is marked" rule is assertable.
        function contextMenuGlyphs(): string {
            var entries = root.pane.menuEntries()
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
        function contextMenuSubmenuGlyphs(): string { return root.pane.menuSubmenuGlyphs() }
        // A peer is named by whoever is on the tailnet, so a test reads the name here rather than
        // knowing it. No separator branch: a flyout holds only Archive.formatEntries or Taildrop
        // peers, and both build {id, label} rows only.
        function contextMenuSubmenuEntries(): string {
            return root.pane.menuSubmenuEntries().map(function (e) { return e.label }).join("|")
        }
        function contextMenuCursor(): int { return root.pane.menuCursor }
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
        function rowIcon(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? String(item.iconUrl) : ""
        }
        function rowIconStatus(i: int): int {
            var item = root.pane.itemFor(i)
            return item ? item.iconStatus : -1
        }
        function rowGlyph(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? String(item.glyphName) : ""
        }
        // The empty-directory mark's own visibility, off the same listingState the overlay binds to.
        function emptyShown(): bool { return root.pane.listingState === "empty" }
        function rowAt(i: int): string {
            var item = root.pane.itemFor(i)
            return item ? item.describe() : "loading"
        }
        function visibleRows(): int { return root.pane.visibleRows }
        function thumbRequests(): int { return root.backend.thumbRequests }
        function dirSizeRequests(): int { return root.backend.dirSizeRequests }
        function thumbFile(i: int): string { return root.pane.thumbFor(i) }
        function rowCentre(i: int): string { return root.pane.rowFor(i) ? root.fleaWindow.centreOf(root.pane.visibleItemFor(i)) : "" }
        // The same lookup as rowCentre, but for the preview's own seek slider, so a test can drive
        // a real wheel event over it without hardcoding the strip's layout.
        function previewSliderCentre(): string {
            return root.pane.preview.active && root.pane.preview.isMedia ? root.fleaWindow.centreOf(root.pane.preview.seekSlider) : ""
        }
        // The same lookup as rowCentre, but for a rail row: the rail has no ListView, so Sidebar.railItemFor(i) walks its own two Repeaters instead.
        function railRowCentre(i: int): string { return root.fleaWindow.centreOf(root.pane.sidebar.railItemFor(i)) }
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
        function convertOpen(): bool { return root.convertDialog.opened }
        function keymapSheetOpen(): bool { return root.keymapSheet.opened }
        // One row per line, "<cap> <wording>", so a test asserts the sheet without OCR.
        function keymapSheetRows(): string { return root.keymapSheet.rows() }
        function convertFormat(): string { return root.convertDialog.format }
        function convertStrip(): bool { return root.convertDialog.strip }
        // The preview column's own table and state, so a test asserts the canvas's rows without OCR.
        function previewFacts(): string { return root.pane.columnsArea.factsLine() }
        function previewColumnState(): string { return root.pane.columnsArea.previewStateName() }
        // The preview column's transport, so a test can prove it plays rather than eyeball a glyph.
        function columnMediaPlaying(): bool { return root.pane.columnsArea.mediaPlaying() }
        function columnMediaPosition(): int { return root.pane.columnsArea.mediaPosition() }
        function columnPlayCentre(): string {
            var strip = root.pane.columnsArea.mediaStrip()
            return strip ? root.fleaWindow.centreOf(strip.playItem) : ""
        }
        function columnStripCentre(): string { return root.fleaWindow.centreOf(root.pane.columnsArea.mediaStrip()) }
        // The preview column's PDF page position, so a test proves a page turned rather than
        // eyeballing a render. Both readers are pure, like every other one on this handler.
        function columnPdfPage(): int { return root.pane.columnsArea.pdfPage() }
        function columnPdfPages(): int { return root.pane.columnsArea.pdfPages() }
        function columnPdfLoaded(): bool { return root.pane.columnsArea.pdfLoaded() }
        function columnChevronCentre(dir: string): string {
            var item = root.pane.columnsArea.pdfChevron(dir)
            return item && item.visible ? root.fleaWindow.centreOf(item) : ""
        }
        function chromeHeight(): int { return Math.round(Theme.chromeHeight) }
        // The chrome's buttons carry a glyph and no text, so a test reaches one by name and clicks
        // its centre, exactly the way rowCentre already works for a row.
        function chromeButtonCentre(glyph: string): string { return root.fleaWindow.centreOf(root.chrome.buttonFor(glyph)) }
        // The path bar: whether it has the keyboard, what it is holding, and the box a double click
        // opens it on, which is the pointer's half of ":" and Ctrl+L.
        function pathBarOpen(): bool { return root.chrome.editing }
        function pathBarText(): string { return String(root.chrome.editText) }
        function pathCentre(): string { return root.fleaWindow.centreOf(root.chrome.pathArea) }
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
        function dialogOpen(): bool { return root.networkDialog.opened }
        // The network form's own state, so a test asserts the protocol swap and the URI it built.
        function networkProtocol(): string { return root.networkDialog.formProtocol() }
        function networkPort(): string { return root.networkDialog.formPort() }
        function networkUri(): string { return root.networkDialog.formUri() }
        function networkPathLabel(): string { return root.networkDialog.formPathLabel() }
        // A protocol chip carries a label and no tree, so a test clicks its centre the way it does a row.
        function networkChipCentre(name: string): string { return root.fleaWindow.centreOf(root.networkDialog.formChip(name)) }
        function shareBrowserOpen(): bool { return root.shareBrowser.active }
        // One share name per line, in cursor order; empty when the overlay is shut.
        function shareBrowserEntries(): string { return root.shareBrowser.shares.join("\n") }
        function shareBrowserCursor(): int { return root.shareBrowser.cursorIndex }
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
