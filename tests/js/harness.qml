import QtQuick
import "archive.js" as ArchiveSuite
import "archives.js" as ArchivesSuite
import "archivecards.js" as ArchiveCardsSuite
import "background.js" as BackgroundSuite
import "buttons.js" as ButtonsSuite
import "cardink.js" as CardInkSuite
import "bulkrename.js" as BulkRenameSuite
import "bulksheet.js" as BulkSheetSuite
import "collide.js" as CollideSuite
import "collidemerge.js" as CollideMergeSuite
import "clipmarks.js" as ClipMarksSuite
import "clipboard.js" as ClipboardSuite
import "columns.js" as ColumnsSuite
import "columnskeep.js" as ColumnsKeepSuite
import "columnsback.js" as ColumnsBackSuite
import "watchgate.js" as WatchGateSuite
import "codelines.js" as CodeLinesSuite
import "csv.js" as CsvSuite
import "contrast.js" as ContrastSuite
import "glasstag.js" as GlassTagSuite
import "copyas.js" as CopyAsSuite
import "invert.js" as InvertSuite
import "crumbs.js" as CrumbsSuite
import "ddtarget.js" as DdtargetSuite
import "dirsizes.js" as DirSizesSuite
import "drag.js" as DragSuite
import "dragout.js" as DragOutSuite
import "dropkeys.js" as DropKeysSuite
import "drops.js" as DropsSuite
import "dropsflow.js" as DropsFlowSuite
import "dst.js" as DstSuite
import "errors.js" as ErrorsSuite
import "facts.js" as FactsSuite
import "fontfacts.js" as FontFactsSuite
import "figureservice.js" as FigureServiceSuite
import "filter.js" as FilterSuite
import "filter-cursor.js" as FilterCursorSuite
import "foldersorts.js" as FolderSortsSuite
import "flash.js" as FlashSuite
import "flashpane.js" as FlashPaneSuite
import "framegap.js" as FramegapSuite
import "folderviews.js" as FolderViewsSuite
import "focus.js" as FocusSuite
import "focus-forward.js" as FocusForwardSuite
import "focus-grid.js" as FocusGridSuite
import "gridgeometry.js" as GridGeometrySuite
import "density.js" as DensitySuite
import "focus-lines.js" as FocusLinesSuite
import "focus-wrap.js" as FocusWrapSuite
import "fill.js" as FillSuite
import "format.js" as FormatSuite
import "icons.js" as IconsSuite
import "jump.js" as JumpSuite
import "jump-heldout.js" as JumpHeldoutSuite
import "keymap.js" as KeymapSuite
import "langmarks.js" as LangmarksSuite
import "keyreach.js" as KeyReachSuite
import "localsend.js" as LocalSendSuite
import "markdown.js" as MarkdownSuite
import "mdprepared.js" as MdPreparedSuite
import "mdstructure.js" as MdStructureSuite
import "mdlineends.js" as MdLineEndsSuite
import "mdliteral.js" as MdLiteralSuite
import "mdspecfix.js" as MdSpecFixSuite
import "mddraw.js" as MdDrawSuite
import "mdnest.js" as MdNestSuite
import "mdhtml.js" as MdHtmlSuite
import "mdhtmlgh.js" as MdHtmlGhSuite
import "mdhead.js" as MdHeadSuite
import "mdhtmlrow.js" as MdHtmlRowSuite
import "mdgates.js" as MdGatesSuite
import "mdr7.js" as MdR7Suite
import "mdtable.js" as MdTableSuite
import "mdround4.js" as MdRound4Suite
import "mdprogress.js" as MdProgressSuite
import "mdslice.js" as MdSliceSuite
import "markdownpictures.js" as MarkdownPicturesSuite
import "listbudget.js" as ListBudgetSuite
import "marks.js" as MarksSuite
import "match.js" as MatchSuite
import "motion.js" as MotionSuite
import "menu.js" as MenuSuite
import "xwnewfile.js" as XwNewFileSuite
import "properties.js" as PropertiesSuite
import "propertyfacts.js" as PropertyFactsSuite
import "openwith.js" as OpenWithSuite
import "devices.js" as DevicesSuite
import "loopdevices.js" as LoopDevicesSuite
import "mounts.js" as MountsSuite
import "names.js" as NamesSuite
import "nav.js" as NavSuite
import "navnewest.js" as NavNewestSuite
import "cursorreturn.js" as CursorReturnSuite
import "navmouse.js" as NavMouseSuite
import "network.js" as NetworkSuite
import "ops.js" as OpsSuite
import "opstrash.js" as OpsTrashSuite
import "palette.js" as PaletteSuite
import "pathbar.js" as PathBarSuite
import "phones.js" as PhonesSuite
import "picker.js" as PickerSuite
import "pickerpath.js" as PickerPathSuite
import "pindrop.js" as PinDropSuite
import "previewkeys.js" as PreviewKeysSuite
import "compare.js" as CompareSuite
import "sequences.js" as SequencesSuite
import "previewask.js" as PreviewAskSuite
import "places.js" as PlacesSuite
import "rail.js" as RailSuite
import "fsslot.js" as FsSlotSuite
import "placemenu.js" as PlaceMenuSuite
import "protocols.js" as ProtocolsSuite
import "railentries.js" as RailEntriesSuite
import "railkeys.js" as RailKeysSuite
import "recent.js" as RecentSuite
import "recentmode.js" as RecentModeSuite
import "recentdates.js" as RecentDatesSuite
import "reclick.js" as ReclickSuite
import "renderer.js" as RendererSuite
import "rowcells.js" as RowCellsSuite
import "runfile.js" as RunFileSuite
import "scroll.js" as ScrollSuite
import "scrollanchor.js" as ScrollAnchorSuite
import "scrolloff.js" as ScrollOffSuite
import "menuscroll.js" as MenuScrollSuite
import "search.js" as SearchSuite
import "searchnoblank.js" as SearchNoBlankSuite
import "selection.js" as SelectionSuite
import "reload.js" as ReloadSuite
import "slowclick.js" as SlowClickSuite
import "slowop.js" as SlowOpSuite
import "slowsource.js" as SlowSourceSuite
import "shiftranges.js" as ShiftRangesSuite
import "sheetquery.js" as SheetQuerySuite
import "sheetcandidates.js" as SheetCandidatesSuite
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
import "quicklookrect.js" as QuickLookRectSuite
import "startup.js" as StartupSuite
import "status.js" as StatusSuite
import "statusfix.js" as StatusFixSuite
import "permissions.js" as PermissionsSuite
import "trashdates.js" as TrashDatesSuite
import "taildrop.js" as TaildropSuite
import "tags.js" as TagsSuite
import "tagskeys.js" as TagsKeysSuite
import "textsize.js" as TextSizeSuite
import "trash.js" as TrashSuite
import "trashdrop.js" as TrashDropSuite
import "tap.js" as TapSuite
import "marquee.js" as MarqueeSuite
import "tabs.js" as TabsSuite
import "tabcatcher.js" as TabCatcherSuite
import "tabs-switch.js" as TabsSwitchSuite
import "tabmove.js" as TabMoveSuite
import "tabrestore.js" as TabRestoreSuite
import "shelfmodel.js" as ShelfModelSuite
import "thumbs.js" as ThumbsSuite
import "extthumbs.js" as ExtThumbsSuite
import "uistate.js" as UiStateSuite
import "unlock.js" as UnlockSuite
import "update.js" as UpdateSuite
import "watch.js" as WatchSuite
import "xwwatch.js" as XwWatchSuite
import "xwanchor-races.js" as XwAnchorRacesSuite
import "renameview.js" as RenameViewSuite
import "xwrl4.js" as Xwrl4Suite
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
            ["clipboard", ClipboardSuite],
            ["archive", ArchiveSuite], ["archives", ArchivesSuite], ["archivecards", ArchiveCardsSuite], ["background", BackgroundSuite], ["buttons", ButtonsSuite], ["cardink", CardInkSuite], ["bulkrename", BulkRenameSuite], ["bulksheet", BulkSheetSuite], ["collide", CollideSuite], ["collidemerge", CollideMergeSuite], ["clipmarks", ClipMarksSuite], ["columns", ColumnsSuite], ["columnskeep", ColumnsKeepSuite], ["columnsback", ColumnsBackSuite], ["watchgate", WatchGateSuite], ["contrast", ContrastSuite], ["glasstag", GlassTagSuite], ["codelines", CodeLinesSuite], ["csv", CsvSuite],
            ["dirsizes", DirSizesSuite], ["drag", DragSuite], ["dragout", DragOutSuite], ["dropkeys", DropKeysSuite], ["drops", DropsSuite], ["dropsflow", DropsFlowSuite], ["dst", DstSuite], ["ddtarget", DdtargetSuite],
            ["copyas", CopyAsSuite], ["invert", InvertSuite],
            ["edmonton", DstSuite],
            ["errors", ErrorsSuite], ["facts", FactsSuite], ["fontfacts", FontFactsSuite], ["figureservice", FigureServiceSuite], ["filter", FilterSuite], ["filter-cursor", FilterCursorSuite],
            ["focus", FocusSuite], ["focus-grid", FocusGridSuite], ["focus-forward", FocusForwardSuite],
            ["flash", FlashSuite], ["flashpane", FlashPaneSuite], ["framegap", FramegapSuite],
            ["foldersorts", FolderSortsSuite],
            ["folderviews", FolderViewsSuite],
            ["gridgeometry", GridGeometrySuite], ["density", DensitySuite],
            ["focus-lines", FocusLinesSuite], ["focus-wrap", FocusWrapSuite],
            ["fill", FillSuite], ["format", FormatSuite], ["icons", IconsSuite],
            ["jump", JumpSuite], ["jump-heldout", JumpHeldoutSuite], ["keymap", KeymapSuite], ["langmarks", LangmarksSuite], ["keyreach", KeyReachSuite], ["localsend", LocalSendSuite], ["listbudget", ListBudgetSuite], ["match", MatchSuite], ["menu", MenuSuite], ["properties", PropertiesSuite], ["propertyfacts", PropertyFactsSuite], ["xwnewfile", XwNewFileSuite],
            ["marks", MarksSuite], ["markdown", MarkdownSuite], ["mdstructure", MdStructureSuite], ["mdlineends", MdLineEndsSuite], ["mdliteral", MdLiteralSuite], ["mdspecfix", MdSpecFixSuite], ["mddraw", MdDrawSuite], ["mdnest", MdNestSuite], ["mdhtml", MdHtmlSuite], ["mdhtmlgh", MdHtmlGhSuite], ["mdhead", MdHeadSuite], ["mdhtmlrow", MdHtmlRowSuite], ["mdr7", MdR7Suite], ["mdround4", MdRound4Suite], ["mdprogress", MdProgressSuite], ["mdslice", MdSliceSuite], ["markdownpictures", MarkdownPicturesSuite], ["mounts", MountsSuite], ["motion", MotionSuite], ["names", NamesSuite], ["nav", NavSuite], ["navnewest", NavNewestSuite], ["cursorreturn", CursorReturnSuite], ["crumbs", CrumbsSuite], ["devices", DevicesSuite], ["loopdevices", LoopDevicesSuite], ["network", NetworkSuite], ["navmouse", NavMouseSuite], ["mdtable", MdTableSuite],
            ["openwith", OpenWithSuite], ["ops", OpsSuite], ["opstrash", OpsTrashSuite],
            ["palette", PaletteSuite], ["pathbar", PathBarSuite], ["places", PlacesSuite], ["placemenu", PlaceMenuSuite], ["pindrop", PinDropSuite], ["rail", RailSuite], ["fsslot", FsSlotSuite], ["scripts", ScriptsSuite],
            ["phones", PhonesSuite],
            ["picker", PickerSuite],
            ["pickerpath", PickerPathSuite],
            ["previewkeys", PreviewKeysSuite], ["compare", CompareSuite], ["sequences", SequencesSuite], ["previewask", PreviewAskSuite], ["mdprepared", MdPreparedSuite], ["previewswap", PreviewSwapSuite], ["quicklookrect", QuickLookRectSuite],
            ["protocols", ProtocolsSuite], ["railentries", RailEntriesSuite], ["railkeys", RailKeysSuite],
            ["recent", RecentSuite],
            ["recentmode", RecentModeSuite],
            ["recentdates", RecentDatesSuite],
            ["reclick", ReclickSuite],
            ["renderer", RendererSuite], ["rowcells", RowCellsSuite], ["runfile", RunFileSuite],
            ["scroll", ScrollSuite], ["scrollanchor", ScrollAnchorSuite], ["scrolloff", ScrollOffSuite], ["menuscroll", MenuScrollSuite], ["search", SearchSuite], ["searchnoblank", SearchNoBlankSuite],
            ["selection", SelectionSuite], ["shiftranges", ShiftRangesSuite], ["sheetquery", SheetQuerySuite], ["sheetcandidates", SheetCandidatesSuite], ["reload", ReloadSuite], ["slowclick", SlowClickSuite], ["slowop", SlowOpSuite], ["slowsource", SlowSourceSuite], ["settings", SettingsSuite], ["settingsmenus", SettingsMenusSuite], ["settingsshelf", SettingsShelfSuite], ["settingsabout", SettingsAboutSuite],
            ["sort", SortSuite], ["swap", SwapSuite], ["startup", StartupSuite], ["trashdates", TrashDatesSuite], ["permissions", PermissionsSuite], ["status", StatusSuite], ["statusfix", StatusFixSuite], ["taildrop", TaildropSuite], ["tags", TagsSuite], ["tagskeys", TagsKeysSuite], ["textsize", TextSizeSuite],
            ["trash", TrashSuite], ["trashdrop", TrashDropSuite], ["tap", TapSuite], ["marquee", MarqueeSuite], ["tabs", TabsSuite], ["tabcatcher", TabCatcherSuite], ["tabs-switch", TabsSwitchSuite], ["tabmove", TabMoveSuite], ["tabrestore", TabRestoreSuite], ["shelfmodel", ShelfModelSuite],
            ["mdgates", MdGatesSuite],
            ["themes", ThemesSuite], ["thumbs", ThumbsSuite], ["extthumbs", ExtThumbsSuite], ["transfer", TransferSuite], ["uistate", UiStateSuite], ["unlock", UnlockSuite], ["update", UpdateSuite],
            ["watch", WatchSuite], ["xwwatch", XwWatchSuite], ["xwanchor-races", XwAnchorRacesSuite], ["renameview", RenameViewSuite], ["xwrl4", Xwrl4Suite], ["gvfsbridge", GvfsBridgeSuite]
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
                if (suiteIndex < argv.length && argv[suiteIndex].indexOf("--qt-version=") !== 0) {
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
        console.log((hasOnly ? only : "js") + ": " + checked + " checks, " + failures.length + " failed")
        Qt.exit(failures.length === 0 ? 0 : 1)
    }
}
