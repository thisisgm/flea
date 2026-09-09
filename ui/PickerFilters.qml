import Quickshell
import Quickshell.Io
import QtQuick
import "js/PickerFilters.js" as Filters

// The user's own chooser pills: $XDG_CONFIG_HOME/flea/filters.toml, read once at start and parsed
// by ui/js/PickerFilters.js. ui/PickerState.qml owns one and draws these only when the caller sent
// no filters of its own. The read blocks for the reason ui/ViewState.qml's does: an async read would
// paint one chip row and correct it. printErrors is off, because most users never write this file.
QtObject {
    id: root

    readonly property string configDir: (Quickshell.env("XDG_CONFIG_HOME") && Quickshell.env("XDG_CONFIG_HOME").length > 0
                                         ? Quickshell.env("XDG_CONFIG_HOME") : Quickshell.env("HOME") + "/.config") + "/flea"

    // Every [[filter]] table with a glob, in file order, each {name, globs, mimes}.
    property var filters: []

    property FileView file: FileView {
        path: root.configDir + "/filters.toml"
        blockLoading: true
        printErrors: false
    }

    // Read here and not in onLoaded, which ui/ViewState.qml measured arriving after the first
    // property read; blockLoading is what makes text() answer inside this call. The owner reads
    // filters through bindings and never in its own Component.onCompleted, which was measured
    // firing before this one: a root completes before the objects its properties hold.
    Component.onCompleted: root.filters = Filters.parse(root.file.text())
}
