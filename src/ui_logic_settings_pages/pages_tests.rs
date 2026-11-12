// tests/js/settings.js: every pane row a section draws, and every value a control holds.
use super::super::rows::{first_row, focusable, step_row, Kind};
use super::super::test_support::say;
use super::*;
use crate::jsondoc::parse;
use crate::keys::{action_group, binding_rows, lookup, Context, Frontend, Input, PRESETS};

fn json(text: &str) -> Json {
    parse(text).expect("fixture parses")
}

fn display(text_size: &str, base: u32, scale: f64) -> Vec<Row> {
    let data = json(&format!(r#"{{"display":{{"textSize":{text_size}}}}}"#));
    display_rows(&State { base_size: base, monitor_scale: scale, ..State::new(&data) })
}

fn view(text: &str) -> Vec<Row> {
    view_rows(&State::new(&json(text)))
}

fn find<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("no row {id}"))
}

fn kinds(rows: &[Row]) -> Vec<Kind> {
    rows.iter().map(|row| row.kind).collect()
}

fn labels(row: &Row) -> Vec<String> {
    row.labels.iter().map(say).collect()
}

#[test]
fn display_is_text_size_then_scale_then_appearance() {
    let rows = display(r#"{"mode":"system"}"#, 14, 1.);
    use Kind::*;
    assert_eq!(kinds(&rows), [Group, Choice, Ruler, Hint, Fact, Group, Fact, Hint, Group, Check]);
    assert_eq!(say(&find(&rows, "textMode").value), "Follow Omarchy");
    assert_eq!(labels(find(&rows, "textMode")), ["Follow Omarchy", "Override"]);
    assert_eq!(rows[2].stops, [9, 10, 11, 12, 14, 16, 20]);
    assert_eq!(rows[2].index, 4);
    let between = display(r#"{"mode":"system"}"#, 13, 1.);
    assert_eq!(between[2].index, 3);
    assert_eq!(say(&rows[3].label), "Ctrl+Shift++ and Ctrl+Shift+- step through them; Ctrl+Shift+0 follows Omarchy again.");
    assert_eq!(format!("{}|{}|{:?}", say(&rows[4].label), say(&rows[4].value), rows[4].role), "Effective|14 px|Live");
    assert_eq!(format!("{}|{}", say(&between[4].value), say(&between[4].caption)), "13 px|Omarchy's own size. The ruler marks 12, the nearest stop.");
    assert_eq!(say(&rows[4].caption), "Omarchy's own size.");
    assert_eq!(say(&rows[6].value), "1x");
    assert_eq!(say(&display(r#"{"mode":"system"}"#, 14, 1.25)[6].value), "1.25x");
    assert_eq!(say(&display(r#"{"mode":"system"}"#, 14, 0.)[6].value), "not reported");
    assert_eq!(say(&display(r#"{"mode":"system"}"#, 14, f64::NAN)[6].value), "not reported");
    assert_eq!(say(&rows[7].label), "Flea follows Hyprland's scale and does not change it.");
    assert_eq!((rows[9].id.as_str(), rows[9].on), ("display.hyprlandIcons", false));
}

#[test]
fn an_override_turns_the_ruler_into_the_control() {
    let follow = display(r#"{"mode":"system"}"#, 14, 1.);
    let pinned = display(r#"{"mode":16}"#, 14, 1.);
    assert_eq!(pinned.len(), follow.len());
    assert_eq!(say(&find(&pinned, "textMode").value), "Override");
    let effective = find(&pinned, "textEffective");
    assert_eq!(format!("{}|{}", say(&effective.value), say(&effective.caption)), "16 px|Your override.");
    assert_eq!(pinned[2].index, 5);
    assert!(find(&pinned, "textStop").on);
    assert!(!find(&follow, "textStop").on);
}

#[test]
fn display_cursor_skips_read_only_rows() {
    let follow = display(r#"{"mode":"system"}"#, 14, 1.);
    assert_eq!(first_row(&follow), 1);
    assert_eq!(step_row(&follow, 1, 1), 9);
    let pinned = display(r#"{"mode":16}"#, 16, 1.);
    assert_eq!(step_row(&pinned, 1, 1), 2);
    assert_eq!(step_row(&pinned, 2, 1), 9);
}

#[test]
fn keys_pane_leads_with_the_preset_choice() {
    let data = json("{}");
    let keys = key_rows(&State { preset: "mac", ..State::new(&data) });
    assert_eq!(keys[1].kind, Kind::Choice);
    assert_eq!(say(&keys[1].value), "Mac");
    assert_eq!(labels(&keys[1]), ["Default", "Vim", "Mac", "Windows"]);
    assert_eq!(format!("{:?}|{}", keys[2].kind, say(&keys[2].label)), "Hint|Keys change, actions do not.");
    assert_eq!(say(&key_rows(&State { preset: "windows", ..State::new(&data) })[1].value), "Windows");
    assert_eq!(say(&key_rows(&State { preset: "emacs", ..State::new(&data) })[1].value), "emacs");
    assert_eq!(say(&keys.last().unwrap().label), "Press ? for Keyboard Shortcuts.");
}

#[test]
fn every_preset_previews_six_bound_examples() {
    assert_eq!(PRESETS, ["default", "vim", "mac", "windows"]);
    let primary = [("default", "y,p,dd"), ("vim", "yy,pp,D"), ("mac", "super-c,super-v,delete"), ("windows", "ctrl-c,ctrl-v,delete")];
    let data = json("{}");
    let mut total = 0;
    for (preset, keys) in primary {
        let section = key_rows(&State { preset, ..State::new(&data) });
        let preview = find(&section, "keyPreview");
        assert_eq!(preview.items.len(), 6, "{preset}");
        assert!(!focusable(preview));
        assert_eq!(preview.items[3..].iter().map(|i| i.keys.as_str()).collect::<Vec<_>>().join(","), keys);
        let enter = lookup(preset, Context::Listing, Frontend::Gui, Input { key: "Return", text: "", modifiers: 0 }).action();
        let label = binding_rows(preset, Frontend::Gui).find(|r| action_group(r.action) == enter).unwrap().label_key;
        assert_eq!(preview.items[1].label.key(), Some(label), "{preset}");
        for item in &preview.items[3..] {
            assert!(binding_rows(preset, Frontend::Gui).any(|r| r.keys == item.keys && Some(r.label_key) == item.label.key()), "{preset} {}", item.keys);
        }
        let table: Vec<_> = binding_rows(preset, Frontend::Gui).collect();
        assert!(table.len() > 20);
        for row in &table {
            assert_eq!(lookup(preset, Context::Listing, Frontend::Gui, row.input()).action(), row.action);
            total += 1;
        }
    }
    assert!(total > 100);
    let default = key_preview("default");
    let words: Vec<String> = default.iter().map(|i| say(&i.label)).collect();
    assert_eq!(words, ["Move cursor", "Open", "Quick Look", "Copy", "Paste", "Move to Trash"]);
    // The arrows are prose, so the cell draws a catalog word in place of the key name (S3-2).
    for preset in PRESETS {
        for item in key_preview(preset) {
            let word = (item.keys == "arrows").then_some(Text::Key("settings.keys.arrows"));
            assert_eq!(item.word, word.unwrap_or(Text::Empty), "{preset} {}", item.keys);
        }
    }
    assert_eq!(say(&default[0].word), "arrows");
}

#[test]
fn no_preset_draws_a_hollow_hint() {
    let data = json(r#"{"preview":{"loadOn":"manual"}}"#);
    for preset in PRESETS {
        let state = State { preset, ..State::new(&data) };
        let mut drawn = display_rows(&state);
        drawn.extend(key_rows(&state));
        drawn.extend(preview_rows(&state));
        for row in drawn.iter().filter(|r| r.kind == Kind::Hint) {
            let text = say(&row.label);
            assert!(!text.trim().is_empty() && !text.starts_with(' ') && !text.contains("  "), "{preset}: {text:?}");
        }
    }
    let hints = |state: &State<'_>, needle: &str| {
        display_rows(state).into_iter().chain(preview_rows(state)).filter(|r| r.kind == Kind::Hint && say(&r.label).contains(needle)).count()
    };
    let state = State::new(&data);
    assert!(hints(&state, "step through them").min(hints(&state, "loads the current selection")) >= 1);
}

#[test]
fn view_rows_read_every_stored_value() {
    let code = find(&view("{}"), "langMarks").clone();
    assert_eq!(format!("{:?}|{}|{}|{}", code.kind, say(&code.label), code.glyph, code.on), "Check|Show code icons|code|true");
    assert!(!find(&view(r#"{"langMarks":false}"#), "langMarks").on);
    assert!(find(&view(r#"{"langMarks":"false"}"#), "langMarks").on);
    let state = r#"{"view":"grid","density":"comfortable","columns":["name","kind"],
        "preview":{"column":false,"loadOn":"manual","thumbnails":"off","thumbSize":"xlarge","ctrlZoom":false}}"#;
    let rows = view(state);
    assert_eq!(find(&rows, "view").selected, "grid");
    assert_eq!(say(&find(&rows, "columns").value), "Name, Kind");
    assert_eq!(say(&find(&view("{}"), "columns").value), "Name, Size, Modified");
    assert_eq!(say(&find(&view(r#"{"columns":["name","owner"]}"#), "columns").value), "Name, Owner");
    assert_eq!(find(&rows, "density").selected, "comfortable");
    assert_eq!(find(&view("{}"), "density").selected, "compact");
    assert_eq!(find(&rows, "foldersFirst").glyph, "folders-first");
    assert_eq!(SECTIONS[section_index("preview")].glyph, "preview");
    assert_eq!(say(&find(&rows, "groupByKind").caption), "folders, photos, files");
    assert_eq!(say(&find(&rows, "wrapAtEnds").caption), "up at the top");
    let escape = find(&rows, "escapeUp");
    assert_eq!(format!("{}|{}|{}", say(&escape.label), escape.on, say(&escape.caption)), "Escape goes up a folder|false|with nothing to close");
    assert!(find(&view(r#"{"escapeUp":true}"#), "escapeUp").on);
    let open = find(&rows, "openMode");
    assert_eq!((open.selected.as_str(), open.values.join(",")), ("double", "double,single".into()));
    assert_eq!(labels(open), ["Double Click", "Single Click"]);
    assert_eq!(open.glyph, "pointer");
    let rename = find(&rows, "clickRename");
    assert_eq!(format!("{}|{}|{}", say(&rename.label), rename.on, rename.glyph), "Click a selected name to rename|true|");
    assert_eq!(find(&rows, "escapeUp").glyph, "");
    let single = view(r#"{"openMode":"single"}"#);
    assert_eq!(find(&single, "openMode").selected, "single");
    assert!(!find(&single, "clickRename").available && !focusable(find(&single, "clickRename")));
    assert!(find(&rows, "clickRename").available && focusable(find(&rows, "clickRename")));
    assert_eq!(format!("{}|{}", say(&find(&rows, "highlightToday").label), find(&rows, "highlightToday").on), "Highlight today's dates|false");
    assert!(find(&view(r#"{"highlightToday":true}"#), "highlightToday").on);
    assert_eq!(find(&rows, "columnsLimit").selected, "3");
    assert_eq!(find(&view(r#"{"columnsLimit":4}"#), "columnsLimit").selected, "4");
    assert!(rows.last().unwrap().footer);
}

#[test]
fn view_gains_git_status_before_today_and_off_as_it_ships() {
    let rows = view("{}");
    let git = rows.iter().position(|r| r.id == "showGitStatus").unwrap();
    assert_eq!(rows[git + 1].id, "highlightToday");
    assert_eq!(rows[git - 1].id, "columnsLimit");
    assert_eq!(format!("{}|{}|{}", say(&rows[git].label), rows[git].glyph, rows[git].on), "Show Git status|folder-git-2|false");
    assert!(find(&view(r#"{"showGitStatus":true}"#), "showGitStatus").on);
    assert_eq!(labels(find(&rows, "sort.key")), ["Name", "Size", "Modified", "Kind"]);
}

#[test]
fn opening_rows_and_the_last_folder_hint() {
    let opening = view("{}");
    assert_eq!(find(&opening, "startIn").selected, "home");
    assert_eq!(find(&opening, "startIn").values, ["home", "last", "folder"]);
    assert_eq!(labels(find(&opening, "startIn")), ["Home", "Last", "Chosen"]);
    assert_eq!(say(&find(&opening, "startFolder").value), "Use this folder");
    assert_eq!(find(&opening, "newTab").selected, "current");
    assert_eq!(find(&opening, "newTab").values, ["current", "home", "start"]);
    let opened = view(r#"{"startIn":"folder","startFolder":"/home/gm/Work","newTab":"home"}"#);
    assert_eq!(say(&find(&opened, "startFolder").value), "/home/gm/Work");
    assert_eq!(find(&opened, "startIn").selected, "folder");
    assert_eq!(find(&opened, "newTab").selected, "home");
    let hint = "Last folder reopens every tab you had.";
    let hinted = view(r#"{"startIn":"last"}"#);
    let at: Vec<usize> = hinted.iter().enumerate().filter(|(_, r)| say(&r.label) == hint).map(|(i, _)| i).collect();
    assert_eq!(at.len(), 1);
    assert_eq!(hinted[at[0]].kind, Kind::Hint);
    assert_eq!(at[0] - hinted.iter().position(|r| r.id == "startIn").unwrap(), 1);
    assert!(!view(r#"{"startIn":"folder"}"#).iter().any(|r| say(&r.label) == hint));
    assert!(!view("{}").iter().any(|r| say(&r.label) == hint));
}

#[test]
fn the_save_footer_is_muted_and_a_failure_keeps_its_error_role() {
    let saved = view("{}");
    let last = saved.last().unwrap();
    assert_eq!(format!("{}|{:?}|{}", say(&last.label), last.role, last.footer), "Saved|None|true");
    let data = json("{}");
    let failed = view_rows(&State { save_error: Some(Text::Key("settings.error.save_session")), ..State::new(&data) });
    let last = failed.last().unwrap();
    assert_eq!(format!("{}|{:?}|{}", say(&last.label), last.role, last.footer), "Could not save settings \u{b7} changes apply to this session only|Error|true");
}

#[test]
fn preview_rows_keep_their_dependents() {
    let state = r#"{"preview":{"column":false,"loadOn":"manual","thumbnails":"off","thumbSize":"xlarge","ctrlZoom":false}}"#;
    let data = json(state);
    let preview = preview_rows(&State::new(&data));
    assert!(!find(&preview, "preview.column").on);
    assert_eq!(say(&find(&preview, "preview.loadOn").value), "Manual");
    assert_eq!(find(&preview, "preview.thumbSize").values, ["small", "medium", "large", "xlarge", "huge", "largest"]);
    assert_eq!(find(&preview, "preview.thumbnails").selected, "off");
    assert!(!find(&preview, "preview.ctrlZoom").on);
    let sizes = ["Small", "Medium", "Large", "Extra large", "Huge", "Largest"];
    for (value, word) in ["small", "medium", "large", "xlarge", "huge", "largest"].iter().zip(sizes) {
        let data = json(&format!(r#"{{"preview":{{"thumbSize":"{value}"}}}}"#));
        let row = find(&preview_rows(&State::new(&data)), "preview.thumbSize").clone();
        assert!(row.caption.is_empty());
        assert_eq!(say(&row.value), word);
    }
    assert_eq!(format!("{:?}|{}|{}", preview[3].kind, say(&preview[3].label), preview[3].footer), "Hint|Ctrl+Space loads the current selection.|false");
    assert_eq!(say(&preview_rows(&State::new(&json("{}")))[3].label), "Automatic follows the cursor.");
    let shown_data = json(r#"{"preview":{"column":true,"thumbnails":"off"}}"#);
    let shown = preview_rows(&State::new(&shown_data));
    let load = find(&preview, "preview.loadOn");
    assert_eq!((load.available, focusable(load), find(&shown, "preview.loadOn").available), (false, false, true));
    assert!(find(&shown, "preview.ctrlZoom").available && find(&shown, "preview.thumbSize").available);
    assert_eq!(say(&find(&shown, "preview.ctrlZoom").label), "Zoom the grid with ctrl and scroll");
    assert!(!find(&shown, "preview.thumbUsb").available);
}

#[test]
fn columns_rows_lock_name_and_read_stored_columns() {
    let data = json(r#"{"columns":["name","kind"]}"#);
    let columns = column_rows(&data);
    assert_eq!(columns[1].kind, Kind::Lock);
    assert!(find(&columns, "column:kind").on);
    assert!(!find(&columns, "column:size").on);
}

#[test]
fn sections_carry_plugins_in_place_of_the_shelf() {
    let ids: Vec<&str> = SECTIONS.iter().map(|s| s.id).collect();
    assert_eq!(ids, ["view", "places", "plugins", "preview", "keys", "display", "menus", "about"]);
    assert_eq!(section_index("nothing"), 0);
    assert_eq!(say(&Text::Key(SECTIONS[2].label)), "Plugins");
    let data = json("{}");
    let plugins = vec![Row::group("settings.group.view")];
    assert_eq!(rows("plugins", &State::new(&data), plugins.clone(), Vec::new), plugins);
    assert_eq!(rows("unknown", &State::new(&data), Vec::new(), Vec::new), key_rows(&State::new(&data)));
    assert_eq!(rows("menus", &State::new(&json(r#"{"menu":{"hidden":["paste"]}}"#)), Vec::new(), Vec::new)[0].state, super::super::rows::Master::Some);
}

// The schema keys View draws read their own stored values and fall back to the shipped defaults.
#[test]
fn view_controls_read_their_stored_keys_and_the_shipped_defaults() {
    let rows = view("{}");
    assert_eq!(find(&rows, "addressBar").selected, "breadcrumb");
    assert_eq!(say(&find(&rows, "addressBar").value), "Breadcrumb");
    assert!(!find(&rows, "openFoldersWhileDragging").on);
    assert!(find(&rows, "rememberSort").on);
    let rows = view(r#"{"addressBar":"path","openFoldersWhileDragging":true,"rememberSort":false}"#);
    assert_eq!(find(&rows, "addressBar").selected, "path");
    assert_eq!(say(&find(&rows, "addressBar").value), "Path");
    assert!(find(&rows, "openFoldersWhileDragging").on);
    assert!(!find(&rows, "rememberSort").on);
}

// tests/js/settings.js withoutKeys: a key the preset leaves unbound leaves its hint out, so no sentence is drawn with a hole.
#[test]
fn an_unbound_key_leaves_its_hint_out_and_no_sentence_is_holed() {
    let (empty, manual) = (json("{}"), json(r#"{"preview":{"loadOn":"manual"}}"#));
    let state = State::new(&empty);
    let holed = |text: &str| text.trim().is_empty() || text.starts_with(' ') || text.contains("  ") || [" .", " ,", " ;"].iter().any(|gap| text.contains(gap));
    for row in display_rows(&state).iter().chain(&preview_rows(&State::new(&manual))).filter(|row| row.kind == Kind::Hint) {
        assert!(!holed(&say(&row.label)), "{:?}", say(&row.label));
    }
    let keys = |list: &[&str]| list.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();
    assert_eq!(display::sentence("settings.hint.text_keys", keys(&["ctrl-shift-+", "", "ctrl-shift-0"])), None);
    assert_eq!(display::sentence("settings.hint.text_keys", keys(&["", "", ""])), None);
    let drawn = display::sentence("settings.hint.text_keys", keys(&["a", "b", "c"])).map(|text| say(&text)).unwrap_or_default();
    assert!(drawn.contains("step through them") && !holed(&drawn), "{drawn:?}");
    assert!(view::load_hint(true, String::new()).is_none());
    assert_eq!(view::load_hint(true, "ctrl-space".into()).map(|row| say(&row.label)).as_deref(), Some("ctrl-space loads the current selection."));
    assert_eq!(view::load_hint(false, String::new()).map(|row| say(&row.label)).as_deref(), Some("Automatic follows the cursor."));
    assert_eq!(say(&super::super::textsize::announce(super::super::textsize::Stored::Stop(16), 14, "")), "Text size 16px");
}

// tests/js/settings.js: every preset's section names its own preset in the choice.
#[test]
fn every_preset_section_names_its_own_preset() {
    let data = json("{}");
    let named: Vec<String> = PRESETS.iter().map(|preset| say(&key_rows(&State { preset, ..State::new(&data) })[1].value)).collect();
    assert_eq!(named, ["Default", "Vim", "Mac", "Windows"]);
}
