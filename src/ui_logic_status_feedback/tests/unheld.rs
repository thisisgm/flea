use super::{root, I18n, Paths, Root};
use crate::ui_logic_status_feedback::errors::clause;

fn german(root: &Root) -> I18n {
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../../i18n/fixtures/locales.json"),
    );
    root.file("language", "de");
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n")],
    })
}

#[test]
fn fxi18n3_paste_summary_and_drive_reasons_read_in_german() {
    let root = root();
    let catalog = german(&root);
    assert_eq!(
        clause("this drive cannot hold links", &catalog),
        "Dieses Laufwerk kann keine Verknüpfungen speichern"
    );
    let summary = clause(
        "2 items inside could not be copied; the first is sub/a: this drive cannot hold links",
        &catalog,
    );
    assert!(!summary.contains("could not be copied"), "{summary}");
    assert!(summary.contains("sub/a"), "{summary}");
    assert!(!summary.contains("cannot hold links"), "{summary}");
}

#[test]
fn fxi18n3_every_catalog_translates_copy_move_and_all_five_drive_reasons() {
    let root = root();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    let roster = include_str!("../../i18n/fixtures/locales.json");
    root.file("omarchy/default/i18n/locales.json", roster);
    let roster = crate::jsondoc::parse(roster).unwrap();
    for (code, _) in roster.as_object().unwrap() {
        root.file("language", code);
        let catalog = I18n::load(Paths {
            omarchy: root.join("omarchy"),
            language_file: root.join("language"),
            catalogs: vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n")],
        });
        for why in [
            "this drive cannot hold links",
            "this drive cannot hold special files",
            "this drive does not allow that name",
            "that name is too long for this drive",
            "this drive treats that name as the same as another in the folder",
            "':' is not allowed in a name on this vfat drive",
        ] {
            for verb in ["copied", "moved"] {
                for count in [1, 2, 3, 5, 11, 21, 101] {
                    let message = if count == 1 {
                        format!("sub/a: b could not be {verb}: {why}")
                    } else {
                        format!("{count} items inside could not be {verb}; the first is sub/a: b: {why}")
                    };
                    let rendered = clause(&message, &catalog);
                    assert!(rendered.contains("sub/a: b"), "{code}: {rendered}");
                    assert!(
                        !rendered.contains("feedback.unheld.") && !rendered.contains('%'),
                        "{code}: {rendered}"
                    );
                    if code == "en" {
                        assert_eq!(rendered, message);
                    } else {
                        assert!(
                            !rendered.contains("could not be")
                                && !rendered.contains("this drive")
                                && !rendered.contains("is not allowed"),
                            "{code}: {rendered}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fxi18n3_unknown_diagnostics_are_kept_verbatim() {
    let root = root();
    // German renders every recognised sentence differently, so an unknown one that was translated cannot equal its source.
    for catalog in [super::english(), german(&root)] {
        for message in [
            "not an unheld error",
            "0 items inside could not be copied; the first is a: this drive cannot hold links",
            "2 items inside could not be moved; the first is a: third-party failure",
            "this drive cannot hold links: user data",
            "this drive cannot hold links could not be copied: third-party failure",
        ] {
            assert_eq!(clause(message, &catalog), message);
        }
    }
}
