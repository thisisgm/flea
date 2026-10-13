import QtQuick
import "js/ColumnFit.js" as ColumnFit

// Fit-only metrics, so Header carries no fit helper until a fit runs.
Item {
    TextMetrics {
        id: fitText
        renderType: Text.NativeRendering
        font.family: Theme.font.family
        font.hintingPreference: Font.PreferFullHinting
        font.pixelSize: Theme.font.caption
    }

    // One held row's measured width, in the cells' own caption face.
    function cellWidth(key, row, kindNames, dirSize) {
        fitText.text = ColumnFit.cellText(key, row, kindNames, dirSize)
        return Math.ceil(fitText.advanceWidth)
    }

    // The held index behind one held row, for the dirsize the row would draw.
    function dirSizeFor(state, held, at) {
        return ColumnFit.dirSizeFor(state, held, at)
    }
}
