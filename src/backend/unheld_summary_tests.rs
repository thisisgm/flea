// The summary of refused entries reads as one whole sentence for each verb and count.
use std::path::Path;

#[test]
fn fxi18n3_actual_unheld_reasons_and_summaries_keep_their_catalog_identity() {
    use flea::ui_logic_status_feedback::errors::passthrough;
    for why in [
        super::LINKS,
        super::NODES,
        super::NAME,
        super::LONG,
        super::TAKEN,
    ] {
        assert!(
            passthrough(why).key.starts_with("feedback.unheld."),
            "{why}"
        );
        for count in [1, 2] {
            super::begin(false);
            for name in ["a", "b"].into_iter().take(count) {
                super::note(super::marked(&Path::new("/d").join(name), why.to_owned()));
            }
            for moving in [false, true] {
                let summary = super::summary(Path::new("/d"), moving).unwrap();
                assert_eq!(passthrough(&summary.msg).cause, Some(0), "{}", summary.msg);
            }
        }
    }
    super::begin(false);
}

// Sample output: "a could not be moved: no", or "2 items inside could not be copied; the first is a: no".
#[test]
fn the_summary_is_one_whole_sentence_for_each_verb_and_count() {
    let refuse = |names: &[&str]| {
        super::begin(false);
        for name in names {
            super::note(super::marked(&Path::new("/d").join(name), "no".to_string()));
        }
    };
    refuse(&["a"]);
    assert_eq!(
        super::summary(Path::new("/d"), true).unwrap().msg,
        "a could not be moved: no"
    );
    assert_eq!(
        super::summary(Path::new("/d"), false).unwrap().msg,
        "a could not be copied: no"
    );
    refuse(&["a", "b"]);
    assert_eq!(
        super::summary(Path::new("/d"), true).unwrap().msg,
        "2 items inside could not be moved; the first is a: no"
    );
    assert_eq!(
        super::summary(Path::new("/d"), false).unwrap().msg,
        "2 items inside could not be copied; the first is a: no"
    );
    super::begin(false);
}

// The wire path is the exact text the client compares, and the sentence names the entry by its shown spelling.
#[test]
fn a_raw_named_tree_keeps_its_exact_path_and_shows_its_lossy_name() {
    use std::os::unix::ffi::OsStrExt;
    let top = Path::new(std::ffi::OsStr::from_bytes(b"/d\xff"));
    super::begin(false);
    super::note(super::marked(
        &top.join(std::ffi::OsStr::from_bytes(b"a\xfe")),
        "no".to_string(),
    ));
    let summary = super::summary(top, true).unwrap();
    assert_eq!(
        summary.msg, "a\u{fffd} could not be moved: no",
        "the entry strips the raw top and reads lossy"
    );
    assert_eq!(
        flea::bytename::text_path(&summary.path),
        top,
        "the path decodes to the raw top"
    );
    super::begin(false);
}
