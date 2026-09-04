import QtQuick
import "archive.js" as ArchiveSuite
import "columns.js" as ColumnsSuite
import "contrast.js" as ContrastSuite
import "dirsizes.js" as DirSizesSuite
import "drag.js" as DragSuite
import "dst.js" as DstSuite
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
import "scale.js" as ScaleSuite
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

        // Naming a suite after a "--" separator runs that one alone, which is how js.sh
        // gives the DST suite its own timezone; the separator itself is what stops qml6
        // treating the name as a second file to load. A name matching nothing runs
        // nothing, so it fails rather than printing a green zero-check tally.
        var suites = [
            ["archive", ArchiveSuite], ["columns", ColumnsSuite], ["contrast", ContrastSuite],
            ["dirsizes", DirSizesSuite], ["drag", DragSuite], ["dst", DstSuite],
            ["errors", ErrorsSuite], ["facts", FactsSuite], ["filter", FilterSuite],
            ["focus", FocusSuite], ["format", FormatSuite], ["icons", IconsSuite],
            ["keymap", KeymapSuite], ["match", MatchSuite], ["menu", MenuSuite],
            ["mounts", MountsSuite], ["nav", NavSuite], ["ops", OpsSuite],
            ["palette", PaletteSuite], ["pathbar", PathBarSuite], ["places", PlacesSuite],
            ["protocols", ProtocolsSuite], ["scale", ScaleSuite], ["search", SearchSuite],
            ["selection", SelectionSuite], ["sort", SortSuite], ["taildrop", TaildropSuite],
            ["trash", TrashSuite], ["tap", TapSuite], ["tabs", TabsSuite],
            ["thumbs", ThumbsSuite]
        ]
        var argv = Qt.application.arguments
        var only = ""
        for (var a = 0; a < argv.length; a++) {
            if (argv[a].indexOf(".qml") >= 0) {
                for (var b = a + 1; b < argv.length; b++) {
                    if (argv[b] !== "--" && argv[b] !== "") {
                        only = argv[b]
                        break
                    }
                }
                break
            }
        }
        var matched = 0
        for (var s = 0; s < suites.length; s++) {
            // The unnamed battery excludes dst, whose pins are New York's wall clock;
            // it runs alone when named, under the timezone js.sh hands it.
            if ((only === "" && suites[s][0] !== "dst") || suites[s][0] === only) {
                matched += 1
                suites[s][1].run(check)
            }
        }
        if (only !== "" && matched === 0) {
            console.log("no suite named " + only)
            Qt.exit(1)
            return
        }

        for (var i = 0; i < failures.length; i++) {
            console.log("FAIL " + failures[i])
        }
        console.log(checked + " checks, " + failures.length + " failed")
        Qt.exit(failures.length === 0 ? 0 : 1)
    }
}
