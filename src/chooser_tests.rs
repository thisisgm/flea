use super::*;
use crate::backend::testdir::TestDir;
use std::fs;

const BOX_SHAPE: &str = "[preferred]\ndefault=hyprland;gtk\n";

#[test]
fn set_preferred_adds_the_key_without_touching_the_default() {
    let out = set_preferred(BOX_SHAPE, IFACE, PREFERRED).expect("the file gains a line");
    assert_eq!(out, "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n");
}

#[test]
fn set_preferred_creates_the_group_when_the_file_has_another_one() {
    let out = set_preferred("[something]\nkey=value\n", IFACE, PREFERRED).expect("the file gains a group");
    assert_eq!(out, "[something]\nkey=value\n[preferred]\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n");
}

#[test]
fn set_preferred_replaces_another_backend_and_answers_none_for_its_own() {
    let held = "[preferred]\norg.freedesktop.impl.portal.FileChooser=gtk\ndefault=hyprland\n";
    let out = set_preferred(held, IFACE, PREFERRED).expect("the value changes");
    assert_eq!(
        out,
        "[preferred]\n# flea replaced: org.freedesktop.impl.portal.FileChooser=gtk\norg.freedesktop.impl.portal.FileChooser=flea;gtk\ndefault=hyprland\n"
    );
    assert_eq!(set_preferred(&out, IFACE, PREFERRED), None);
}

// The reported box: a desktop file routing the chooser to Nautilus, which the claim now edits in place.
const NAUTILUS: &str = "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=gnome;gtk\n";

#[test]
fn a_claim_and_its_undo_put_the_replaced_backend_back_byte_for_byte() {
    let claimed = set_preferred(NAUTILUS, IFACE, PREFERRED).expect("the value changes");
    assert_eq!(preferred_value(&claimed, IFACE).as_deref(), Some(PREFERRED));
    assert_eq!(drop_preferred(&claimed, IFACE).as_deref(), Some(NAUTILUS));
}

#[test]
fn a_second_claim_after_a_hand_edit_keeps_one_note_naming_the_latest_backend() {
    let claimed = set_preferred(NAUTILUS, IFACE, PREFERRED).expect("claimed");
    let edited = claimed.replace("FileChooser=flea;gtk", "FileChooser=kde");
    let again = set_preferred(&edited, IFACE, PREFERRED).expect("claimed again");
    assert_eq!(again.matches(REPLACED).count(), 1, "one note, not two: {:?}", again);
    assert!(again.contains("# flea replaced: org.freedesktop.impl.portal.FileChooser=kde\n"));
    assert_eq!(drop_preferred(&again, IFACE).as_deref(), Some(edited.replace("# flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk\n", "").as_str()));
}

#[test]
fn an_undo_leaves_a_backend_the_user_chose_after_the_claim_alone() {
    let claimed = set_preferred(NAUTILUS, IFACE, PREFERRED).expect("claimed");
    let edited = claimed.replace("FileChooser=flea;gtk", "FileChooser=kde");
    assert_eq!(drop_preferred(&edited, IFACE), None);
}

#[test]
fn a_bare_flea_value_is_flea_s_own_and_never_saved_as_the_one_to_restore() {
    let bare = "[preferred]\norg.freedesktop.impl.portal.FileChooser=flea\n";
    let out = set_preferred(bare, IFACE, PREFERRED).expect("the value changes");
    assert!(!out.contains(REPLACED), "{:?}", out);
    assert_eq!(drop_preferred(&out, IFACE).as_deref(), Some("[preferred]\n"));
}

#[test]
fn set_preferred_puts_the_key_inside_the_group_and_not_after_the_next_one() {
    let two = "[preferred]\ndefault=hyprland\n\n[other]\nkey=value\n";
    let out = set_preferred(two, IFACE, PREFERRED).expect("the file gains a line");
    assert_eq!(out, "[preferred]\ndefault=hyprland\n\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n[other]\nkey=value\n");
}

#[test]
fn drop_preferred_removes_only_fleas_line() {
    let held = "[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n";
    assert_eq!(drop_preferred(held, IFACE), Some("[preferred]\ndefault=hyprland;gtk\n".to_string()));
    assert_eq!(drop_preferred(BOX_SHAPE, IFACE), None);
    assert_eq!(drop_preferred("", IFACE), None);
}

#[test]
fn a_terminal_run_restarts_the_portal_and_a_piped_one_leaves_it_to_its_caller() {
    let calls = std::cell::Cell::new(0);
    let restart = |ok: bool| {
        let calls = &calls;
        move || {
            calls.set(calls.get() + 1);
            ok
        }
    };
    assert_eq!(follow_line(false, restart(true)), AT_STARTUP, "a piped run is told how to restart");
    assert_eq!(calls.get(), 0, "a piped run never restarts the portal");
    assert_eq!(follow_line(true, restart(true)), RESTARTED);
    assert_eq!(follow_line(true, restart(false)), AT_STARTUP, "a refused restart falls back to the command");
    assert_eq!(calls.get(), 2, "a terminal run restarts exactly once");
}

#[test]
fn drop_preferred_leaves_the_same_key_in_another_group_alone() {
    let elsewhere = "[other]\norg.freedesktop.impl.portal.FileChooser=gtk\n";
    assert_eq!(drop_preferred(elsewhere, IFACE), None);
}

#[test]
fn a_flea_spelled_value_keeps_the_note_naming_the_backend_before_it() {
    let noted = "[preferred]\n# flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk\norg.freedesktop.impl.portal.FileChooser=flea\n";
    let out = set_preferred(noted, IFACE, PREFERRED).expect("the value changes");
    assert!(out.contains("# flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk\n"), "{:?}", out);
    assert_eq!(drop_preferred(&out, IFACE).as_deref(), Some("[preferred]\norg.freedesktop.impl.portal.FileChooser=gnome;gtk\n"));
}

#[test]
fn an_undo_never_deletes_a_desktop_file_it_leaves_empty_but_does_delete_its_own() {
    let d = TestDir::new("chooser-release");
    let only_flea = "[preferred]\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n";
    let desktop = d.file("hyprland-portals.conf", only_flea);
    let line = release_file(&desktop, false).expect("released").expect("a line");
    assert!(line.contains("Flea's line removed"), "{}", line);
    assert_eq!(fs::read_to_string(&desktop).expect("the user's file stays"), "[preferred]\n");
    let plain = d.file("portals.conf", only_flea);
    release_file(&plain, true).expect("released").expect("a line");
    assert!(!plain.exists(), "Flea's own portals.conf goes when its line was all it held");
}

#[test]
fn a_note_left_without_its_line_is_dropped_so_an_undo_restores_nothing_it_names() {
    let stale = "[preferred]\n# flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk\ndefault=hyprland;gtk\n";
    let out = set_preferred(stale, IFACE, PREFERRED).expect("the file gains a line");
    assert!(!out.contains(REPLACED), "{:?}", out);
    assert_eq!(drop_preferred(&out, IFACE).as_deref(), Some("[preferred]\ndefault=hyprland;gtk\n"));
}

#[test]
fn a_portal_file_that_cannot_be_read_says_why_in_words() {
    let dir = TestDir::new("chooserunreadable");
    // A folder where the file belongs: reading it is refused with a reason a person can act on.
    let folder = dir.dir("portals.conf");
    let expected = format!("{} could not be read: item is a folder", folder.display());
    assert_eq!(claim_chooser((folder.clone(), false)).unwrap_err(), expected);
    assert_eq!(release_file(&folder, true).unwrap_err(), expected);
}

// The rerun it asks for is the command the user ran: --picker alone must not be told to make Flea the default for everything.
#[test]
fn an_unknown_desktop_asks_for_the_command_the_user_ran() {
    let found = vec![PathBuf::from("/h/hyprland-portals.conf"), PathBuf::from("/h/sway-portals.conf")];
    for command in ["flea --picker", "flea --default"] {
        let asked = unknown_desktop(&found, command);
        assert!(asked.contains("reads /h/hyprland-portals.conf or /h/sway-portals.conf. "), "{}", asked);
        assert!(asked.ends_with(&format!("Run {} inside the desktop session.", command)), "{}", asked);
    }
    let claims = include_str!("claims.rs");
    assert_eq!(claims.matches("chooser::claim(\"flea --picker\")").count(), 1, "the picker route names itself");
    assert_eq!(claims.matches("chooser::claim(\"flea --default\")").count(), 1, "the default route names itself");
}
