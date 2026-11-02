use super::*;
use crate::backend::testdir::TestDir;

fn localized(root: &TestDir, code: &str, xml: &str) -> Kinds {
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    root.file("language", code);
    let catalog = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n")],
    });
    let mut kinds = Kinds::new();
    kinds.catalog = Some(catalog.into());
    kinds
        .comments
        .insert("image/png".into(), Comments::parse(xml));
    kinds.refresh();
    kinds
}

#[test]
fn lang_polish_png_missing_provider_translations_use_the_selected_catalog() {
    let root = TestDir::new("lang-polish-png");
    for (code, expected) in [
        ("bn", "PNG ছবি"),
        ("fa", "تصویر PNG"),
        ("fil", "Larawang PNG"),
        ("no", "PNG-bilde"),
        ("sw", "Picha ya PNG"),
        ("ta", "PNG படம்"),
        ("th", "ภาพ PNG"),
        ("ur", "PNG تصویر"),
        ("uz", "PNG tasviri"),
    ] {
        let mut kinds = localized(&root, code, "<comment>PNG image</comment>");
        assert_eq!(
            kinds.comment("image/png").as_deref(),
            Some(expected),
            "{code}"
        );
        assert_eq!(kinds.reads(), 0, "the cached MIME data is reused");
    }
}

#[test]
fn lang_polish_png_preserves_provider_words_and_norwegian_alias() {
    let root = TestDir::new("lang-polish-provider");
    for (code, tag) in [
        ("fa", "fa"),
        ("no", "nb"),
        ("de", "de"),
        ("zh-CN", "zh-Hans-CN"),
    ] {
        let xml =
            format!("<comment xml:lang='{tag}'>Provider PNG</comment><comment>PNG image</comment>");
        let mut kinds = localized(&root, code, &xml);
        assert_eq!(
            kinds.comment("image/png").as_deref(),
            Some("Provider PNG"),
            "{code}"
        );
    }
}

#[test]
fn lang_polish_png_live_switch_reselects_cached_provider_data_and_fallbacks() {
    let root = TestDir::new("lang-polish-switch");
    let mut kinds = localized(
        &root,
        "bn",
        "<comment xml:lang='de'>PNG-Bild</comment><comment>PNG image</comment>",
    );
    for (code, expected) in [
        ("bn", "PNG ছবি"),
        ("de", "PNG-Bild"),
        ("fa", "تصویر PNG"),
        ("en", "PNG image"),
    ] {
        root.file("language", code);
        kinds.refresh();
        assert_eq!(
            kinds.comment("image/png").as_deref(),
            Some(expected),
            "{code}"
        );
    }
    assert_eq!(kinds.reads(), 0);
}

#[test]
fn lang_polish_all_fifty_catalogs_supply_a_localized_png_fallback() {
    let root = TestDir::new("lang-polish-complete");
    let roster = crate::jsondoc::parse(include_str!("../i18n/fixtures/locales.json")).unwrap();
    let locales = roster.as_object().unwrap();
    assert_eq!(locales.len(), 50);
    for (code, _) in locales {
        let mut kinds = localized(&root, code, "<comment>PNG image</comment>");
        let word = kinds.comment("image/png").expect("registered PNG kind");
        assert!(!word.trim().is_empty(), "{code}");
        if code != "en" {
            assert_ne!(word, "PNG image", "{code}: no English fallback");
        } else {
            assert_eq!(word, "PNG image");
        }
    }
}
