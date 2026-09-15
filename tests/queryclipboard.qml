import QtQuick
import "." as Flea

// Run with QT_QPA_PLATFORM=offscreen: Qt's in-process clipboard never touches the user's clipboard.
Item {
    id: suite
    property int checks: 0
    property int failures: 0
    function check(label, actual, expected) {
        checks++
        if (actual !== expected) { failures++; console.error(label + ": got " + actual + ", expected " + expected) }
    }
    TextInput { id: source; visible: false }
    Flea.QueryClipboard { id: clipboard; pane: fixture }
    QtObject {
        id: fixture
        property string path: "/fixture"
        property string searchMode: "typing"
        property string searchQuery: "prefix "
        property string filterQuery: ""
        property bool filterTyping: false
        property var rows: [{n: "café-猫.txt"}, {n: "other.txt"}]
        property int held: 0
        property int cursorIndex: 1
        property int selectionVersion: 0
        property var selected: [0, 1]
        function selectedIndices() { return selected.slice() }
        function showRow(index) {}
        property var selection: ({ toggle: function(index) { fixture.selected = fixture.selected.filter(function(i) { return i !== index }) } })
    }
    function put(text) {
        source.text = text
        source.selectAll()
        source.copy()
    }
    Component.onCompleted: {
        put("café-猫.txt")
        clipboard.paste()
        check("search appends native clipboard text with Unicode", fixture.searchQuery, "prefix café-猫.txt")
        check("paste does not run the search", fixture.searchMode, "typing")
        check("the hidden reader never takes keyboard focus", clipboard.reader.activeFocus, false)
        check("the reader clears copied contents after use", clipboard.reader.text, "")
        fixture.searchMode = ""
        fixture.filterTyping = true
        clipboard.paste()
        check("filter inserts the same native clipboard text", fixture.filterQuery, "café-猫.txt")
        check("filter paste keeps only the visible selection", fixture.selected.join(","), "0")
        check("filter paste moves the cursor onto the matching row", fixture.cursorIndex, 0)
        check("filter paste keeps editing active", fixture.filterTyping, true)
        fixture.filterTyping = false
        clipboard.paste()
        check("a closed query ignores paste", fixture.filterQuery, "café-猫.txt")
        fixture.searchMode = "typing"
        fixture.searchQuery = ""
        put("first\nsecond")
        clipboard.paste()
        check("multiline clipboard text stays one query line", fixture.searchQuery, "first second")
        check("a pasted newline never submits search", fixture.searchMode, "typing")
        console.log(checks + " checks, " + failures + " failed")
        Qt.exit(failures ? 1 : 0)
    }
}
