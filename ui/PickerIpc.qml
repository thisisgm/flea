import QtQuick
import Quickshell.Io
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
