pragma Singleton
import QtQuick

// The tokens the archive cards read from ui/Theme.qml, fixed, because only their logic is under test.
QtObject {
    readonly property var color: ({ background: "black", surface: "black", muted: "gray", error: "red", foreground: "white" })
    readonly property var spacing: ({ rowPaddingX: 12, hairline: 1, gap: 8 })
    readonly property var font: ({ family: "sans", caption: 10, body: 12 })
    readonly property real dialogWidthRatio: 1
    readonly property var card: ({ padding: 18, gap: 14, labelGap: 4, inset: 8, title: 14, narrow: 370, narrowContent: 332, form: 448, wide: 480, sheet: 568 })
    readonly property int focusClearance: 2
    readonly property int rowHeight: 24
    function space(value) { return value }
    function cardSpan(wanted, room) { return Math.min(wanted, room) }
    function cardOrigin(room, span) { return (room - span) / 2 }
}
