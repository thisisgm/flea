use super::*;

// menu.hidden stores what is hidden, so an action added later is visible without a migration.
#[test]
fn menu_hidden_holds_the_shipped_ids_and_nothing_else() {
    let d = defaults();
    let hidden: Vec<&str> = d
        .get("menu")
        .and_then(|m| m.get("hidden"))
        .and_then(Json::as_array)
        .expect("menu.hidden")
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert_eq!(
        hidden,
        // HIG CHANGES keeps Actions on while Run Script and Open in Agent remain off; Properties and Permissions ship shown and the view rows leave the file menu.
        [
            "delete",
            "openTerminal",
            "placeMenu",
            "runScript",
            "moveto",
            "copyto",
            "copyAs",
            "pasteAs",
            "invertSelection",
            "extThumbs",
            "run",
            "openAsText",
            "openAgent",
            "toggleHidden",
            "toggleLangMarks"
        ]
    );
}

// Code icons default on; a saved false stays off, and invalid values heal to the default.
#[test]
fn language_marks_default_on_and_keep_a_saved_false() {
    let current = crate::uistate::from_file("{}");
    assert_eq!(current.get("langMarks").and_then(Json::as_bool), Some(true));
    for value in ["true", "false"] {
        let patch = jsondoc::parse(&format!(r#"{{"langMarks":{value}}}"#)).expect("boolean patch");
        let next = crate::uistate::patched(&current, &patch).expect("boolean accepted");
        assert_eq!(
            next.get("langMarks").and_then(Json::as_bool),
            Some(value == "true")
        );
    }
    let off = crate::uistate::from_file(r#"{"langMarks":false}"#);
    let patch = jsondoc::parse(r#"{"hidden":true}"#).expect("unrelated patch");
    let kept = crate::uistate::patched(&off, &patch).expect("saved false retained");
    assert_eq!(kept.get("langMarks").and_then(Json::as_bool), Some(false));
    for value in [r#""true""#, r#""false""#, "1", "null", "[]", "{}"] {
        let text = format!(r#"{{"langMarks":{value}}}"#);
        let patch = jsondoc::parse(&text).expect("patch");
        assert!(crate::uistate::patched(&current, &patch)
            .expect_err("not a boolean")
            .contains("langMarks"));
        assert_eq!(
            crate::uistate::from_file(&text)
                .get("langMarks")
                .and_then(Json::as_bool),
            Some(true)
        );
    }
}

// The facts mode takes its two words, and a file holding any other word reads as compact.
#[test]
fn the_preview_facts_mode_takes_two_words_and_a_bad_one_reads_compact() {
    let current = crate::uistate::from_file("{}");
    let takes = |patch: &str| {
        crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"))
    };
    assert!(
        takes(r#"{"previewFacts":"compact"}"#).is_ok()
            && takes(r#"{"previewFacts":"full"}"#).is_ok()
    );
    assert!(takes(r#"{"previewFacts":"wide"}"#)
        .expect_err("refused")
        .contains("previewFacts"));
    let word = |text: &str| {
        crate::uistate::from_file(text)
            .get("previewFacts")
            .and_then(Json::as_str)
            .map(str::to_owned)
    };
    assert_eq!(
        word(r#"{"previewFacts":"wide"}"#).as_deref(),
        Some("compact")
    );
    assert_eq!(word(r#"{"previewFacts":"full"}"#).as_deref(), Some("full"));
}

// Each key a place, each value a sort order; past the cap the oldest entries go.
#[test]
fn folder_sorts_hold_places_to_orders_and_heal_past_the_cap() {
    let current = crate::uistate::from_file("{}");
    let takes = |patch: &str| {
        crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"))
    };
    for good in [
        r#"{"folderSorts":{}}"#,
        r#"{"folderSorts":{"/home/gm/Work":{"key":"size","reverse":true}}}"#,
        r#"{"folderSorts":{"smb://nas/isos":{"key":"date","reverse":false}}}"#,
        r#"{"rememberSort":false}"#,
        r#"{"hiddenLast":true}"#,
        r#"{"highlightToday":true}"#,
    ] {
        assert!(takes(good).is_ok(), "{} is a value its key takes", good);
    }
    for (bad, named) in [
        (r#"{"folderSorts":[]}"#, "folderSorts"),
        (
            r#"{"folderSorts":{"Work":{"key":"size","reverse":true}}}"#,
            "folderSorts",
        ),
        (
            r#"{"folderSorts":{"/a":{"key":"mode","reverse":false}}}"#,
            "folderSorts",
        ),
        (r#"{"folderSorts":{"/a":{"key":"size"}}}"#, "folderSorts"),
        (
            r#"{"folderSorts":{"/a":{"key":"size","reverse":1}}}"#,
            "folderSorts",
        ),
        (
            r#"{"folderSorts":{"/a":{"key":"size","reverse":false,"by":"x"}}}"#,
            "folderSorts",
        ),
        (r#"{"hiddenLast":"yes"}"#, "hiddenLast"),
        (r#"{"highlightToday":"yes"}"#, "highlightToday"),
        (r#"{"rememberSort":1}"#, "rememberSort"),
    ] {
        let message = takes(bad).expect_err("the patch must be refused");
        assert!(
            message.contains(named),
            "{} should name {}, got {}",
            bad,
            named,
            message
        );
    }
    // A file carrying a bad map costs that key its default and nothing else.
    let read = crate::uistate::from_file(
        r#"{"folderSorts":{"Work":{"key":"size","reverse":true}},"hiddenLast":true}"#,
    );
    assert_eq!(
        read.get("folderSorts")
            .and_then(Json::as_object)
            .map(<[(String, Json)]>::len),
        Some(0)
    );
    assert_eq!(read.get("hiddenLast").and_then(Json::as_bool), Some(true));
    // Past the cap the oldest entries go and the newest stay.
    let mut big = String::from(r#"{"folderSorts":{"#);
    for i in 0..MAX_FOLDER_SORTS + 2 {
        if i > 0 {
            big.push(',');
        }
        big.push_str(&format!(r#""/d{:03}":{{"key":"size","reverse":false}}"#, i));
    }
    big.push_str("}}");
    let kept = takes(&big).expect("a long map still patches");
    let map = kept
        .get("folderSorts")
        .and_then(Json::as_object)
        .expect("folderSorts");
    assert_eq!(map.len(), MAX_FOLDER_SORTS);
    assert!(
        map.iter().all(|(k, _)| k != "/d000" && k != "/d001"),
        "the two oldest go"
    );
    assert!(map.iter().any(|(k, _)| k == "/d501"), "the newest stays");
}

// Folder views are opt-in and only name layouts one pane can draw. Dual is a window layout.
#[test]
fn folder_views_hold_places_to_browser_modes_and_heal_past_the_cap() {
    let current = crate::uistate::from_file("{}");
    let takes = |patch: &str| {
        crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"))
    };
    for good in [
        r#"{"rememberViewPerFolder":true}"#,
        r#"{"folderViews":{"/home/gm/Pictures":"grid","smb://nas/docs":"columns"}}"#,
    ] {
        assert!(takes(good).is_ok(), "{} is a value its key takes", good);
    }
    for (bad, named) in [
        (r#"{"folderViews":[]}"#, "folderViews"),
        (r#"{"folderViews":{"Pictures":"grid"}}"#, "folderViews"),
        (r#"{"folderViews":{"/a":"dual"}}"#, "folderViews"),
        (r#"{"folderViews":{"/a":"tiles"}}"#, "folderViews"),
        (r#"{"rememberViewPerFolder":1}"#, "rememberViewPerFolder"),
    ] {
        let message = takes(bad).expect_err("the patch must be refused");
        assert!(
            message.contains(named),
            "{} should name {}, got {}",
            bad,
            named,
            message
        );
    }
    let read = crate::uistate::from_file(
        r#"{"folderViews":{"/home/gm/Pictures":"grid","Pictures":"list","/home/gm/Docs":"dual"},"hidden":true}"#,
    );
    let map = read
        .get("folderViews")
        .and_then(Json::as_object)
        .expect("folderViews");
    assert_eq!(map.len(), 1, "only bad entries go");
    assert_eq!(
        map.iter()
            .next()
            .map(|(key, value)| (key.as_str(), value.as_str())),
        Some(("/home/gm/Pictures", Some("grid")))
    );
    assert_eq!(read.get("hidden").and_then(Json::as_bool), Some(true));
    let mut big = String::from(r#"{"folderViews":{"#);
    for i in 0..MAX_FOLDER_VIEWS + 2 {
        if i > 0 {
            big.push(',');
        }
        big.push_str(&format!(r#""/d{:03}":"grid""#, i));
    }
    big.push_str("}}");
    let kept = takes(&big).expect("a long map still patches");
    let map = kept
        .get("folderViews")
        .and_then(Json::as_object)
        .expect("folderViews");
    assert_eq!(map.len(), MAX_FOLDER_VIEWS);
    assert!(map.iter().all(|(key, _)| key != "/d000" && key != "/d001"));
    assert!(map.iter().any(|(key, _)| key == "/d501"));
}

// The rules nothing else reached: an exact stop, a non-empty path, and the four shipped presets.
#[test]
fn the_stop_the_preset_and_the_favourites_rules_each_bite_at_their_own_edge() {
    let current = crate::uistate::from_file("{}");
    let takes = |patch: &str| {
        crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"))
    };
    for good in [
        r#"{"display":{"textSize":{"mode":"system"}}}"#,
        r#"{"display":{"textSize":{"mode":9}}}"#,
        r#"{"keys":"default"}"#,
        r#"{"keys":"vim"}"#,
        r#"{"keys":"mac"}"#,
        r#"{"keys":"windows"}"#,
        r#"{"pickerView":"list"}"#,
        r#"{"pickerView":"grid"}"#,
        r#"{"escapeUp":true}"#,
        r#"{"escapeUp":false}"#,
        r#"{"openMode":"double"}"#,
        r#"{"openMode":"single"}"#,
        r#"{"clickRename":true}"#,
        r#"{"clickRename":false}"#,
        r#"{"places":{"favourites":[]}}"#,
        r#"{"places":{"driveSize":true,"trashCount":true}}"#,
        r#"{"places":{"driveSize":false,"trashCount":false}}"#,
        r#"{"places":{"rail":"hidden"}}"#,
        r#"{"places":{"rail":"shown"}}"#,
        r#"{"places":{"showUnmounted":false}}"#,
        r#"{"updates":{"autoCheck":false}}"#,
    ] {
        assert!(takes(good).is_ok(), "{} is a value its key takes", good);
    }
    for (bad, named) in [
        (
            r#"{"display":{"textSize":{"mode":13}}}"#,
            "display.textSize.mode",
        ),
        (
            r#"{"display":{"textSize":{"mode":14.5}}}"#,
            "display.textSize.mode",
        ),
        (
            r#"{"display":{"textSize":{"mode":"14"}}}"#,
            "display.textSize.mode",
        ),
        (
            r#"{"display":{"textSize":{"mode":"override"}}}"#,
            "display.textSize.mode",
        ),
        (r#"{"display":{"opacity":1.0}}"#, "display.opacity"),
        (r#"{"display":{"shadows":true}}"#, "display.shadows"),
        (r#"{"menu":{"basic":false}}"#, "menu.basic"),
        (r#"{"escapeUp":"yes"}"#, "escapeUp"),
        (r#"{"openMode":"triple"}"#, "openMode"),
        (r#"{"clickRename":1}"#, "clickRename"),
        (r#"{"keys":"emacs"}"#, "keys"),
        (r#"{"pickerView":"columns"}"#, "pickerView"),
        (r#"{"language":"en"}"#, "language"),
        (r#"{"places":{"favourites":"/a"}}"#, "places.favourites"),
        (r#"{"places":{"driveSize":1}}"#, "places.driveSize"),
        (r#"{"places":{"trashCount":"true"}}"#, "places.trashCount"),
        (r#"{"places":{"rail":"off"}}"#, "places.rail"),
        (r#"{"places":{"rail":true}}"#, "places.rail"),
        (r#"{"updates":{"autoCheck":"yes"}}"#, "updates.autoCheck"),
        (r#"{"stateVersion":0}"#, "stateVersion"),
        (r#"{"stateVersion":2}"#, "stateVersion"),
    ] {
        let message = takes(bad).expect_err("the patch must be refused");
        assert!(
            message.contains(named),
            "{} should name {}, got {}",
            bad,
            named,
            message
        );
    }
}

// Tabs040 callout 2: a bounded list of places in tab order, with the current tab's index
// inside it. A file carrying a bad one costs that key its default, the way a bad place does.
#[test]
fn last_tabs_hold_every_tab_folder_with_its_current_index() {
    let current = crate::uistate::from_file("{}");
    let takes = |patch: &str| {
        crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"))
    };
    for good in [
        r#"{"lastTabs":{"paths":[],"index":0}}"#,
        r#"{"lastTabs":{"paths":["/home/gm/Work"],"index":0}}"#,
        r#"{"lastTabs":{"paths":["/a","smb://nas/isos","/b"],"index":2}}"#,
    ] {
        assert!(takes(good).is_ok(), "{} is a value its key takes", good);
    }
    for (bad, named) in [
        (r#"{"lastTabs":[]}"#, "lastTabs"),
        (r#"{"lastTabs":{"paths":[],"index":1}}"#, "lastTabs"),
        (r#"{"lastTabs":{"paths":["/a"],"index":1}}"#, "lastTabs"),
        (r#"{"lastTabs":{"paths":["/a"],"index":-1}}"#, "lastTabs"),
        (r#"{"lastTabs":{"paths":["/a"],"index":0.5}}"#, "lastTabs"),
        (r#"{"lastTabs":{"paths":["Work"],"index":0}}"#, "lastTabs"),
        (r#"{"lastTabs":{"paths":[""],"index":0}}"#, "lastTabs"),
        (
            r#"{"lastTabs":{"paths":["/a"],"index":0,"by":"x"}}"#,
            "lastTabs",
        ),
        (r#"{"lastTabs":{"paths":"/a","index":0}}"#, "lastTabs"),
    ] {
        let message = takes(bad).expect_err("the patch must be refused");
        assert!(
            message.contains(named),
            "{} should name {}, got {}",
            bad,
            named,
            message
        );
    }
    let mut big = String::from(r#"{"lastTabs":{"paths":["#);
    for i in 0..MAX_LAST_TABS + 1 {
        if i > 0 {
            big.push(',');
        }
        big.push_str(&format!(r#""/d{:03}""#, i));
    }
    big.push_str(r#"],"index":0}}"#);
    let message = takes(&big).expect_err("past the tab cap the patch must be refused");
    assert!(message.contains("lastTabs"), "got {}", message);
    let read = crate::uistate::from_file(r#"{"lastTabs":{"paths":["/a"],"index":3}}"#);
    assert_eq!(
        read.get("lastTabs")
            .and_then(|t| t.get("paths"))
            .and_then(Json::as_array)
            .map(<[Json]>::len),
        Some(0)
    );
}

#[test]
fn every_default_key_carries_a_rule_and_no_rule_is_orphaned() {
    assert_eq!(
        rule_names(SCHEMA),
        default_names(),
        "the schema and the shipped shape must name the same keys"
    );
}
