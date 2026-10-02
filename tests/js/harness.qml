import QtQuick
import "archive.js" as ArchiveSuite
import "background.js" as BackgroundSuite
import "buttons.js" as ButtonsSuite
import "clipshare.js" as ClipShareSuite
import "collide.js" as CollideSuite
import "clipmarks.js" as ClipMarksSuite
import "columns.js" as ColumnsSuite
import "contrast.js" as ContrastSuite
import "crumbs.js" as CrumbsSuite
import "ddtarget.js" as DdtargetSuite
import "dirsizes.js" as DirSizesSuite
import "drag.js" as DragSuite
import "dst.js" as DstSuite
import "errors.js" as ErrorsSuite
import "facts.js" as FactsSuite
import "filter.js" as FilterSuite
import "filter-cursor.js" as FilterCursorSuite
import "foldersorts.js" as FolderSortsSuite
import "focus.js" as FocusSuite
import "focus-forward.js" as FocusForwardSuite
import "focus-grid.js" as FocusGridSuite
import "gridgeometry.js" as GridGeometrySuite
import "density.js" as DensitySuite
import "focus-lines.js" as FocusLinesSuite
import "focus-wrap.js" as FocusWrapSuite
import "format.js" as FormatSuite
import "icons.js" as IconsSuite
import "jump.js" as JumpSuite
import "jump-heldout.js" as JumpHeldoutSuite
import "keymap.js" as KeymapSuite
import "localsend.js" as LocalSendSuite
import "listbudget.js" as ListBudgetSuite
import "marks.js" as MarksSuite
import "match.js" as MatchSuite
import "menu.js" as MenuSuite
import "openwith.js" as OpenWithSuite
import "devices.js" as DevicesSuite
import "mounts.js" as MountsSuite
import "names.js" as NamesSuite
import "nav.js" as NavSuite
import "network.js" as NetworkSuite
import "ops.js" as OpsSuite
import "opstrash.js" as OpsTrashSuite
import "palette.js" as PaletteSuite
import "pathbar.js" as PathBarSuite
import "phones.js" as PhonesSuite
import "picker.js" as PickerSuite
import "previewkeys.js" as PreviewKeysSuite
import "places.js" as PlacesSuite
import "rail.js" as RailSuite
import "placemenu.js" as PlaceMenuSuite
import "protocols.js" as ProtocolsSuite
import "railkeys.js" as RailKeysSuite
import "recent.js" as RecentSuite
import "recentdates.js" as RecentDatesSuite
import "reclick.js" as ReclickSuite
import "renderer.js" as RendererSuite
import "rowcells.js" as RowCellsSuite
import "scroll.js" as ScrollSuite
import "search.js" as SearchSuite
import "selection.js" as SelectionSuite
import "scripts.js" as ScriptsSuite
import "settings.js" as SettingsSuite
import "settingsmenus.js" as SettingsMenusSuite
import "settingsshelf.js" as SettingsShelfSuite
import "settingsabout.js" as SettingsAboutSuite
import "themes.js" as ThemesSuite
import "transfer.js" as TransferSuite
import "sort.js" as SortSuite
import "swap.js" as SwapSuite
import "previewswap.js" as PreviewSwapSuite
import "startup.js" as StartupSuite
import "status.js" as StatusSuite
import "statusfix.js" as StatusFixSuite
import "permissions.js" as PermissionsSuite
import "trashdates.js" as TrashDatesSuite
import "taildrop.js" as TaildropSuite
import "textsize.js" as TextSizeSuite
import "trash.js" as TrashSuite
import "tap.js" as TapSuite
import "marquee.js" as MarqueeSuite
import "tabs.js" as TabsSuite
import "tabs-switch.js" as TabsSwitchSuite
import "shelfmodel.js" as ShelfModelSuite
import "thumbs.js" as ThumbsSuite
import "extthumbs.js" as ExtThumbsSuite
import "uistate.js" as UiStateSuite
import "update.js" as UpdateSuite
import "watch.js" as WatchSuite
import "gvfsbridge.js" as GvfsBridgeSuite

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

        var suites = [
            ["archive", ArchiveSuite], ["background", BackgroundSuite], ["buttons", ButtonsSuite], ["clipshare", ClipShareSuite], ["collide", CollideSuite], ["clipmarks", ClipMarksSuite], ["columns", ColumnsSuite], ["contrast", ContrastSuite],
            ["dirsizes", DirSizesSuite], ["drag", DragSuite], ["dst", DstSuite], ["ddtarget", DdtargetSuite],
            ["edmonton", DstSuite],
            ["errors", ErrorsSuite], ["facts", FactsSuite], ["filter", FilterSuite], ["filter-cursor", FilterCursorSuite],
            ["focus", FocusSuite], ["focus-grid", FocusGridSuite], ["focus-forward", FocusForwardSuite],
            ["foldersorts", FolderSortsSuite],
            ["gridgeometry", GridGeometrySuite], ["density", DensitySuite],
            ["focus-lines", FocusLinesSuite], ["focus-wrap", FocusWrapSuite],
            ["format", FormatSuite], ["icons", IconsSuite],
            ["jump", JumpSuite], ["jump-heldout", JumpHeldoutSuite], ["keymap", KeymapSuite], ["localsend", LocalSendSuite], ["listbudget", ListBudgetSuite], ["match", MatchSuite], ["menu", MenuSuite],
            ["marks", MarksSuite],             ["mounts", MountsSuite], ["names", NamesSuite], ["nav", NavSuite], ["crumbs", CrumbsSuite], ["devices", DevicesSuite], ["network", NetworkSuite],
            ["openwith", OpenWithSuite], ["ops", OpsSuite], ["opstrash", OpsTrashSuite],
            ["palette", PaletteSuite], ["pathbar", PathBarSuite], ["places", PlacesSuite], ["placemenu", PlaceMenuSuite], ["rail", RailSuite], ["scripts", ScriptsSuite],
            ["phones", PhonesSuite],
            ["picker", PickerSuite],
            ["previewkeys", PreviewKeysSuite], ["previewswap", PreviewSwapSuite],
            ["protocols", ProtocolsSuite], ["railkeys", RailKeysSuite],
            ["recent", RecentSuite],
            ["recentdates", RecentDatesSuite],
            ["reclick", ReclickSuite],
            ["renderer", RendererSuite], ["rowcells", RowCellsSuite],
            ["scroll", ScrollSuite], ["search", SearchSuite],
            ["selection", SelectionSuite], ["settings", SettingsSuite], ["settingsmenus", SettingsMenusSuite], ["settingsshelf", SettingsShelfSuite], ["settingsabout", SettingsAboutSuite],
            ["sort", SortSuite], ["swap", SwapSuite], ["startup", StartupSuite], ["trashdates", TrashDatesSuite], ["permissions", PermissionsSuite], ["status", StatusSuite], ["statusfix", StatusFixSuite], ["taildrop", TaildropSuite], ["textsize", TextSizeSuite],
            ["trash", TrashSuite], ["tap", TapSuite], ["marquee", MarqueeSuite], ["tabs", TabsSuite], ["tabs-switch", TabsSwitchSuite], ["shelfmodel", ShelfModelSuite],
            ["themes", ThemesSuite], ["thumbs", ThumbsSuite], ["extthumbs", ExtThumbsSuite], ["transfer", TransferSuite], ["uistate", UiStateSuite], ["update", UpdateSuite],
            ["watch", WatchSuite], ["gvfsbridge", GvfsBridgeSuite]
        ]
        var argv = Qt.application.arguments
        var only = ""
        var hasOnly = false
        for (var a = 0; a < argv.length; a++) {
            if (argv[a].indexOf(".qml") >= 0) {
                var suiteIndex = a + 1
                if (argv[suiteIndex] === "--") {
                    suiteIndex += 1
                }
                if (suiteIndex < argv.length) {
                    only = argv[suiteIndex]
                    hasOnly = true
                }
                break
            }
        }
        var matched = 0
        for (var s = 0; s < suites.length; s++) {
            // Unnamed runs omit timezone-specific fixtures; tests/js.sh invokes them by name.
            if ((!hasOnly && suites[s][0] !== "dst" && suites[s][0] !== "edmonton")
                    || (hasOnly && suites[s][0] === only)) {
                matched += 1
                suites[s][1].run(check, suites[s][0])
            }
        }
        if (hasOnly && matched === 0) {
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
