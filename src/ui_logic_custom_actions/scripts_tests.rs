//! Port of `tests/js/scripts.js`: the custom actions dispatcher listing.
use super::scripts::{self, Failure};
use crate::ui_logic_context_menus::testkit::english;

// English only: no Omarchy roster, no language file and no catalog root beside the compiled English.
fn ids(scripts: &[super::scripts::Script]) -> Vec<&str> {
    scripts.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn mn2_the_rows_are_sorted_by_name_whatever_order_find_printed() {
    let parsed = scripts::parse(
        "ocr.sh\nconvert-to-webp\nupload-to-s3.py\n",
        "/home/x/.config/flea/scripts",
    );
    assert_eq!(
        ids(&parsed),
        ["convert-to-webp", "ocr.sh", "upload-to-s3.py"]
    );
}

#[test]
fn mn2_the_label_is_the_name_without_its_extension() {
    let parsed = scripts::parse(
        "ocr.sh\nconvert-to-webp\nupload-to-s3.py\n",
        "/home/x/.config/flea/scripts",
    );
    let labels: Vec<&str> = parsed.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["convert-to-webp", "ocr", "upload-to-s3"]);
}

#[test]
fn mn2_each_row_carries_the_path_it_will_run() {
    let parsed = scripts::parse(
        "ocr.sh\nconvert-to-webp\nupload-to-s3.py\n",
        "/home/x/.config/flea/scripts",
    );
    assert_eq!(parsed[1].path, "/home/x/.config/flea/scripts/ocr.sh");
}

#[test]
fn mn2_a_blank_listing_is_no_rows_at_all() {
    assert!(scripts::parse("\n\n", "/x").is_empty());
}

#[test]
fn mn2_a_name_that_is_only_an_extension_keeps_it() {
    assert_eq!(scripts::parse(".hidden\n", "/x")[0].label, ".hidden");
}

#[test]
fn mn2_a_scripts_nonzero_exit_is_reported_by_its_own_last_stderr_line() {
    let i18n = english();
    let failed = scripts::failure("ocr.sh", "warming up\nno such page\n", 2);
    assert_eq!(
        failed,
        Failure::Line("ocr.sh \u{00b7} no such page".into())
    );
    assert_eq!(
        scripts::sentence(&failed, &i18n),
        "ocr.sh \u{00b7} no such page"
    );
}

#[test]
fn mn2_one_that_said_nothing_is_reported_by_its_status() {
    let i18n = english();
    let failed = scripts::failure("ocr.sh", "  \n", 3);
    assert_eq!(
        failed,
        Failure::Exited {
            name: "ocr.sh".into(),
            code: 3
        }
    );
    assert_eq!(
        scripts::sentence(&failed, &i18n),
        "ocr.sh exited with status 3"
    );
}
