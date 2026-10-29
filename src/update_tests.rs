use super::*;

// What `pacman -Qi flea` prints under LC_ALL=C for OPR's signed package, trimmed to the fields around the two read.
const OPR_INFO: &str = "Name            : flea
Version         : 0.3.2-1
Description     : Fast, keyboard-first file manager for Omarchy
Architecture    : x86_64
Install Reason  : Explicitly installed
Install Script  : No
Validated By    : Signature
";

// The same fields for a `makepkg -si` build, which pacman installed with nothing to validate.
const LOCAL_INFO: &str = "Name            : flea
Version         : 0.3.3-1
Validated By    : None
";

#[test]
fn the_owner_is_the_one_package_name_pacman_printed() {
    assert_eq!(owner_from("flea\n"), Some("flea".to_string()));
    assert_eq!(owner_from("flea-bin\n"), Some("flea-bin".to_string()));
    assert_eq!(owner_from(""), None, "no package owns the binary");
    assert_eq!(owner_from("flea\nflea-git\n"), None, "one path has one owner, so two names are not an answer");
    assert_eq!(owner_from("error: No package owns /tmp/flea\n"), None);
}

#[test]
fn package_info_reads_the_version_and_whether_a_signature_validated_it() {
    assert_eq!(package_info(OPR_INFO), (Some("0.3.2-1".to_string()), true));
    assert_eq!(package_info(LOCAL_INFO), (Some("0.3.3-1".to_string()), false));
    // Two methods share one field, separated by two spaces.
    assert_eq!(package_info("Validated By    : SHA-256 Sum  Signature\n").1, true);
    assert_eq!(package_info("Validated By    : SHA-256 Sum\n").1, false);
    // A description that happens to mention a signature is not the Validated By field.
    assert_eq!(package_info("Description     : Signature\nValidated By    : None\n").1, false);
    assert_eq!(package_info(""), (None, false));
}

#[test]
fn the_kind_follows_the_owner_and_only_a_signed_flea_is_opr() {
    assert_eq!(kind_for("flea", true), Kind::Opr);
    assert_eq!(kind_for("flea", false), Kind::Local, "a local makepkg build carries OPR's name and no signature");
    assert_eq!(kind_for("flea-bin", false), Kind::Local);
    assert_eq!(kind_for("flea-bin", true), Kind::Local);
    assert_eq!(kind_for("flea-git", false), Kind::Local);
    assert_eq!(kind_for("flea-git", true), Kind::Local);
    assert_eq!(kind_for("my-flea", true), Kind::Local, "a name Flea never shipped under has no known updater");
}

#[test]
fn checkupdates_names_the_new_version_of_the_package_asked_about_and_no_other() {
    let listed = "linux 6.16.8.arch1-1 -> 6.16.9.arch1-1\nflea-bin 0.3.2-1 -> 0.3.9-1\nflea 0.3.2-1 -> 0.3.3-1\n";
    assert_eq!(checkupdates_version(listed, "flea"), Some("0.3.3-1".to_string()));
    assert_eq!(checkupdates_version("linux 6.16.8.arch1-1 -> 6.16.9.arch1-1\n", "flea"), None);
    assert_eq!(checkupdates_version("flea-git 0.3.2-1 -> 0.3.3-1\n", "flea"), None, "a prefix is not the package");
    assert_eq!(checkupdates_version("flea 0.3.2-1 -> 0.3.3-1 [ignored]\n", "flea"), None, "an IgnorePkg line is no offer");
    assert_eq!(checkupdates_version("", "flea"), None);
}

#[test]
fn only_a_strict_pkgver_pkgrel_is_a_version() {
    for good in ["0.3.3-1", "10.20.30-12", "0.0.0-0"] {
        assert!(valid_version(good), "{} is a version", good);
    }
    let bad = ["0.3.3", "0.3-1", "0.3.3.1-1", "v0.3.3-1", "1:0.3.3-1", "0.3.3-1.1", "0.3.3-", "-1", "",
               "0..3-1", "0.3.3-1 ", " 0.3.3-1", "0.3.3-1\n", "0.3.1.r0.g6433131-2", "0.3.3-1;reboot", "0.3.3-1-1",
               "\u{0660}.3.3-1"];
    for version in bad {
        assert!(!valid_version(version), "{:?} must be refused", version);
    }
}

#[test]
fn vercmp_output_reads_as_an_ordering() {
    assert_eq!(vercmp_order("1\n"), Some(Ordering::Greater));
    assert_eq!(vercmp_order("0\n"), Some(Ordering::Equal));
    assert_eq!(vercmp_order("-1\n"), Some(Ordering::Less));
    assert_eq!(vercmp_order(""), None);
    assert_eq!(vercmp_order("usage: vercmp <ver1> <ver2>\n"), None);
}

#[test]
fn a_newer_offer_is_available_and_anything_else_is_current() {
    let installed = Some("0.3.2-1".to_string());
    let newer = decide(Kind::Opr, installed.clone(), "0.3.3-1".to_string(), Some(Ordering::Greater));
    assert_eq!(newer.state, State::Available);
    assert_eq!(newer.line(), "available opr 0.3.2-1 0.3.3-1");
    assert_eq!(newer.status(), AVAILABLE);
    let same = decide(Kind::Opr, installed.clone(), "0.3.2-1".to_string(), Some(Ordering::Equal));
    assert_eq!((same.line(), same.status()), ("current opr 0.3.2-1 0.3.2-1".to_string(), NOTHING_TO_INSTALL));
    let older = decide(Kind::Opr, installed.clone(), "0.3.1-1".to_string(), Some(Ordering::Less));
    assert_eq!(older.state, State::Current, "a mirror behind the installed build offers nothing");
    let unknown = decide(Kind::Opr, installed, "0.3.3-1".to_string(), None);
    assert_eq!((unknown.line(), unknown.status()), ("failed opr 0.3.2-1 0.3.3-1".to_string(), FAILED));
}

#[test]
fn a_source_that_could_not_be_asked_fails_and_one_with_nothing_newer_is_current() {
    let installed = Some("0.3.2-1".to_string());
    let down = from_source(Kind::Opr, installed.clone(), Err("the package mirrors could not be asked for a newer Flea"));
    assert_eq!((down.line(), down.status()), ("failed opr 0.3.2-1 -".to_string(), FAILED));
    let quiet = from_source(Kind::Opr, installed, Ok(None));
    assert_eq!((quiet.line(), quiet.status()), ("current opr 0.3.2-1 -".to_string(), NOTHING_TO_INSTALL));
    let unreadable = from_source(Kind::Opr, None, Ok(Some("0.3.3-1".to_string())));
    assert_eq!(unreadable.line(), "failed opr - -", "an installed version that is not one cannot be compared");
    let hostile = from_source(Kind::Opr, Some("0.3.2-1".to_string()), Ok(Some("0.3.3-1 $(reboot)".to_string())));
    assert_eq!(hostile.line(), "failed opr 0.3.2-1 -", "network text that is not a version never reaches the line");
}

#[test]
fn a_build_no_release_describes_is_unchecked_and_names_its_kind() {
    let local = unchecked(Kind::Local, None);
    assert_eq!((local.line(), local.status()), ("unchecked local - -".to_string(), NOTHING_TO_INSTALL));
    assert_eq!(unchecked(Kind::Local, Some("0.3.3-1".to_string())).line(), "unchecked local 0.3.3-1 -");
    assert_eq!(unchecked(Kind::Unowned, None).line(), "unchecked unowned - -");
}

#[test]
fn a_comparison_vercmp_could_not_make_is_said_in_plain_words() {
    let report = decide(Kind::Opr, Some("0.3.2-1".to_string()), "0.3.4-1".to_string(), None);
    assert_eq!(report.state, State::Failed("the two versions could not be compared"));
}
