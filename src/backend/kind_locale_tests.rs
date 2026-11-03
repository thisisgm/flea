use super::*;
use crate::backend::{
    aliases::Aliases, icons::Names, listing::Listing, mime::Db, rows, searchrowcache::Cache,
    testdir::TestDir, thumbspec::Thumbnailers,
};

fn localized(root: &TestDir, code: &str) -> Kinds {
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    root.file("language", code);
    let catalog = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: Vec::new(),
    });
    let mut kinds = Kinds::new();
    kinds.catalog = Some(catalog.into());
    kinds.comments.insert(
        "image/png".into(),
        Comments::parse(concat!(
            "<comment>PNG image</comment><comment xml:lang='de'>PNG-Bild</comment>",
            "<comment xml:lang='fr'>image PNG</comment>",
        )),
    );
    kinds
}

#[test]
fn kind_lang_changed_locale_reselects_without_xml_reads() {
    let root = TestDir::new("kind-live");
    let mut kinds = localized(&root, "de");
    let revision = kinds.refresh();
    assert_eq!(kinds.comment("image/png").as_deref(), Some("PNG-Bild"));
    root.file("language", "fr");
    assert_ne!(kinds.refresh(), revision);
    assert_eq!(kinds.comment("image/png").as_deref(), Some("image PNG"));
    assert_eq!(kinds.reads(), 0, "the cached XML was not read again");
    root.file("language", "not-a-locale");
    kinds.refresh();
    assert_eq!(kinds.comment("image/png").as_deref(), Some("PNG image"));
}

#[test]
fn kind_lang_search_cache_observes_a_change_another_response_saw_first() {
    let root = TestDir::new("kind-search");
    let mut kinds = localized(&root, "de");
    root.file("image.png", "png");
    let mut listing = Listing::new();
    listing.push("image.png", false);
    let (mime, icons, aliases) = (
        Db::from_str("50:image/png:*.png\n"),
        Names::from_str(""),
        Aliases::from_str(""),
    );
    let thumbs = Thumbnailers::from_entries(&[], &aliases);
    let mut cache = Cache::default();
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    let render = |cache: &mut Cache, kinds: &mut Kinds| {
        let (metas, ms) = cache.window(root.path(), &listing, 0, 1, &body);
        rows::rows_line_cached(
            root.path(),
            &body,
            &listing,
            &metas,
            0,
            ms,
            &mime,
            &icons,
            &aliases,
            &thumbs,
            kinds,
            cache,
        )
    };
    for _ in 0..3 {
        assert!(render(&mut cache, &mut kinds).contains("PNG-Bild"));
    }
    assert!(cache.rendered(&root.join("image.png")).is_some());
    root.file("language", "fr");
    kinds.refresh();
    assert_eq!(kinds.comment("image/png").as_deref(), Some("image PNG"));
    let translated = render(&mut cache, &mut kinds);
    assert!(translated.contains("image PNG"), "{translated}");
    assert!(!translated.contains("PNG-Bild"));
}
