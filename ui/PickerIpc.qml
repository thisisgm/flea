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

    // The wrapper holds the references because an IpcHandler marshals every property it owns.
    property IpcHandler seam: IpcHandler {
        target: "fleapicker"
        function ready(): bool { return true }
        function path(): string { return root.state.path }
        function total(): int { return root.state.total }
        function shownTotal(): int { return root.state.shownTotal }
        function cursor(): int { return root.state.cursorIndex }
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
        function fetching(): int { return root.fetcher.fetching ? 1 : 0 }
        function fetchLine(): string { return root.fetcher.line }
        function entry(): string { return root.entry.text }
        function entryFocused(): int { return root.entry.focused ? 1 : 0 }
        function saveFocused(): int { return root.save.focused ? 1 : 0 }
        function rowCentre(index: int): string { return root.list.rowCentre(index) }
        function dragRows(): string { return root.list.dragRows.join(",") }
    }
}
