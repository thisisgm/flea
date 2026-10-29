use super::*;
use flea::ui_logic_st1::Text;

// The captured signed and local package fields from src/update_tests.rs.
pub(super) const OPR_INFO: &str = "Name            : flea
Version         : 0.3.2-1
Description     : Fast, keyboard-first file manager for Omarchy
Architecture    : x86_64
Install Reason  : Explicitly installed
Install Script  : No
Validated By    : Signature
";

pub(super) const LOCAL_INFO: &str = "Name            : flea
Version         : 0.3.3-1
Validated By    : None
";

// The build-date answer from v0.3.12's SettingsAbout.js and tests/js/settingsabout.js.
const CUSTOM_INFO: &str = "Name            : flea-bin
Version         : 0.3.4-1
Packager        : Unknown Packager
Build Date      : Wed Sep 23 12:40:50 2026
Install Script  : No
Validated By    : None
";

#[test]
fn settings_about_provenance_parses_signed_package_fields() {
    assert_eq!(
        about::package_facts(OPR_INFO),
        about::PackageFacts {
            package: "flea 0.3.2-1".into(),
            signed: true,
            ..Default::default()
        }
    );
}

#[test]
fn settings_about_provenance_parses_local_package_fields() {
    assert_eq!(
        about::package_facts(LOCAL_INFO),
        about::PackageFacts {
            package: "flea 0.3.3-1".into(),
            ..Default::default()
        }
    );
    assert_eq!(
        about::package_facts(CUSTOM_INFO),
        about::PackageFacts {
            package: "flea-bin 0.3.4-1".into(),
            built: "Wed Sep 23 12:40:50 2026".into(),
            signed: false,
        }
    );
}

#[test]
fn settings_about_provenance_parses_empty_package_answer() {
    assert_eq!(about::package_facts(""), about::PackageFacts::default());
}

#[test]
fn settings_about_provenance_applies_owned_package_fields() {
    for (owner, answer, source, package) in [
        (
            "flea",
            OPR_INFO,
            "settings.about.source_opr",
            "flea 0.3.2-1",
        ),
        (
            "flea",
            LOCAL_INFO,
            "settings.about.source_local",
            "flea 0.3.3-1",
        ),
        (
            "flea-bin",
            CUSTOM_INFO,
            "settings.about.source_local",
            "flea-bin 0.3.4-1",
        ),
        (
            "flea-bin",
            "Name : flea-bin\nVersion : 0.3.4-1\nValidated By : Signature\n",
            "settings.about.source_local",
            "flea-bin 0.3.4-1",
        ),
        (
            "flea-git",
            "Name : flea-git\nVersion : r123-1\nValidated By : Signature\n",
            "settings.about.source_local",
            "flea-git r123-1",
        ),
        (
            "flea-git",
            "Name : flea-git\nVersion : r123-1\n",
            "settings.about.source_local",
            "flea-git r123-1",
        ),
    ] {
        let mut facts = about::Facts::default();
        apply(&mut facts, owner, &about::package_facts(answer));
        assert_eq!(facts.source, Text::Key(source), "owner {owner}");
        assert_eq!(facts.package.as_deref(), Some(package), "owner {owner}");
        assert_eq!(facts.built, about::package_facts(answer).built);
    }
}

#[test]
fn settings_about_provenance_failed_query_keeps_unknown_package_and_build() {
    let mut facts = about::Facts::default();
    apply(&mut facts, "flea", &about::package_facts(""));
    assert_eq!(facts.source, Text::Key("settings.about.source_local"));
    assert_eq!(facts.package, None);
    assert!(facts.built.is_empty());
}
