use super::{stamp, tests::Fixture, I18n};

#[test]
fn opr_reds_language_rewrite_with_identical_stamps_still_refreshes() {
    let fixture = Fixture::new();
    fixture.registry();
    fixture.write("config/omarchy/language", "ja");
    let mut words = I18n::load(fixture.paths());
    assert_eq!(words.language(), "ja");
    for language in ["ar", "ja"] {
        fixture.write("config/omarchy/language", language);
        // Model the exact metadata collision observed in the OPR Docker filesystem.
        for (path, before) in &mut words.watched {
            *before = stamp(path);
        }
        assert!(!words.sources_unchanged(), "worker sees same-stamp rewrite");
        let old_language = words.language().to_owned();
        let prepared = words
            .prepare_changed()
            .expect("stable replacement is staged");
        assert_eq!(prepared.language(), language);
        assert_eq!(
            words.language(),
            old_language,
            "staging leaves current words intact"
        );
        assert!(words.poll_changed(), "same-stamp rewrite to {language}");
        assert_eq!(words.language(), language);
        assert!(!words.poll_changed(), "an unchanged language stays stable");
    }
}
