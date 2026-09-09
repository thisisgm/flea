import QtQuick
import Quickshell.Io
import "js/Filter.js" as Filter
import "js/Picker.js" as Picker

// The seam tests/picker.sh drives, the same read-only shape ui/Ipc.qml has for the window: it
// reports, never acts. Lifted out of ui/picker.qml at the 400-line cap; every reader answers as
// it did there, reading ui/PickerState.qml through state.
QtObject {
    id: root

    property var state: null
    property var footer: null
    property var fetcher: null
    property var entry: null
    property var save: null
    property var list: null
    property var chrome: null
    property var places: null
    property var header: null
    property var menu: null

    // A drawn item's painted box reduced to the point a test clicks, the same read rowCentre makes.
    function centreOf(item) {
        var rect = root.state.itemRect(item)
        return Math.round(rect.x + rect.width / 2) + " " + Math.round(rect.y + rect.height / 2)
    }

    // The wrapper holds the references because an IpcHandler marshals every property it owns.
    property IpcHandler seam: IpcHandler {
        target: "fleapicker"
        function ready(): bool { return true }
        function path(): string { return root.state.path }
        function total(): int { return root.state.total }
        function shownTotal(): int { return root.state.shownTotal }
        function cursor(): int { return root.state.cursorIndex }
        function focusView(): string { return root.state.focusView }
        function railCursor(): int { return root.places.cursorIndex }
        function marks(): string { return Picker.paths(root.state.marks).join(",") }
        function rowAt(index: int): string { var row = root.state.rowFor(index); return row ? row.n : "" }
        function cursorName(): string { return root.state.rowFor(root.state.cursorIndex) ? root.state.rowFor(root.state.cursorIndex).n : "" }
        function state(): string { return root.state.listingState }
        function recent(): bool { return root.state.recent }
        function accept(): string { return Picker.acceptLabel(root.state.req, root.state.marks.length) }
        function chip(): int { return root.state.filterIndex }
        function filterQuery(): string { return root.state.filterQuery }
        function filterTyping(): int { return root.state.filterTyping ? 1 : 0 }
        function chips(): string { return root.state.chips.map(function (c) { return c.label }).join(",") }
        function saveName(): string { return root.state.saveName }
        function message(): string { return root.footer.message }
        // The footer's standing line, the count and selection a message or a download would cover.
        function count(): string { return root.footer.standing }
        function fetching(): int { return root.fetcher.fetching ? 1 : 0 }
        function fetchLine(): string { return root.fetcher.line }
        function entry(): string { return root.entry.text }
        function entryFocused(): int { return root.entry.focused ? 1 : 0 }
        function saveFocused(): int { return root.save.focused ? 1 : 0 }
        function rowCentre(index: int): string { return root.list.rowCentre(index) }
        function visibleRows(): int { return root.list.visibleRows }
        // The settle gate's two reads, as ui/Ipc.qml has them: requests attempted, and a row's cached file.
        function thumbRequests(): int { return root.state.backend.thumbRequests }
        function thumbFile(index: int): string { return root.list.thumbFor(index) }
        function dragRows(): string { return root.list.dragRows.join(",") }
        // The header's readers, as ui/Ipc.qml has them, and the centre a test aims a sort click at.
        function headerTitles(): string { return root.header.titles() }
        function sortMark(): string { return root.header.sortBy + ":" + (root.header.sortDesc ? "desc" : "asc") }
        function headerCellRect(name: string): string {
            var item = root.header.cell(name)
            return item ? Math.round(item.x) + "|" + Math.round(item.width) : ""
        }
        function headerCentre(name: string): string {
            var item = root.header.cell(name)
            return item ? root.centreOf(item) : ""
        }
        // The header's drawn columns beside a listing row's, both resolved through Theme.columns.
        function columnSet(index: int): string {
            var item = root.list.itemAtIndex(Filter.viewOf(root.state.shown, index))
            return root.header.columnSet() + "|" + (item ? item.rowItem.columnSet() : "")
        }
        // The menu's readers, as ui/Ipc.qml has them: whether it is up, its rows as labels with "-"
        // for a rule, the open flyout's rows, and the row the keyboard is on.
        function contextMenuVisible(): bool { return root.menu.opened }
        function contextMenuEntries(): string {
            return root.menu.entries.map(function (e) { return e.separator === true ? "-" : e.label }).join("|")
        }
        function contextMenuSubmenuEntries(): string { return root.menu.submenuEntries.map(function (e) { return e.label }).join("|") }
        function menuCursor(): int { return root.menu.cursor }
        // A point on the list under its last row, where a right click raises the background column,
        // or "" when the rows fill the list and there is no such point.
        function emptyCentre(): string {
            var rect = root.state.itemRect(root.list)
            if (root.list.contentHeight >= rect.height)
                return ""
            return Math.round(rect.x + rect.width / 2) + " " + Math.round(rect.y + (root.list.contentHeight + rect.height) / 2)
        }
        // The nav strip's segments as their drawn texts, "" while Recent draws its label instead.
        function crumbs(): string { return root.chrome.crumbs.visible ? root.chrome.crumbs.model.map(function (c) { return c.text }).join(",") : "" }
        function crumbCount(): int { return root.chrome.crumbs.visible ? root.chrome.crumbs.items.count : 0 }
        function locationLabel(): string { return root.chrome.locationLabel }
        // A segment's centre in window pixels, or "" for one the strip does not show, see rowCentre.
        function crumbCentre(index: int): string {
            var item = root.chrome.crumbs.shownItem(index)
            return item ? root.centreOf(item) : ""
        }
        // A rail row's centre by the label it draws, "" for a row the rail has not drawn or does not offer.
        function placeCentre(label: string): string {
            var entries = root.places.entries
            for (var i = 0; i < entries.length; i++) {
                if (entries[i].label === label) {
                    var item = root.places.rail.itemAtIndex(i)
                    return item ? root.centreOf(item) : ""
                }
            }
            return ""
        }
    }
}
