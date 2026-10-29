use super::*;
use crate::keys::presentation::sheet;
use crate::keys::Frontend;
use crate::ui_logic_st1::test_support::{english, say};

fn cand(label: &str, keys: &str, section: u8, where_: &str) -> Candidate {
    Candidate { label: Text::data(label), keys: keys.into(), section, where_: Text::data(where_), ..Default::default() }
}

fn key_row(label: &str, keys: &str, action: &str) -> Candidate {
    Candidate { label: Text::data(label), keys: keys.into(), action: action.into(), ..Default::default() }
}

fn labels(rows: &[Candidate]) -> String {
    rows.iter().map(|row| say(&row.label)).collect::<Vec<_>>().join("|")
}

fn ranked(rows: &[Candidate], query: &str) -> Vec<Candidate> {
    rank(rows, query, &english())
}

fn sheet_actions() -> Vec<Candidate> {
    action_candidates(&sheet("default", Frontend::Gui, &english()))
}

fn query_rows() -> Vec<Candidate> {
    vec![
        cand("move", "j k", 0, ""),
        cand("trash", "dd", 0, ""),
        cand("menu", "m", 0, ""),
        cand("Open Downloads", "", 1, "Places"),
        cand("Open Documents", "", 1, "Places"),
        cand("Open receipt.pdf", "", 2, "Downloads"),
    ]
}

#[test]
fn rank_empty_query_keeps_every_row_in_order() {
    let rows = query_rows();
    assert_eq!(ranked(&rows, "").len(), 6);
    assert_eq!(say(&ranked(&rows, "  ")[0].label), "move");
}

#[test]
fn rank_label_substring_is_case_insensitive_and_section_ordered() {
    let rows = query_rows();
    assert_eq!(ranked(&rows, "open").len(), 3);
    assert_eq!(labels(&ranked(&rows, "open")), "Open Downloads|Open Documents|Open receipt.pdf");
    assert_eq!(ranked(&rows, "OPEN").len(), 3);
}

#[test]
fn rank_exact_label_ranks_first() {
    assert_eq!(say(&ranked(&query_rows(), "menu")[0].label), "menu");
}

#[test]
fn rank_exact_label_outranks_substring_ahead_of_it() {
    let rows = vec![cand("menu bar", "", 0, ""), cand("menu", "m", 0, "")];
    assert_eq!(labels(&ranked(&rows, "menu")), "menu|menu bar");
}

#[test]
fn rank_exact_place_name_ranks_first() {
    assert_eq!(say(&ranked(&query_rows(), "Open Documents")[0].label), "Open Documents");
}

#[test]
fn rank_keys_match_answers_when_no_label_does() {
    let keys = ranked(&query_rows(), "dd");
    assert_eq!(keys.len(), 1);
    assert_eq!(say(&keys[0].label), "trash");
}

#[test]
fn rank_no_match_is_empty() {
    let rows = query_rows();
    assert!(ranked(&rows, "zzz").is_empty());
    assert!(ranked(&rows, "Open ^z").is_empty());
}

#[test]
fn rank_capture_perm_finds_shipped_row() {
    let found = ranked(&sheet_actions(), "perm");
    let text: Vec<String> = found.iter().map(|row| format!("{} {}", row.keys, say(&row.label))).collect();
    assert_eq!(text.join("\n"), "shift-delete Delete permanently");
}

#[test]
fn rank_capture_mute_row_reads_its_where() {
    let found = ranked(&sheet_actions(), "mute");
    let text: Vec<String> =
        found.iter().map(|row| format!("{} {} in {}", row.keys, say(&row.label), say(&row.where_))).collect();
    assert_eq!(text.join("\n"), "m Mute in Preview");
}

#[test]
fn rank_fl_lists_favourite_then_recent_file() {
    let rail = [RailEntry {
        label: "flea".into(),
        group: "favourite".into(),
        kind: "favourite".into(),
        identity: "favourite:/home/probe/flea".into(),
    }];
    let mut rows = place_candidates(&rail);
    rows.extend(recent_candidates(&["/home/probe/Documents/claude/mix.flac".into()], "/home/probe"));
    let found: Vec<String> = ranked(&rows, "fl").iter().map(|row| format!("{} in {}", say(&row.label), say(&row.where_))).collect();
    assert_eq!(found.join("|"), "Open flea in Favorites|Open mix.flac in ~/Documents/claude");
}

#[test]
fn rank_exact_place_name_beats_exact_action_label() {
    let rows = vec![
        key_row("trash", "dd", "trash"),
        Candidate {
            label: Text::args("settings.sheet.open_place", vec![Text::data("Trash")]),
            name: "Trash".into(),
            section: 2,
            where_: Text::Key("settings.sheet.where_places"),
            rail_index: Some(0),
            place: Some(PlaceRef { identity: "trash:///".into() }),
            ..Default::default()
        },
    ];
    assert_eq!(say(&ranked(&rows, "trash")[0].label), "Open Trash");
}

#[test]
fn rank_sections_hold_order_across_all_four() {
    let rows: Vec<Candidate> = (0..4)
        .map(|section| cand(if section == 0 { "open sesame" } else { "Open Sesame" }, "", section, ""))
        .collect();
    let sections: Vec<u8> = ranked(&rows, "sesame").iter().map(|row| row.section).collect();
    assert_eq!(sections, vec![0, 1, 2, 3]);
}

#[test]
fn rank_result_list_is_bounded() {
    let rows: Vec<Candidate> = (0..80).map(|i| cand(&format!("open {i}"), "", (i % 4) as u8, "")).collect();
    assert!(ranked(&rows, "open").len() <= RESULT_LIMIT);
}

#[test]
fn rank_flyout_leaf_holds_the_first_row_and_parent_yields() {
    let flyout = vec![
        Candidate { label: Text::data("Compress"), section: 1, menu_action: "compress".into(), ..Default::default() },
        Candidate {
            label: Text::data("Compress to .zip"),
            section: 1,
            menu_action: "compress:zip".into(),
            parent_action: "compress".into(),
            ..Default::default()
        },
        Candidate { label: Text::data("Open with"), section: 1, menu_action: "openWith".into(), ..Default::default() },
    ];
    assert_eq!(labels(&ranked(&flyout, "comp")), "Compress to .zip");
    assert_eq!(labels(&ranked(&flyout, "open w")), "Open with");
}

#[test]
fn rank_parent_whose_leaves_the_bound_cuts_keeps_its_row() {
    let mut crowded = vec![Candidate { label: Text::data("Compress"), section: 1, menu_action: "compress".into(), ..Default::default() }];
    for k in 0..RESULT_LIMIT {
        crowded.push(cand(&format!("compress note {k}"), "", 2, ""));
    }
    crowded.push(Candidate {
        label: Text::data("Compress to .zip"),
        section: 1,
        menu_action: "compress:zip".into(),
        parent_action: "compress".into(),
        ..Default::default()
    });
    let labels_out: Vec<String> = ranked(&crowded, "comp").iter().map(|row| say(&row.label)).collect();
    assert!(labels_out.contains(&"Compress".to_string()));
    assert!(!labels_out.contains(&"Compress to .zip".to_string()));
}

#[test]
fn match_of_names_the_run_and_misses_nothing() {
    let at = match_of("Compress to .zip", "comp").expect("a match");
    assert_eq!((at.start, at.length), (0, 4));
    assert_eq!(match_of("trash", "zzz"), None);
    assert_eq!(match_of("trash", "  "), None);
}

#[test]
fn where_for_context_names_only_the_media_preview() {
    assert_eq!(say(&where_for_context("listing")), "");
    assert_eq!(say(&where_for_context("rail,menu")), "");
    assert_eq!(say(&where_for_context("media")), "Preview");
    assert_eq!(say(&where_for_context("pdf")), "");
    assert_eq!(say(&where_for_context("all")), "");
}

#[test]
fn run_decision_refuses_a_swallowed_action_while_a_listing_is_out() {
    assert_eq!(run_decision(true, "trash"), Run::Refuse(Text::Key("settings.sheet.loading")));
    assert_eq!(say(&Text::Key("settings.sheet.loading")), "A directory is already loading.");
}

#[test]
fn run_decision_acts_when_idle_or_for_an_answered_action() {
    assert_eq!(run_decision(false, "trash"), Run::Act);
    assert_eq!(run_decision(true, "open"), Run::Act);
    assert_eq!(run_decision(true, "reload"), Run::Act);
    assert_eq!(run_decision(true, ""), Run::Act);
}

#[test]
fn menu_run_decision_refuses_while_a_listing_is_out() {
    assert_eq!(menu_run_decision(true), Run::Refuse(Text::Key("settings.sheet.loading")));
    assert_eq!(menu_run_decision(false), Run::Act);
}

#[test]
fn dispatch_action_row_runs_its_key_action() {
    assert_eq!(dispatch(&key_row("trash", "dd", "trash")), Decision::Action { action: "trash".into() });
}

#[test]
fn dispatch_menu_confirm_disabled_and_nothing() {
    let menu = Candidate { label: Text::data("Move to Trash"), section: 1, menu_action: "trash".into(), ..Default::default() };
    assert_eq!(dispatch(&menu), Decision::Menu { menu_action: "trash".into() });
    let danger = Candidate { danger: true, menu_action: "deletePermanently".into(), ..menu.clone() };
    assert_eq!(dispatch(&danger), Decision::Confirm { menu_action: "deletePermanently".into() });
    let disabled = Candidate { disabled: true, ..menu };
    assert_eq!(dispatch(&disabled), Decision::Disabled);
    assert_eq!(dispatch(&Candidate { section: 0, ..Default::default() }), Decision::Nothing);
}

#[test]
fn dispatch_place_and_recent_rows() {
    let place = Candidate {
        section: 2,
        rail_index: Some(3),
        place: Some(PlaceRef { identity: "trash:///".into() }),
        ..Default::default()
    };
    assert_eq!(dispatch(&place), Decision::Place { rail_index: Some(3), identity: "trash:///".into() });
    let recent = Candidate { section: 3, path: "/home/gm/a.txt".into(), ..Default::default() };
    assert_eq!(dispatch(&recent), Decision::Recent { path: "/home/gm/a.txt".into() });
}

#[test]
fn place_index_answers_the_rail_own_row_for_duplicate_labels() {
    let rail = vec![
        RailEntry { label: "src".into(), group: "favourite".into(), kind: "favourite".into(), identity: "favourite:/a/src".into() },
        RailEntry { label: "src".into(), group: "favourite".into(), kind: "favourite".into(), identity: "favourite:/b/src".into() },
    ];
    let second = place_candidates(&rail).into_iter().find(|row| row.rail_index == Some(1)).expect("second src");
    assert_eq!(say(&second.label), "Open src");
    assert_eq!(place_index(&rail, &dispatch(&second)), Some(1));
    let first = place_candidates(&rail).into_iter().find(|row| row.rail_index == Some(0)).expect("first src");
    assert_eq!(place_index(&rail, &dispatch(&first)), Some(0));
}

#[test]
fn place_index_answers_none_for_a_gone_row_or_a_non_place() {
    let rail = vec![RailEntry { label: "src".into(), identity: "favourite:/b/src".into(), ..Default::default() }];
    let decided = Decision::Place { rail_index: Some(1), identity: "favourite:/b/src".into() };
    assert_eq!(place_index(&[], &decided), None);
    assert_eq!(place_index(&rail, &Decision::Nothing), None);
    assert_eq!(place_index(&rail, &Decision::Place { rail_index: None, identity: String::new() }), None);
}

#[test]
fn unique_keeps_the_key_row_and_takes_the_menu_wording_and_verdict() {
    let key = key_row("trash", "dd", "trash");
    let menu = Candidate {
        label: Text::data("Move to Trash"),
        section: 1,
        menu_action: "trash".into(),
        disabled: true,
        ..Default::default()
    };
    let merged = unique(&[key, menu]);
    assert_eq!(merged.len(), 1);
    assert_eq!(say(&merged[0].menu_label), "Move to Trash");
    assert_eq!(merged[0].section, 0);
    assert!(merged[0].disabled);
}

#[test]
fn rank_menu_wording_finds_the_action_once_under_its_key() {
    let rows = vec![
        key_row("trash", "dd", "trash"),
        Candidate { label: Text::data("Move to Trash"), section: 1, menu_action: "trash".into(), ..Default::default() },
    ];
    let found = ranked(&rows, "move to trash");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].keys, "dd");
    assert_eq!(found[0].action, "trash");
    assert_eq!(say(&found[0].label), "Move to Trash");
    assert_eq!(ranked(&rows, "trash").len(), 1);
}

#[test]
fn a_query_lists_the_sheet_row_by_its_key_and_by_a_word_of_its_name() {
    let resting = sheet("default", Frontend::Gui, &english());
    assert!(resting.iter().any(|row| row.action == "keymapSheet"), "the resting sheet keeps its ? row");
    let rows = sheet_actions();
    let by_key = ranked(&rows, "?");
    assert_eq!(by_key.len(), 1, "{:?}", labels(&by_key));
    assert_eq!(by_key[0].action, "keymapSheet");
    assert_eq!(by_key[0].keys, "?");
    assert_eq!(say(&by_key[0].label), "Keyboard Shortcuts");
    let by_name = ranked(&rows, "shortcuts");
    assert_eq!(by_name.len(), 1, "{:?}", labels(&by_name));
    assert_eq!(by_name[0].action, "keymapSheet");
    for query in ["keyboard", "short", "keyboard sh"] {
        assert!(ranked(&rows, query).iter().any(|row| row.action == "keymapSheet"), "{query}");
    }
    for query in ["ho", "cuts"] {
        assert!(ranked(&rows, query).iter().all(|row| row.action != "keymapSheet"), "{query}");
    }
}
