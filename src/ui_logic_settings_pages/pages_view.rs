// The View and Preview panes, from ui/js/Settings.js viewRows and previewRows, in the 0.4.0 boards' words.
use super::super::rows::{choice, Role, Row};
use super::super::Text;
use super::{at, column_label, columns, is_true, not_false, number, word, State};
use crate::keys::presentation::key_for;

pub fn view_rows(state: &State<'_>) -> Vec<Row> {
    let data = state.data;
    let check = |id: &str, label: &'static str, glyph: &'static str, on: bool| {
        Row::check(id, label, glyph, on)
    };
    let shown: Vec<Text> = columns(data).iter().map(|key| column_label(key)).collect();
    let limit = number(data, &["columnsLimit"], 3.).to_string();
    let mut out = vec![
        Row::group("settings.group.view"),
        choice(
            "view",
            "settings.row.view",
            "",
            &["list", "columns", "grid", "dual"],
            &[
                "settings.choice.view_list",
                "settings.choice.view_columns",
                "settings.choice.view_grid",
                "settings.choice.view_dual",
            ],
            word(data, &["view"], "list"),
        ),
        check(
            "rememberViewPerFolder",
            "settings.row.remember_view",
            "folder",
            is_true(data, &["rememberViewPerFolder"]),
        ),
        choice(
            "density",
            "settings.row.density",
            "list",
            &["tight", "compact", "normal", "comfortable"],
            &[
                "settings.choice.tight",
                "settings.choice.compact",
                "settings.choice.normal",
                "settings.choice.comfortable",
            ],
            word(data, &["density"], "compact"),
        ),
        Row::hint(Text::Key("settings.hint.compact_default")),
        Row::action(
            "columns",
            "settings.row.columns",
            "columns",
            Text::List(shown),
        ),
        choice(
            "addressBar",
            "settings.row.address_bar",
            "",
            &["path", "breadcrumb"],
            &["settings.choice.path", "settings.choice.breadcrumb"],
            word(data, &["addressBar"], "breadcrumb"),
        ),
        // The window's bar across the whole window above the places, or only above the list beside them as Trash draws
        // it; mixed keeps each as it ships.
        choice(
            "windowBar",
            "settings.row.window_bar",
            "",
            &["mixed", "list", "window"],
            &[
                "settings.choice.bar_mixed",
                "settings.choice.bar_list",
                "settings.choice.bar_window",
            ],
            word(data, &["windowBar"], "mixed"),
        ),
        choice(
            "columnsLimit",
            "settings.row.columns_limit",
            "columns",
            &["2", "3", "4", "5"],
            &[
                "settings.choice.n2",
                "settings.choice.n3",
                "settings.choice.n4",
                "settings.choice.n5",
            ],
            &limit,
        ),
        // 0.4.0: Git status sits before Highlight today's dates and ships off.
        check(
            "showGitStatus",
            "settings.row.show_git_status",
            "folder-git-2",
            is_true(data, &["showGitStatus"]),
        ),
        check(
            "highlightToday",
            "settings.row.highlight_today",
            "history",
            is_true(data, &["highlightToday"]),
        ),
        // 0401-settings-hover: the pointer's wash on rows, tiles and the rail, off as decision hover-default ships it.
        check(
            "highlightHover",
            "settings.row.highlight_hover",
            "pointer",
            is_true(data, &["highlightHover"]),
        ),
        // 0401-settings-view-c: the window's resting look, tab marks then Omi before the messages it sits over.
        check(
            "tabsAsIcons",
            "settings.row.tabs_as_icons",
            "app-window",
            is_true(data, &["tabsAsIcons"]),
        ),
        check(
            "emptyFolderOmi",
            "settings.row.empty_omi",
            "bot",
            not_false(data, &["emptyFolderOmi"]),
        ),
        check(
            "emptyFolderRotate",
            "settings.row.empty_rotate",
            "bot",
            not_false(data, &["emptyFolderRotate"]),
        ),
        check(
            "openFoldersWhileDragging",
            "settings.row.open_while_dragging",
            "",
            is_true(data, &["openFoldersWhileDragging"]),
        ),
        Row::group("settings.group.sorting"),
        choice(
            "sort.key",
            "settings.row.sort_by",
            "sort",
            &["name", "size", "date", "kind"],
            &[
                "settings.column.name",
                "settings.column.size",
                "settings.column.date",
                "settings.column.kind",
            ],
            word(data, &["sort", "key"], "name"),
        ),
        check(
            "foldersFirst",
            "settings.row.folders_first",
            "folders-first",
            not_false(data, &["foldersFirst"]),
        ),
        Row {
            caption: Text::Key("settings.caption.group_by_kind"),
            ..check(
                "groupByKind",
                "settings.row.group_by_kind",
                "grid",
                is_true(data, &["groupByKind"]),
            )
        },
        check(
            "hidden",
            "settings.row.show_hidden",
            "eye",
            is_true(data, &["hidden"]),
        ),
        // Issue 70: a dependent greys in place while hidden files are off.
        Row {
            available: is_true(data, &["hidden"]),
            ..check(
                "hiddenLast",
                "settings.row.hidden_last",
                "eye-off",
                is_true(data, &["hiddenLast"]),
            )
        },
        check(
            "langMarks",
            "settings.row.lang_marks",
            "code",
            not_false(data, &["langMarks"]),
        ),
        check(
            "rememberSort",
            "settings.row.remember_sort",
            "sort",
            not_false(data, &["rememberSort"]),
        ),
        Row::group("settings.group.cursor"),
        Row {
            caption: Text::Key("settings.caption.wrap"),
            ..check(
                "wrapAtEnds",
                "settings.row.wrap",
                "arrow-up",
                is_true(data, &["wrapAtEnds"]),
            )
        },
        Row {
            caption: Text::Key("settings.caption.grid_wrap_rows"),
            ..check(
                "gridWrapRows",
                "settings.row.grid_wrap_rows",
                "move-horizontal",
                is_true(data, &["gridWrapRows"]),
            )
        },
        Row {
            caption: Text::Key("settings.caption.escape_up"),
            ..check(
                "escapeUp",
                "settings.row.escape_up",
                "",
                is_true(data, &["escapeUp"]),
            )
        },
        Row::group("settings.group.opening"),
        choice(
            "startIn",
            "settings.row.start_in",
            "house",
            &["home", "last", "folder"],
            &[
                "settings.choice.home",
                "settings.choice.last",
                "settings.choice.chosen",
            ],
            word(data, &["startIn"], "home"),
        ),
        Row::action(
            "startFolder",
            "settings.row.start_folder",
            "folder",
            match word(data, &["startFolder"], "") {
                "" => Text::Key("settings.row.use_this_folder"),
                path => Text::data(path),
            },
        ),
        choice(
            "newTab",
            "settings.row.new_tab",
            "columns",
            &["current", "home", "start"],
            &[
                "settings.choice.current_folder",
                "settings.choice.home",
                "settings.choice.start_folder",
            ],
            word(data, &["newTab"], "current"),
        ),
        choice(
            "openMode",
            "settings.row.open_mode",
            "pointer",
            &["double", "single"],
            &[
                "settings.choice.double_click",
                "settings.choice.single_click",
            ],
            word(data, &["openMode"], "double"),
        ),
        Row {
            available: at(data, &["openMode"]).and_then(|v| v.as_str()) != Some("single"),
            ..check(
                "clickRename",
                "settings.row.click_rename",
                "",
                not_false(data, &["clickRename"]),
            )
        },
        match &state.save_error {
            Some(error) => Row {
                footer: true,
                role: Role::Error,
                ..Row::hint(error.clone())
            },
            None => Row {
                footer: true,
                ..Row::hint(Text::Key("settings.footer.saved"))
            },
        },
    ];
    // Tabs040 callout 2: the board's own sentence, under the control it explains and only there.
    if word(data, &["startIn"], "home") == "last" {
        let at = out
            .iter()
            .position(|row| row.id == "startIn")
            .map_or(out.len(), |i| i + 1);
        out.insert(at, Row::hint(Text::Key("settings.hint.last_folder_tabs")));
    }
    out
}

pub fn preview_rows(state: &State<'_>) -> Vec<Row> {
    let data = state.data;
    let p = |key: &'static str| ["preview", key];
    let column = not_false(data, &p("column"));
    let thumbs_on = word(data, &p("thumbnails"), "media") != "off";
    // Rule 7: with the column off neither load path reads, so Load greys rather than lying.
    let load = Row {
        available: column,
        ..choice(
            "preview.loadOn",
            "settings.row.load",
            "eye",
            &["automatic", "manual"],
            &["settings.choice.automatic", "settings.choice.manual"],
            word(data, &p("loadOn"), "automatic"),
        )
    };
    let mut out = vec![
        Row::group("settings.group.preview_column"),
        Row::check(
            "preview.column",
            "settings.row.preview_column",
            "columns",
            column,
        ),
        load,
    ];
    // A hint sits under the control it explains; none when the preset binds no key to name.
    let manual = at(data, &p("loadOn")).and_then(|v| v.as_str()) == Some("manual");
    out.extend(load_hint(manual, key_for(state.preset, "loadPreview")));
    let class = |id: &str, label: &'static str, on: bool| Row {
        available: thumbs_on,
        ..Row::check(id, label, "", on)
    };
    out.extend([
        Row::group("settings.group.thumbnails"),
        choice(
            "preview.thumbnails",
            "settings.row.thumbnails",
            "image",
            &["off", "images", "media"],
            &[
                "settings.choice.off",
                "settings.choice.images",
                "settings.choice.everything",
            ],
            word(data, &p("thumbnails"), "media"),
        ),
        choice(
            "preview.thumbSize",
            "settings.row.thumb_size",
            "maximize",
            &["small", "medium", "large", "xlarge", "huge", "largest"],
            &[
                "settings.choice.small",
                "settings.choice.medium",
                "settings.choice.large",
                "settings.choice.xlarge",
                "settings.choice.huge",
                "settings.choice.largest",
            ],
            word(data, &p("thumbSize"), "medium"),
        ),
        class(
            "preview.thumbNetwork",
            "settings.row.thumb_network",
            is_true(data, &p("thumbNetwork")),
        ),
        class(
            "preview.thumbPhone",
            "settings.row.thumb_phone",
            is_true(data, &p("thumbPhone")),
        ),
        class(
            "preview.thumbUsb",
            "settings.row.thumb_usb",
            not_false(data, &p("thumbUsb")),
        ),
        Row {
            available: thumbs_on,
            ..Row::hint(Text::Key("settings.hint.thumbs_kept"))
        },
        // Grid zoom never greys with thumbnails: the tiles still size from it.
        Row::check(
            "preview.ctrlZoom",
            "settings.row.ctrl_zoom",
            "move-horizontal",
            not_false(data, &p("ctrlZoom")),
        ),
    ]);
    out
}

// Load's sentence: what automatic follows, or the key that loads, left out while the preset binds none.
pub(super) fn load_hint(manual: bool, key: String) -> Option<Row> {
    if !manual {
        Some(Row::hint(Text::Key("settings.hint.automatic_load")))
    } else if key.is_empty() {
        None
    } else {
        Some(Row::hint(Text::args(
            "settings.hint.load_key",
            vec![Text::Keycap(key)],
        )))
    }
}

#[cfg(test)]
#[path = "pages_view_tests.rs"]
mod tests;
