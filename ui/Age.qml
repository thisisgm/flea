pragma Singleton

import Quickshell
import QtQuick
import qs.Commons
import "js/Format.js" as Format

// The age column's own home: the width its text is cut for, the recency ramp that tints it, and
// the one low-rate tick that repaints it as time passes. Rows repaint on a listing, a scroll or
// a selection and never on the clock, so a column whose value is "how long ago" needs one timer
// the whole panel shares: every binding that reads nowMs re-evaluates when the tick moves it,
// which is the repaint. Thirty seconds keeps the minute band at worst half a minute stale.
Singleton {
    id: root

    property real nowMs: Date.now()

    property Timer tick: Timer {
        interval: 30000
        repeat: true
        running: true
        onTriggered: root.nowMs = Date.now()
    }

    // "1 h 19'" is the widest of Format.age's forms, the same character-count measure
    // ui/Theme.qml's dateChars is, so the column is cut for its own widest text.
    readonly property int ageChars: 7
    // One glyph's advance in the monospace face, the same measure ui/Theme.qml sizes every fixed
    // column from, held here so that file's own budget did not have to carry the fifth column.
    TextMetrics {
        id: glyph
        font.family: Style.font.family
        font.pixelSize: Theme.font.bodySmall
        text: "0"
    }
    readonly property int width: Math.round(root.ageChars * glyph.advanceWidth)
    // DualPane's own slot: the window's own width scaled the way ui/Theme.qml's dualColumn
    // scales size and date, both anchored at bodySmall 13.
    readonly property real dualWidth: 55 * Theme.font.bodySmall / 13

    // The recency ramp, in absolute terms: red for the first minute, orange for the hour,
    // yellow for the day, green for the week, cyan for the month, blue for everything older.
    // Deliberately not read from the theme -- a palette ships pastels and a pastel ramp reads
    // as one grey band after another, which is what "tinted by recency" must never do. The
    // text carries the fact, so colour never carries it alone.
    readonly property var ramp: ["#f87171", "#fb923c", "#facc15", "#4ade80", "#22d3ee", "#60a5fa"]

    // The tint for one row: its band's rung for a real mtime, the muted role for a row that
    // has none, and foreground for a lifted row, which gives the tint up for contrast the way
    // it gives up every semantic colour.
    function tint(mtime, lifted) {
        if (lifted)
            return Theme.color.foreground
        if (mtime === null || mtime === undefined)
            return Theme.color.muted
        return root.ramp[Format.ageBand(mtime, root.nowMs)]
    }
}
