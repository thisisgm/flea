import QtQuick
import "archive.js" as ArchiveSuite
import "columns.js" as ColumnsSuite
import "contrast.js" as ContrastSuite
import "dirsizes.js" as DirSizesSuite
import "drag.js" as DragSuite
import "errors.js" as ErrorsSuite
import "facts.js" as FactsSuite
import "filter.js" as FilterSuite
import "focus.js" as FocusSuite
import "format.js" as FormatSuite
import "icons.js" as IconsSuite
import "keymap.js" as KeymapSuite
import "match.js" as MatchSuite
import "menu.js" as MenuSuite
import "mounts.js" as MountsSuite
import "nav.js" as NavSuite
import "ops.js" as OpsSuite
import "palette.js" as PaletteSuite
import "pathbar.js" as PathBarSuite
import "places.js" as PlacesSuite
import "protocols.js" as ProtocolsSuite
import "search.js" as SearchSuite
import "selection.js" as SelectionSuite
import "sort.js" as SortSuite
import "taildrop.js" as TaildropSuite
import "trash.js" as TrashSuite
import "tap.js" as TapSuite
import "tabs.js" as TabsSuite
import "thumbs.js" as ThumbsSuite

Item {
    Component.onCompleted: {
        var failures = []
        var checked = 0

        function check(label, actual, expected) {
            checked += 1
            if (actual !== expected) {
                failures.push(label + ": got " + JSON.stringify(actual)
                              + ", expected " + JSON.stringify(expected))
            }
        }

        ArchiveSuite.run(check)
        ColumnsSuite.run(check)
        ContrastSuite.run(check)
        DirSizesSuite.run(check)
        DragSuite.run(check)
        ErrorsSuite.run(check)
        FactsSuite.run(check)
        FilterSuite.run(check)
        FocusSuite.run(check)
        FormatSuite.run(check)
        IconsSuite.run(check)
        KeymapSuite.run(check)
        MatchSuite.run(check)
        MenuSuite.run(check)
        MountsSuite.run(check)
        NavSuite.run(check)
        OpsSuite.run(check)
        PaletteSuite.run(check)
        PathBarSuite.run(check)
        PlacesSuite.run(check)
        ProtocolsSuite.run(check)
        SearchSuite.run(check)
        SelectionSuite.run(check)
        SortSuite.run(check)
        TaildropSuite.run(check)
        TrashSuite.run(check)
        TapSuite.run(check)
        TabsSuite.run(check)
        ThumbsSuite.run(check)

        for (var i = 0; i < failures.length; i++) {
            console.log("FAIL " + failures[i])
        }
        console.log(checked + " checks, " + failures.length + " failed")
        Qt.exit(failures.length === 0 ? 0 : 1)
    }
}
