import QtQuick
import "flea/js/Filter.js" as Filter
import "flea/js/Focus.js" as Focus

// The stub pane tests/matchtext-zerowidth.qml hands its views: the Columns view and the list read one of these, and the search and no-match rows read their own. Each property is what the real pane holds for those views; the functions answer what they call.
QtObject {
    property string path: "/probe/studio"
    property string listingPath: "/probe/studio"
    property string viewMode: "columns"
    property string focusView: "list"
    property var rows: []
    property var shown: null
    property int shownTotal: 3
    property int total: 3
    property int held: 0
    property int cursorIndex: 0
    property int cursorSeq: 0
    property int renamingIndex: -1
    property bool paneFocused: true
    property bool dualMode: false
    property string filterQuery: ""
    property bool filterTyping: false
    property real filterKeyAt: 0
    property real filterNarrowedAt: 0
    property string searchMode: ""
    property string searchQuery: ""
    property var swap: ({ answeredQuery: "", searchWarm: false })
    property string recentMode: ""
    property var clipboard: ({ paths: [], moving: false })
    property var picked: ({})
    property var selection: null
    property int selectionVersion: 0
    property int selectionAnchor: 0
    property string keySequence: ""
    property string keySequenceIdentity: ""
    property real inputAt: 0
    property real rowsAt: 0
    property var thumbState: ({ file: {}, order: [] })
    property var dirSizeState: ({ file: {}, order: [] })
    property var kindNames: ["text-x-generic"]
    property bool showHidden: false
    property bool storageKnown: true
    property string storageClass: "local"
    property bool listInFlight: false
    property string listingState: "ready"
    property var trash: ({ opened: false })
    property var selectionBand: null
    property var sidebar: null
    property int previewIndex: -1
    property int trashArmedAt: 0
    property var pendingSort: null
    property var pendingBackground: []
    property string stateMessage: ""
    property int lockedMode: 0
    property string dropPath: ""
    property bool dragHolding: false
    property int visibleRows: 8
    property int cacheRows: 0
    property int firstSettleMs: 70
    property int settleMs: 120
    property int coalesceMs: 16
    property int refetchMargin: 25
    property int buffer: 150
    property int windowSize: 35
    property var backend: null
    property var statusBar: null
    property var listArea: ({ primeSettle: function () {} })
    function join(base, name) { return String(base) + "/" + String(name) }
    function rowFor(index) { var o = index - held; return (o >= 0 && o < rows.length) ? rows[o] : null }
    function isSelected(index) { return picked[index] === true }
    function selectedIndices() { var out = []; for (var k in picked) out.push(Number(k)); return out }
    function selectionCount() { return selectedIndices().length }
    function clearSelection() { picked = {}; selectionVersion += 1 }
    function thumbFor(index) { return "" }
    function showRow(view, context) {}
    function renameEditor() { return null }
    function cancelSlowClick() {}
    function commitRename(newName) {}
    function act(action) { Focus.act(action, this) }
    function open(path) {}
    function openFile(path) {}
    function focusRequested() {}
    function refresh() {
        shown = Filter.shown(rows, held, filterQuery)
        shownTotal = shown === null ? total : shown.length
    }
}
