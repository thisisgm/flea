use super::*;

#[test]
fn the_defaults_are_the_shipped_shape_key_for_key() {
    let d = defaults();
    let keys: Vec<&str> = d
        .as_object()
        .expect("object")
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "view",
            "pickerView",
            "density",
            "columns",
            "columnsLimit",
            "columnWidths",
            "addressBar",
            "windowBar",
            "sort",
            "rememberSort",
            "folderSorts",
            "rememberViewPerFolder",
            "folderViews",
            "dual",
            "foldersFirst",
            "groupByKind",
            "hidden",
            "langMarks",
            "hiddenLast",
            "showGitStatus",
            "highlightToday",
            "highlightHover",
            "tabsAsIcons",
            "emptyFolderOmi",
            "emptyFolderRotate",
            "wrapAtEnds",
            "gridWrapRows",
            "escapeUp",
            "openMode",
            "clickRename",
            "openFoldersWhileDragging",
            "keyHints",
            "startIn",
            "startFolder",
            "lastPath",
            "lastTabs",
            "newTab",
            "trashAutoEmpty",
            "trashSweptOn",
            "places",
            "shelf",
            "preview",
            "previewFacts",
            "keys",
            "display",
            "menu",
            "updates",
            "plugins",
            "menuVersion",
            "stateVersion"
        ]
    );
    assert_eq!(
        d.get(STATE_VERSION).and_then(Json::as_f64),
        Some(4.0),
        "a fresh document is already stamped"
    );
    assert_eq!(
        d.get(MENU_VERSION).and_then(Json::as_f64),
        Some(1.0),
        "and carries the menu stamp beside it"
    );
    assert_eq!(d.get("view").and_then(Json::as_str), Some("list"));
    assert_eq!(d.get("pickerView").and_then(Json::as_str), Some("list"));
    assert_eq!(d.get("density").and_then(Json::as_str), Some("compact"));
    assert_eq!(
        d.get("addressBar").and_then(Json::as_str),
        Some("breadcrumb")
    );
    assert_eq!(d.get("windowBar").and_then(Json::as_str), Some("mixed"));
    assert_eq!(d.get("keys").and_then(Json::as_str), Some("default"));
    assert_eq!(d.get("foldersFirst").and_then(Json::as_bool), Some(true));
    assert_eq!(d.get("groupByKind").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("hidden").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("gridWrapRows").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("hiddenLast").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("langMarks").and_then(Json::as_bool), Some(true));
    assert_eq!(d.get("showGitStatus").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("highlightToday").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("highlightHover").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("tabsAsIcons").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("emptyFolderOmi").and_then(Json::as_bool), Some(true));
    assert_eq!(
        d.get("emptyFolderRotate").and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(d.get("wrapAtEnds").and_then(Json::as_bool), Some(false));
    // Escape stays put and a double click opens as in 0.3.4; slow click renames, on by the defaults ledger.
    assert_eq!(d.get("escapeUp").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("openMode").and_then(Json::as_str), Some("double"));
    assert_eq!(d.get("clickRename").and_then(Json::as_bool), Some(true));
    assert_eq!(d.get("keyHints").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("startIn").and_then(Json::as_str), Some("home"));
    assert_eq!(d.get("startFolder").and_then(Json::as_str), Some(""));
    assert_eq!(d.get("lastPath").and_then(Json::as_str), Some(""));
    assert_eq!(
        d.get("lastTabs")
            .and_then(|t| t.get("paths"))
            .and_then(Json::as_array)
            .map(<[Json]>::len),
        Some(0)
    );
    assert_eq!(
        d.get("lastTabs")
            .and_then(|t| t.get("index"))
            .and_then(Json::as_f64),
        Some(0.0)
    );
    assert_eq!(d.get("newTab").and_then(Json::as_str), Some("current"));
    assert_eq!(d.get("trashAutoEmpty").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("trashSweptOn").and_then(Json::as_f64), Some(0.0));
    let cols: Vec<&str> = d
        .get("columns")
        .and_then(Json::as_array)
        .expect("columns")
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert_eq!(cols, ["name", "size", "date"]);
    // ColumnsWidth ships at 3, so a default install holds 3 columns from 900 px up.
    assert_eq!(d.get("columnsLimit").and_then(Json::as_f64), Some(3.0));
    assert_eq!(
        d.get("columnWidths")
            .and_then(Json::as_object)
            .map(<[(String, Json)]>::len),
        Some(0)
    );
    assert_eq!(
        d.get("sort")
            .and_then(|s| s.get("key"))
            .and_then(Json::as_str),
        Some("name")
    );
    assert_eq!(
        d.get("sort")
            .and_then(|s| s.get("reverse"))
            .and_then(Json::as_bool),
        Some(false)
    );
    // Sorting release: hidden files keep today's order, and each folder remembers its sort.
    assert_eq!(d.get("hiddenLast").and_then(Json::as_bool), Some(false));
    assert_eq!(d.get("rememberSort").and_then(Json::as_bool), Some(true));
    assert_eq!(
        d.get("folderSorts")
            .and_then(Json::as_object)
            .map(<[(String, Json)]>::len),
        Some(0)
    );
    assert_eq!(
        d.get("rememberViewPerFolder").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("folderViews")
            .and_then(Json::as_object)
            .map(<[(String, Json)]>::len),
        Some(0)
    );
    assert_eq!(
        d.get("dual")
            .and_then(|s| s.get("paths"))
            .and_then(Json::as_array)
            .map(<[Json]>::len),
        Some(0)
    );
    assert_eq!(
        d.get("dual")
            .and_then(|s| s.get("focus"))
            .and_then(Json::as_f64),
        Some(0.0)
    );
    // The left listing's share of the room right of the rail, in ten-thousandths: 5000 is the equal split 0.3.9 lays out.
    assert_eq!(
        d.get("dual")
            .and_then(|s| s.get("split"))
            .and_then(Json::as_f64),
        Some(5000.0)
    );
    // The shelf ships off, and its switch is what installs the plugin.
    assert_eq!(
        d.get("shelf")
            .and_then(|s| s.get("enabled"))
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("shelf")
            .and_then(|s| s.get("bar"))
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        d.get("shelf")
            .and_then(|s| s.get("rail"))
            .and_then(Json::as_str),
        Some("off")
    );
    assert_eq!(
        d.get("shelf")
            .and_then(|s| s.get("recent"))
            .and_then(Json::as_f64),
        Some(3.0)
    );
    assert_eq!(
        d.get("shelf")
            .and_then(|s| s.get("screenshots"))
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        d.get("shelf")
            .and_then(|s| s.get("recordings"))
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("sidebarWidth"))
            .and_then(Json::as_f64),
        Some(192.0)
    );
    // RailAdditions rule 4: the rail is a remembered state, and a fresh home remembers it shown.
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("rail"))
            .and_then(Json::as_str),
        Some("shown")
    );
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("autoHide"))
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("driveSize"))
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("trashCount"))
            .and_then(Json::as_bool),
        Some(false)
    );
    // Sidebar040: Recent ships off, so a fresh rail names no recent file.
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("showRecent"))
            .and_then(Json::as_bool),
        Some(false)
    );
    // Decided in 0.3.3: unmounted drives are on the rail unless the operator switches them off.
    assert_eq!(
        d.get("places")
            .and_then(|p| p.get("showUnmounted"))
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("loadOn"))
            .and_then(Json::as_str),
        Some("automatic")
    );
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("thumbnails"))
            .and_then(Json::as_str),
        Some("media")
    );
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("thumbSize"))
            .and_then(Json::as_str),
        Some("medium")
    );
    // ExtThumbs, decided 2026-09-23: network shares and phones are off, USB drives are on.
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("thumbNetwork"))
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("thumbPhone"))
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("thumbUsb"))
            .and_then(Json::as_bool),
        Some(true)
    );
    // Document view flips belong only to an open Quick Look.
    for leaf in ["markdownView", "htmlView"] {
        assert!(
            d.get("preview").expect("preview").get(leaf).is_none(),
            "{leaf} is not stored"
        );
    }
    // TallPreviews: the tall kinds show one compact facts line until `i` opens the full table.
    assert_eq!(
        d.get("previewFacts").and_then(Json::as_str),
        Some("compact")
    );
    assert_eq!(
        d.get("preview")
            .and_then(|p| p.get("wrap"))
            .and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        d.get("display")
            .and_then(|p| p.get("textSize"))
            .and_then(|t| t.get("mode"))
            .and_then(Json::as_str),
        Some("system")
    );
    let display: Vec<&str> = d
        .get("display")
        .and_then(Json::as_object)
        .expect("display")
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        display,
        ["textSize", "hyprlandIcons"],
        "the compositor owns opacity, icons and shadows"
    );
    let menu: Vec<&str> = d
        .get("menu")
        .and_then(Json::as_object)
        .expect("menu")
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(
        menu,
        ["hidden"],
        "the master row is derived from menu.hidden, not stored beside it"
    );
    assert_eq!(
        d.get("updates")
            .and_then(|u| u.get("autoCheck"))
            .and_then(Json::as_bool),
        Some(true)
    );
}
