import QtQuick
import "." as Flea
import "js/PickerMenu.js" as PickerMenu

// The chooser's one context menu: ui/ContextMenu.qml over ui/PickerState.qml. The four families
// the picker does not carry, archive, convert, taildrop and dropbox (user decision 2026-09-09),
// are fed as absent, so ui/js/Menu.js listingEntries hides their rows itself; Settings leaves by
// name, because the background column draws it unconditionally and a chooser has no panel. One
// instance, as the window has: a second in this tree takes the keyboard from the list.
Flea.ContextMenu {
    id: root

    property var picker: null
    // ui/PickerList.qml, the ops ui/js/PickerKeys.js act moves the cursor through.
    property var list: null

    showHidden: root.picker !== null && root.picker.showHidden
    archiveFormats: []
    canConvert: false
    taildropPeers: []
    dropboxPath: ""
    rowInDropbox: false
    // Not the Menus settings section's stored set: that section is the browser window's, and the
    // chooser draws every row it can answer, Open in terminal included where the shipped set hides it.
    hiddenActions: ["settings"]

    onChosen: function (action) { PickerMenu.route(action, root.picker, root.list, ViewState) }

    // A right click on a row, ui/js/PickerMenu.js aim: the cursor and the marks are settled first,
    // because the entries are built from the cursor row once the frame is up.
    function openOnRow(index, scenePoint) {
        if (PickerMenu.aim(root.picker, index))
            root.openAt(scenePoint)
    }
}
