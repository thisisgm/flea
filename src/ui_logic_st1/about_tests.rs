use super::*;
use crate::ui_logic_st1::rows::{focusable, Kind, Row};
use crate::ui_logic_st1::test_support::say;
use crate::ui_logic_st1::update::{self, Status};

const LOCAL_QI: &str = "Name            : flea\nVersion         : 0.3.4-1\nPackager        : Unknown Packager\n\
    Build Date      : Wed Sep 23 12:40:50 2026\nInstall Script  : No\nValidated By    : None\n";

fn answered_status(line: &str) -> Status {
    update::answered(&update::checking(&update::idle()), line, 1000)
}

fn about(status: Status) -> Vec<Row> {
    rows(&Facts { update: status, ..Facts::default() }, true, "English", Vec::new())
}

fn stub_make_default() -> Vec<Row> {
    vec![Row { id: "makeDefault".to_owned(), ..Row::new(Kind::Check, Text::data("stub")) }]
}

fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Group => "group",
        Kind::Hint => "hint",
        Kind::Fact => "fact",
        Kind::Lock => "lock",
        Kind::Check => "check",
        Kind::Choice => "choice",
        Kind::Action => "action",
        Kind::Favourite => "favourite",
        Kind::Ruler => "ruler",
        Kind::KeyPreview => "keypreview",
        Kind::Hero => "hero",
    }
}

fn labels(rows: &[Row]) -> Vec<String> {
    rows.iter().map(|row| format!("{}:{}", kind_word(row.kind), say(&row.label))).collect()
}

// The rows between the Updates heading and This Computer, as kind:label.
fn updates_group(rows: &[Row]) -> String {
    let labels = labels(rows);
    let from = labels.iter().position(|label| label == "group:Updates").map_or(0, |at| at + 1);
    let to = labels.iter().position(|label| label == "group:This Computer").unwrap_or(labels.len());
    labels[from..to].join(",")
}

fn find<'a>(rows: &'a [Row], id: &str) -> &'a Row {
    rows.iter().find(|row| row.id == id).expect("row present")
}

fn hint_with_prefix<'a>(rows: &'a [Row], prefix: &str) -> &'a Row {
    rows.iter().find(|row| row.kind == Kind::Hint && say(&row.label).starts_with(prefix)).expect("hint present")
}

#[test]
fn updates_group_is_update_then_switch_while_idle() {
    assert_eq!(updates_group(&about(update::idle())), "action:Update Flea,check:Check automatically");
}

#[test]
fn no_static_update_owner_fact() {
    assert!(about(update::idle()).iter().all(|row| say(&row.label) != "Update owner"));
}

#[test]
fn update_row_wears_the_download_mark() {
    assert_eq!(find(&about(update::idle()), "updateFlea").glyph, "download");
}

#[test]
fn idle_row_offers_the_check_in_the_foreground_with_a_chevron() {
    let rows = about(update::idle());
    let row = find(&rows, "updateFlea");
    assert_eq!(format!("{}|{:?}|{}", say(&row.value), row.role, row.inert), "Check|Live|false");
    assert!(focusable(row));
}

#[test]
fn an_update_reads_in_the_accent() {
    let rows = about(answered_status("available opr 0.3.3-1 0.3.4-1"));
    let row = find(&rows, "updateFlea");
    assert_eq!(format!("{}|{:?}", say(&row.value), row.role), "0.3.4 available|Accent");
}

#[test]
fn a_running_check_is_muted_inert_and_still_a_stop() {
    let rows = about(update::checking(&update::idle()));
    let row = find(&rows, "updateFlea");
    assert_eq!(format!("{}|{:?}|{}|{}", say(&row.value), row.role, row.inert, focusable(row)), "Checking|None|true|true");
}

#[test]
fn an_unchecked_build_is_a_local_fact_the_cursor_steps_over() {
    let rows = about(answered_status("unchecked git - -"));
    let row = find(&rows, "updateFlea");
    assert_eq!(format!("{:?}|{}|{}", row.kind, say(&row.value), focusable(row)), "Fact|Built from source|false");
}

#[test]
fn after_a_launch_the_group_ends_on_the_restart_note() {
    let launched = update::launched_from(&answered_status("available opr 0.3.3-1 0.3.4-1"), true);
    assert_eq!(
        updates_group(&about(launched)),
        "action:Update Flea,check:Check automatically,hint:Restart Flea when the update finishes"
    );
}

#[test]
fn the_restart_note_is_in_the_accent_and_takes_no_cursor() {
    let launched = update::launched_from(&answered_status("available opr 0.3.3-1 0.3.4-1"), true);
    let rows = about(launched);
    let note = hint_with_prefix(&rows, "Restart");
    assert_eq!(format!("{:?}|{}", note.role, focusable(note)), "Accent|false");
}

#[test]
fn offline_explains_itself_in_the_foreground() {
    let rows = about(answered_status("failed opr 0.3.3-1 -"));
    let note = hint_with_prefix(&rows, "Offline");
    assert_eq!(format!("{}|{:?}", say(&note.label), note.role), "Offline: Omarchy update still opens|Foreground");
}

#[test]
fn the_switch_carries_no_mark_and_names_both_triggers_in_its_caption() {
    let rows = about(update::idle());
    let row = find(&rows, "updates.autoCheck");
    assert_eq!(format!("{}|{}", row.glyph, say(&row.caption)), "|when About opens, every 6 h");
}

#[test]
fn the_switch_ships_on() {
    assert!(find(&about(update::idle()), "updates.autoCheck").on);
}

#[test]
fn a_stored_false_reads_as_off() {
    assert!(!auto_check(Some(false)));
    assert!(auto_check(None));
    assert!(auto_check(Some(true)));
    let rows = rows(&Facts::default(), auto_check(Some(false)), "English", Vec::new());
    assert!(!find(&rows, "updates.autoCheck").on);
}

#[test]
fn the_switch_is_a_check_the_cursor_stops_on() {
    let rows = about(update::idle());
    let row = find(&rows, "updates.autoCheck");
    assert!(focusable(row));
    assert_eq!(row.kind, Kind::Check);
}

#[test]
fn the_two_web_rows_carry_their_own_destinations() {
    let rows = about(update::idle());
    assert_eq!(find(&rows, "reportIssue").url, "https://github.com/thisisgm/flea/issues");
    assert_eq!(find(&rows, "support").url, "https://github.com/sponsors/thisisgm");
}

#[test]
fn no_other_row_has_a_destination() {
    assert_eq!(about(update::idle()).iter().filter(|row| !row.url.is_empty()).count(), 2);
}

#[test]
fn the_language_row_is_an_action_with_the_globe() {
    let rows = about(update::idle());
    let row = find(&rows, "language");
    assert_eq!(format!("{:?}|{}|{}", row.kind, row.glyph, say(&row.value)), "Action|globe|English");
}

#[test]
fn the_pane_lists_its_rows_in_order() {
    let rows = rows(&Facts::default(), true, "English", stub_make_default());
    assert_eq!(
        labels(&rows),
        [
            "hero:Flea",
            "fact:Version",
            "fact:Built",
            "fact:Installed from",
            "fact:Package",
            "fact:License",
            "group:Language",
            "action:Language",
            "group:Updates",
            "action:Update Flea",
            "check:Check automatically",
            "group:This Computer",
            "fact:File manager",
            "check:stub",
            "action:Keyboard sheet",
            "action:Report an issue",
            "action:Support Flea",
        ]
    );
}

#[test]
fn an_empty_facts_read_as_not_reported() {
    let rows = rows(&Facts::default(), true, "English", Vec::new());
    let shown = ["settings.about.version", "settings.about.built", "settings.about.installed_from", "settings.about.package"];
    let values: Vec<String> = shown
        .iter()
        .map(|key| say(&find_by_label(&rows, key).value))
        .collect();
    assert_eq!(values, vec!["Not reported", "Not recorded in this build", "Not reported", "Not reported"]);
    assert_eq!(say(&find_by_label(&rows, "settings.about.file_manager").value), "Not reported");
}

fn find_by_label<'a>(rows: &'a [Row], key: &str) -> &'a Row {
    rows.iter().find(|row| row.label.key() == Some(key)).expect("row present")
}

#[test]
fn an_unowned_build_reads_not_owned_and_not_from_a_package() {
    let facts = Facts {
        package: Some(String::new()),
        source: installed_from("", false),
        ..Facts::default()
    };
    let rows = rows(&facts, true, "English", Vec::new());
    assert_eq!(say(&find_by_label(&rows, "settings.about.package").value), "Not owned by a package");
    assert_eq!(say(&find_by_label(&rows, "settings.about.installed_from").value), "Not from a package");
}

#[test]
fn the_file_manager_row_elides_its_head_and_takes_the_handler() {
    let facts = Facts { handler: "kitty-open.desktop".to_owned(), ..Facts::default() };
    let rows = rows(&facts, true, "English", Vec::new());
    let row = find_by_label(&rows, "settings.about.file_manager");
    assert_eq!(format!("{}|{}|{}", row.glyph, row.elide_head, say(&row.value)), "folder|true|kitty-open.desktop");
}

#[test]
fn the_local_query_reads_as_its_package_build_and_no_signature() {
    assert_eq!(
        package_facts(LOCAL_QI),
        PackageFacts { package: "flea 0.3.4-1".to_owned(), built: "Wed Sep 23 12:40:50 2026".to_owned(), signed: false }
    );
}

#[test]
fn a_former_binary_package_is_local() {
    assert_eq!(say(&installed_from("flea-bin", false)), "Local package");
}

#[test]
fn the_kind_the_update_row_reads_is_the_same_one() {
    let answer = answered_status("unchecked local 0.3.4-1 -");
    assert_eq!(format!("{}|{}", answer.kind, install_kind("flea-bin", false)), "local|local");
}

#[test]
fn the_signed_opr_flea_keeps_the_board_words() {
    assert_eq!(say(&installed_from("flea", true)), "Omarchy Package Repository");
}

#[test]
fn the_same_name_unsigned_is_a_local_build() {
    assert_eq!(say(&installed_from("flea", false)), "Local package");
}

#[test]
fn a_former_git_package_is_local_even_when_signed() {
    assert_eq!(say(&installed_from("flea-git", false)), "Local package");
    assert_eq!(say(&installed_from("flea-git", true)), "Local package");
    assert_eq!(say(&installed_from("flea-bin", true)), "Local package");
}

#[test]
fn any_other_owner_is_somebody_elses_package() {
    assert_eq!(say(&installed_from("flea-custom", true)), "Local package");
}

#[test]
fn no_owner_is_not_from_a_package() {
    assert_eq!(say(&installed_from("", false)), "Not from a package");
}

#[test]
fn validated_by_is_read_as_src_update_rs_reads_it() {
    let read = |text: &str| package_facts(text).signed;
    assert!(read("Validated By    : SHA-256 Sum  Signature\n"));
    assert!(!read("Validated By    : SHA-256 Sum\n"));
    assert!(!read("Description     : Signature\nValidated By    : None\n"));
}

#[test]
fn a_failed_query_is_an_empty_answer() {
    assert_eq!(package_facts(""), PackageFacts::default());
}

// The text of a Rust source file in this tree, read at compile time.
fn const_str<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("const {name}: &str = \"");
    let start = src.find(&marker)? + marker.len();
    let end = src[start..].find('"')?;
    Some(&src[start..start + end])
}

#[test]
fn the_opr_package_name_is_src_update_rs_own() {
    assert_eq!(Some(OPR_PACKAGE), const_str(include_str!("../update.rs"), "OPR_PACKAGE"));
}

#[test]
fn one_sentence_for_each_kind_word_and_no_other() {
    let src = include_str!("../update.rs");
    let mut words: Vec<&str> = src
        .lines()
        .filter(|line| line.contains("Kind::") && line.contains("=> \""))
        .filter_map(|line| line.split('"').nth(1))
        .collect();
    words.sort_unstable();
    let mut keys: Vec<&str> = SOURCES.iter().map(|(word, _)| *word).collect();
    keys.sort_unstable();
    assert_eq!(keys, words);
}

// tests/js/settings.js: supplied facts read back as given, the handler with its head elided, and the support route takes focus.
#[test]
fn supplied_facts_read_back_and_the_support_route_takes_focus() {
    let facts = Facts { version: "0.1.6".to_owned(), handler: "com.thisisgm.flea.desktop".to_owned(), ..Facts::default() };
    let rows = rows(&facts, true, "English", Vec::new());
    assert_eq!(say(&find_by_label(&rows, "settings.about.version").value), "0.1.6");
    let handler = find_by_label(&rows, "settings.about.file_manager");
    assert_eq!(format!("{}|{}", say(&handler.value), handler.elide_head), "com.thisisgm.flea.desktop|true");
    assert!(!focusable(find_by_label(&rows, "settings.about.version")));
    assert!(focusable(find(&rows, "support")));
}
