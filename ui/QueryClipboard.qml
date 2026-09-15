import QtQuick
import "js/Filter.js" as Filter
import "js/Search.js" as Search

// The query strips draw text themselves. Use Qt's clipboard reader without taking keyboard focus
// or involving the separate file-operation clipboard, a shell command, or a backend request.
QtObject {
    id: root
    required property var pane
    property int revision: 0
    readonly property TextInput reader: TextInput { visible: false; focus: false }
    readonly property Connections changes: Connections {
        target: root.pane
        function onSearchModeChanged() { root.revision++ }
        function onFilterTypingChanged() { root.revision++ }
        function onSearchQueryChanged() { root.revision++ }
        function onFilterQueryChanged() { root.revision++ }
        function onPathChanged() { root.revision++ }
    }

    function paste() {
        if (pane.searchMode !== Search.TYPING && !pane.filterTyping) return
        var version = root.revision
        reader.clear()
        reader.paste()
        var text = reader.text.replace(/\r\n|[\r\n\u2028\u2029]/g, " ")
        reader.clear()
        // Clipboard providers may take time to answer. Discard a read if its query changed meanwhile.
        if (version !== root.revision || text.length === 0) return
        if (pane.searchMode === Search.TYPING) Search.typed(pane, text)
        else if (pane.filterTyping) Filter.typed(pane, text)
    }
}
