// tests/js/bulkrename.js, each line's reason checked against a forward-scan oracle.
use super::bulkrename::*;
use super::english;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

// The forward scan the backward one must agree with: a repeat names the first later line holding the name.
fn oracle(name: &str, index: usize, all: &[String]) -> Option<Reason> {
    if name.is_empty() {
        return Some(Reason::Empty);
    }
    if name.contains('/') {
        return Some(Reason::Slash);
    }
    if name == "." || name == ".." || name.contains('\0') || name.contains('\r') || name.contains('\n') {
        return Some(Reason::Invalid);
    }
    if name.len() > 255 {
        return Some(Reason::TooLong);
    }
    (index + 1..all.len()).find(|j| all[*j] == name).map(Reason::Same)
}

fn check_case(label: &str, old: &[&str], new: &[&str]) {
    let (old, new) = (names(old), names(new));
    let want: Vec<Option<Reason>> = new.iter().enumerate().map(|(i, n)| oracle(n, i, &new)).collect();
    let got = reasons(&new);
    assert_eq!(got, want, "{label} reasons match the forward scan");
    let mut counted = Counts::default();
    for (i, n) in new.iter().enumerate() {
        if want[i].is_some() { counted.invalid += 1 } else if *n == old[i] { counted.unchanged += 1 } else { counted.renamed += 1 }
    }
    assert_eq!(counts(&old, &new, &got), counted, "{label} counts triple matches");
}

fn text(list: &[&str]) -> Vec<String> {
    let i18n = english();
    reasons(&names(list)).into_iter().map(|r| r.map_or_else(String::new, |r| reason_text(&i18n, r))).collect()
}

#[test]
fn find_and_replace_and_the_board_reasons() {
    assert_eq!(apply_find_replace(&names(&["IMG_1.HEIC", "notes.txt"]), "IMG_", "trip-"), names(&["trip-1.HEIC", "notes.txt"]));
    assert_eq!(apply_find_replace(&names(&["a.txt"]), "", "x"), names(&["a.txt"]), "empty find leaves names alone");
    assert_eq!(text(&["trip-4121.MOV", "trip-4121.MOV"])[0], "same as line 2");
    assert_eq!(text(&["a", "b", "c", "trip/4123.HEIC"])[3], "contains /");
    assert_eq!(text(&["", "b"])[0], "empty");
    assert_eq!(text(&["IMG_4120.HEIC", "x"])[0], "", "an unchanged name carries no reason");
}

#[test]
fn the_footer_title_and_default_read_as_the_board_draws_them() {
    let i18n = english();
    assert_eq!(footer(&i18n, Counts { renamed: 5, unchanged: 1, invalid: 0 }), "5 renamed \u{b7} 1 unchanged");
    assert_eq!(footer(&i18n, Counts { renamed: 0, unchanged: 1, invalid: 1 }), "0 renamed \u{b7} 1 unchanged", "the summary drops the invalid count");
    assert_eq!(title(&i18n, 6), "Rename 6 Items");
    assert_eq!(title(&i18n, 2), "Rename 2 Items");
    assert_eq!(rename_label(&i18n, 0), "Rename", "the default reads Rename until a name differs");
    assert_eq!(rename_label(&i18n, 1204), "Rename 1,204");
    let clash = reasons(&names(&["gamma.txt", "gamma.txt"]));
    assert_eq!(clash_line(&i18n, &clash), "Give each item a different name.");
    assert_eq!(clash_line(&i18n, &reasons(&names(&["a/b", "c"]))), "", "the error line stays empty until two names clash");
    assert!(!can_rename(counts(&names(&["alpha.txt", "gamma.txt"]), &names(&["gamma.txt", "gamma.txt"]), &clash)));
}

#[test]
fn counts_split_renamed_unchanged_and_invalid() {
    let new = names(&["a", "b2", "b2"]);
    assert_eq!(counts(&names(&["a", "b", "c"]), &new, &reasons(&new)), Counts { renamed: 1, unchanged: 1, invalid: 1 });
    assert_eq!(format!("{}|{}", name_at(&names(&["a"]), 0), name_at(&names(&["a"]), 1)), "a|");
}

#[test]
fn the_tab_walk_comes_round_both_ways() {
    assert_eq!(focus_stops(2), vec![Stop::Find, Stop::Replace, Stop::Line(0), Stop::Line(1), Stop::Edit, Stop::Cancel, Stop::Rename]);
    assert_eq!(next_stop(Some(Stop::Replace), false, 2), Stop::Line(0));
    assert_eq!(next_stop(Some(Stop::Line(1)), false, 2), Stop::Edit);
    assert_eq!(next_stop(Some(Stop::Line(0)), true, 2), Stop::Replace);
    assert_eq!(next_stop(Some(Stop::Rename), false, 2), Stop::Find);
    assert_eq!(next_stop(None, true, 2), Stop::Rename);
    assert_eq!(next_stop(Some(Stop::Line(9)), false, 2), Stop::Find, "a stop past the lines enters at the start");
}

#[test]
fn every_invalid_shape_agrees_with_the_forward_scan() {
    check_case("two invalid lines", &["a", "b", "c", "d"], &["x", "x", "y/z", "d"]);
    check_case("three-way duplicates", &["o1", "o2", "o3"], &["a", "a", "a"]);
    check_case("a swap names no duplicate", &["a.txt", "b.txt"], &["b.txt", "a.txt"]);
    check_case("empty names", &["a", "b"], &["", "b"]);
    check_case("slash names", &["a", "b"], &["a/b", "b"]);
    check_case("dot names", &["a", "b"], &[".", "b"]);
    check_case("dotdot names", &["a", "b"], &["..", "b"]);
    check_case("nul names", &["a", "b"], &["a\0b", "b"]);
    check_case("cr names", &["a", "b"], &["a\rb", "b"]);
    check_case("lf names", &["a", "b"], &["a\nb", "b"]);
    let long = "x".repeat(300);
    let wide = "\u{4e2d}".repeat(86);
    let fits = "\u{4e2d}".repeat(85);
    check_case("long names", &["a", "b", "c"], &[&long, &wide, &fits]);
}

#[test]
fn the_byte_limit_counts_utf8_not_characters() {
    assert_eq!(text(&[&"a".repeat(256), "b"])[0], "too long");
    assert_eq!(text(&[&"a".repeat(255), "b"])[0], "");
    assert_eq!(text(&[&"\u{e9}".repeat(128), &format!("{}a", "\u{e9}".repeat(127))]).join("|"), "too long|");
    assert_eq!(text(&[&"\u{1f600}".repeat(64), &format!("{}abc", "\u{1f600}".repeat(63))]).join("|"), "too long|");
}

#[test]
fn two_thousand_seeded_names_match_the_forward_scan() {
    let mut seed: u64 = 12345;
    let mut next = move || {
        seed = (seed.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x7fff_ffff;
        seed as f64 / 0x7fff_ffff as f64
    };
    let pool = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "a/b", "", ".", ".."];
    let old: Vec<String> = (0..2000).map(|i| format!("old{i:04}.txt")).collect();
    let new: Vec<String> = (0..2000).map(|_| {
        let base = pool[(next() * pool.len() as f64) as usize % pool.len()];
        if next() < 0.3 { format!("{base}{}", next().round()) } else { base.to_string() }
    }).collect();
    let refs: Vec<&str> = new.iter().map(String::as_str).collect();
    let olds: Vec<&str> = old.iter().map(String::as_str).collect();
    check_case("seeded 2000", &olds, &refs);
    assert!(reasons(&new).iter().filter(|r| matches!(r, Some(Reason::Same(_)))).count() > 100, "the seed reaches the repeat path");
}

#[test]
fn the_edit_file_round_trips_and_refuses_a_changed_count() {
    let new = names(&["first.png", "second.png"]);
    assert_eq!(edit_text(&new), "first.png\nsecond.png\n");
    assert_eq!(reload("first.png\nsecond.png\n", 2), Reloaded::Names(new.clone()));
    assert_eq!(reload("first.png\nsecond.png", 2), Reloaded::Names(new), "a missing trailing newline is the same file");
    assert_eq!(reload("only.png\n", 2), Reloaded::CountChanged);
    assert_eq!(reload(&"x".repeat(2 * NAME_LINE_BYTES + 1), 2), Reloaded::TooLarge);
}

#[test]
fn the_scratch_dir_is_one_flea_rename_leaf_under_its_base() {
    assert!(is_scratch_dir("/tmp", "/tmp/flea-rename-a1B2c3"));
    assert!(!is_scratch_dir("/tmp", "/tmp/flea-rename-"), "a bare stem is not a scratch dir");
    assert!(!is_scratch_dir("/tmp", "/tmp/other/flea-rename-x"));
    assert!(!is_scratch_dir("/tmp", "/tmpx/flea-rename-abc"));
    assert!(!is_scratch_dir("/tmp", "/tmp/flea-rename-..x"));
    assert!(!is_scratch_dir("", "/flea-rename-abc"));
    assert!(!is_scratch_dir("/tmp", "/home/gm"), "a home directory is never a cleanup target");
}

#[test]
fn the_editor_argv_carries_the_file_as_an_argument() {
    let argv = editor_argv("/d", "nvim -u x", "/tmp/flea-rename-1/names");
    assert_eq!(argv, names(&["xdg-terminal-exec", "--dir=/d", "--", "/bin/sh", "-c", "exec nvim -u x \"$1\"", "flea-bulk-rename", "/tmp/flea-rename-1/names"]));
    assert_eq!(folder_of("/d/a.txt"), "/d");
    assert_eq!(folder_of("/a.txt"), "/");
    assert_eq!(request(&names(&["/d/a"]), &names(&["b"])).as_deref(), Some(r#"{"c":"bulkrename","paths":["/d/a"],"names":["b"]}"#));
    assert_eq!(request(&names(&["/d/a"]), &[]), None, "a mismatched batch sends nothing");
}
