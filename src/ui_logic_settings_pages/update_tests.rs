use super::*;
use crate::ui_logic_settings_pages::rows::Role;
use crate::ui_logic_settings_pages::test_support::say;

const HOUR: u64 = 60 * 60 * 1000;
const AVAILABLE: &str = "available opr 0.3.3-1 0.3.4-1\n";
const CURRENT: &str = "current opr 0.3.3-1 -";
const OFFLINE: &str = "failed opr 0.3.3-1 -";
const OTHER: &str = "unchecked git - -";
const SOURCE: &str = "unchecked unowned - -";

fn after(line: &str) -> Status {
    answered(&checking(&idle()), line, 1000)
}

fn role_word(role: Role) -> &'static str {
    match role {
        Role::None => "",
        Role::Live => "live",
        Role::Accent => "accent",
        Role::Foreground => "foreground",
        Role::Error => "error",
    }
}

fn drawn(status: &Status) -> String {
    format!(
        "{}|{}|{}",
        say(&value(status)),
        role_word(role(status)),
        acts(status)
    )
}

fn noted(status: &Status) -> String {
    match note(status) {
        None => "none".to_owned(),
        Some((text, role)) => format!("{}|{}", say(&text), role_word(role)),
    }
}

fn next(status: &Status) -> &'static str {
    let word = on_activate(status);
    if word.is_empty() {
        "nothing"
    } else {
        word
    }
}

#[test]
fn idle_offers_the_check() {
    assert_eq!(drawn(&idle()), "Check|live|true");
    assert_eq!(value(&idle()).key(), Some("settings.update.check"));
}

#[test]
fn checking_is_muted_with_no_chevron() {
    assert_eq!(drawn(&checking(&idle())), "Checking||false");
}

#[test]
fn current_names_the_installed_release() {
    assert_eq!(drawn(&after(CURRENT)), "Up to date · 0.3.3|live|true");
}

#[test]
fn available_takes_the_accent() {
    assert_eq!(drawn(&after(AVAILABLE)), "0.3.4 available|accent|true");
}

#[test]
fn launched_has_no_chevron() {
    assert_eq!(
        drawn(&launched_from(&after(AVAILABLE), true)),
        "Updating in terminal|live|false"
    );
}

#[test]
fn offline_still_acts() {
    assert_eq!(drawn(&after(OFFLINE)), "Could not check|live|true");
}

#[test]
fn an_unknown_unchecked_kind_reads_as_built_from_source() {
    assert_eq!(drawn(&after(OTHER)), "Built from source|live|false");
}

#[test]
fn checkout_build_is_built_from_source() {
    assert_eq!(drawn(&after(SOURCE)), "Built from source|live|false");
}

#[test]
fn unowned_binary_is_built_from_source() {
    assert_eq!(
        say(&value(&after("unchecked unowned - -"))),
        "Built from source"
    );
}

// S1A-1: flea-bin is the prebuilt release, which Omarchy's updater keeps current, never a checkout to rebuild.
#[test]
fn a_release_package_is_a_release_build_that_omarchy_update_keeps_current() {
    let release = after("unchecked release 0.4.1-1 -");
    assert_eq!(drawn(&release), "Release build|live|false");
    assert_eq!(
        noted(&release),
        "Omarchy update installs newer releases.|foreground"
    );
    assert!(is_fact(&release), "nothing here checks or launches for it");
}

#[test]
fn only_unchecked_kinds_are_facts() {
    let list = [
        idle(),
        after(CURRENT),
        after(OTHER),
        after(SOURCE),
        after("unchecked unowned - -"),
    ]
    .iter()
    .map(|status| format!("{}/{}", is_fact(status), acts(status)))
    .collect::<Vec<_>>()
    .join(",");
    assert_eq!(
        list,
        "false/true,false/true,true/false,true/false,true/false"
    );
}

#[test]
fn rebuild_names_the_package_release() {
    assert_eq!(
        say(&value(&after("available opr 0.3.3-1 0.3.3-2"))),
        "0.3.3-2 available"
    );
}

#[test]
fn unknown_line_is_a_failed_check() {
    assert_eq!(
        say(&value(&after("flea: unknown flag --update"))),
        "Could not check"
    );
}

#[test]
fn no_line_at_all_is_a_failed_check() {
    assert_eq!(say(&value(&after(""))), "Could not check");
}

#[test]
fn a_dash_is_an_empty_field() {
    assert!(after(CURRENT).latest.is_empty());
}

#[test]
fn the_answer_records_when_it_came() {
    assert_eq!(after(CURRENT).checked_at, 1000);
}

#[test]
fn idle_and_current_check() {
    assert_eq!(
        format!("{}|{}", next(&idle()), next(&after(CURRENT))),
        "check|check"
    );
}

#[test]
fn available_and_offline_open_the_updater() {
    assert_eq!(
        format!("{}|{}", next(&after(AVAILABLE)), next(&after(OFFLINE))),
        "launch|launch"
    );
}

#[test]
fn enter_does_nothing_while_busy_or_on_a_fact() {
    let list = [
        checking(&idle()),
        launched_from(&after(AVAILABLE), true),
        after(OTHER),
        after(SOURCE),
    ]
    .iter()
    .map(next)
    .collect::<Vec<_>>()
    .join(",");
    assert_eq!(list, "nothing,nothing,nothing,nothing");
}

#[test]
fn a_launch_that_failed_leaves_the_row_as_it_was() {
    assert_eq!(
        say(&value(&launched_from(&after(AVAILABLE), false))),
        "0.3.4 available"
    );
}

#[test]
fn a_launch_from_a_failed_check_is_final() {
    let launched = launched_from(&after(OFFLINE), true);
    assert_eq!(
        format!(
            "{}|{}",
            say(&value(&launched)),
            due(&launched, 7 * HOUR, true)
        ),
        "Updating in terminal|false"
    );
}

#[test]
fn an_answer_after_a_launch_leaves_the_launch_standing() {
    let launched = launched_from(&after(AVAILABLE), true);
    assert_eq!(
        say(&value(&answered(&launched, CURRENT, 2000))),
        "Updating in terminal"
    );
}

#[test]
fn the_footer_says_what_a_launch_did() {
    let (text, is_error) = launch_sentence(true);
    assert_eq!(
        format!("{}|{}", say(&text), is_error),
        "Opening Omarchy update · restart Flea when it finishes|false"
    );
}

#[test]
fn the_footer_says_a_refused_launch_as_an_error() {
    let (text, is_error) = launch_sentence(false);
    assert_eq!(
        format!("{}|{}", say(&text), is_error),
        "Omarchy's updater could not be started.|true"
    );
}

#[test]
fn the_note_asks_for_a_restart_in_the_accent_after_a_launch() {
    assert_eq!(
        noted(&launched_from(&after(AVAILABLE), true)),
        "Restart Flea when the update finishes|accent"
    );
}

#[test]
fn the_note_explains_offline_and_all_unchecked_builds_in_the_foreground() {
    let list = [OFFLINE, OTHER, SOURCE]
        .iter()
        .map(|line| noted(&after(line)))
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(
        list,
        "Offline: Omarchy update still opens|foreground, Rebuild from the checkout to update.|foreground, \
         Rebuild from the checkout to update.|foreground"
    );
}

#[test]
fn the_note_is_absent_everywhere_else() {
    let list = [idle(), checking(&idle()), after(CURRENT), after(AVAILABLE)]
        .iter()
        .map(noted)
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(list, "none,none,none,none");
}

#[test]
fn the_poll_runs_every_six_hours() {
    assert_eq!(INTERVAL_MS, 6 * HOUR);
}

#[test]
fn the_first_background_look_waits_a_minute() {
    assert_eq!(FIRST_CHECK_MS, 60 * 1000);
    assert_eq!(FIRST_CHECK_MS.cmp(&INTERVAL_MS), std::cmp::Ordering::Less);
}

#[test]
fn about_asks_when_nothing_has_been_asked_yet() {
    assert!(due(&idle(), 5 * HOUR, true));
}

#[test]
fn never_asks_with_the_switch_off() {
    assert!(!due(&idle(), 5 * HOUR, false));
}

#[test]
fn never_asks_while_a_check_is_running() {
    assert!(!due(&checking(&idle()), 5 * HOUR, true));
}

#[test]
fn never_asks_once_an_update_is_launched() {
    assert!(!due(
        &launched_from(&after(AVAILABLE), true),
        7 * HOUR,
        true
    ));
}

#[test]
fn an_answer_inside_the_period_stands() {
    assert!(!due(&after(CURRENT), 1000 + 6 * HOUR - 1, true));
}

#[test]
fn an_answer_a_whole_period_old_is_asked_again() {
    assert!(due(&after(CURRENT), 1000 + 6 * HOUR, true));
}

#[test]
fn an_available_or_unchecked_answer_inside_the_period_stands_too() {
    let list = [after(AVAILABLE), after(OTHER)]
        .iter()
        .map(|s| due(s, 1000 + 6 * HOUR - 1, true))
        .collect::<Vec<_>>();
    assert_eq!(list, vec![false, false]);
}

#[test]
fn an_available_or_unchecked_answer_is_asked_again_once_a_period_old() {
    let list = [after(AVAILABLE), after(OTHER)]
        .iter()
        .map(|s| due(s, 1000 + 6 * HOUR, true))
        .collect::<Vec<_>>();
    assert_eq!(list, vec![true, true]);
}

#[test]
fn a_failed_check_never_stands() {
    assert!(due(&after(OFFLINE), 2000, true));
}

#[test]
fn the_menu_row_exists_only_while_a_newer_build_is_known() {
    let list = [
        idle(),
        checking(&idle()),
        after(CURRENT),
        after(OFFLINE),
        after(OTHER),
        after(SOURCE),
        launched_from(&after(AVAILABLE), true),
    ]
    .iter()
    .map(menu_version)
    .collect::<Vec<_>>();
    assert_eq!(list, vec![String::new(); 7]);
}

#[test]
fn the_menu_hint_is_that_version() {
    assert_eq!(menu_version(&after(AVAILABLE)), "0.3.4");
}

#[test]
fn the_menu_hint_is_the_package_release_when_only_that_moved() {
    assert_eq!(
        menu_version(&after("available opr 0.3.3-1 0.3.3-2")),
        "0.3.3-2"
    );
}

#[test]
fn the_first_beat_asks_only_when_due_and_each_later_beat_asks_outright() {
    let fresh = answered(&idle(), CURRENT, 10 * HOUR);
    assert!(!poll_asks(&fresh, 10 * HOUR + 60_000, true, true));
    assert!(poll_asks(&fresh, 10 * HOUR + 60_000, true, false));
    assert!(poll_asks(&idle(), 60_000, true, true));
    assert!(!poll_asks(&idle(), 60_000, false, true));
    assert!(!poll_asks(&fresh, 20 * HOUR, false, false));
    assert!(!poll_asks(&checking(&fresh), 20 * HOUR, true, false));
    assert_eq!((poll_wait(true), poll_wait(false)), (60_000, 6 * HOUR));
}
