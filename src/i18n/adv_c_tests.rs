use super::tests::Fixture;
use super::*;

#[test]
fn adv_c_f1_rounded_size_promotes_at_si_boundaries() {
    let locale = Locale::default();
    for (bytes, expected) in [
        (999, "999 B"), (1000, "1.0 kB"),
        (999_949, "999.9 kB"), (999_950, "1.0 MB"),
        (999_949_999, "999.9 MB"), (999_950_000, "1.0 GB"),
        (999_949_999_999, "999.9 GB"), (999_950_000_000, "1.0 TB"),
        (999_949_999_999_999, "999.9 TB"), (999_950_000_000_000, "1.0 PB"),
    ] {
        assert_eq!(locale.format_size(bytes), expected, "bytes {bytes}");
    }
    let fa = Locale { decimal: "٫".into(), zero_digit: '۰', ..locale };
    assert_eq!(fa.format_size(999_950), "۱٫۰ MB");
}

#[test]
fn adv_c_f7_norwegian_grouping_without_generated_locale() {
    for language in ["nb-NO", "nn-NO", "no-NO"] {
        let mut locale = Locale {
            format_locale: language.into(), glibc_locale: "missing_FLEA_TEST".into(),
            decimal: ",".into(), ..Locale::default()
        };
        format::set_grouping(&mut locale);
        assert_eq!(locale.format_count(1_234_567), "1\u{a0}234\u{a0}567", "{language}");
    }
}

#[test]
fn adv_c_f4_roster_refusal_child() {
    let Some(root) = std::env::var_os("FLEA_I18N_REFUSAL_ROOT") else { return; };
    let root = PathBuf::from(root);
    let i = I18n::load(Paths {
        omarchy: root.clone(), language_file: root.join("unused-language"), catalogs: Vec::new(),
    });
    assert_eq!(i.language(), "en");
}

#[test]
fn adv_c_f4_roster_refusal_names_locales_json_path() {
    let f = Fixture::new();
    f.write("omarchy/default/i18n/locales.json", "{invalid");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "i18n::adv_c_tests::adv_c_f4_roster_refusal_child", "--nocapture"])
        .env("FLEA_I18N_REFUSAL_ROOT", f.path("omarchy")).output().unwrap();
    assert!(output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("refusing i18n roster"), "{error}");
    assert!(error.contains(f.path("omarchy/default/i18n/locales.json").to_str().unwrap()), "{error}");
}

#[test]
fn adv_c_f8_installed_binary_ignores_home_and_build_catalogs() {
    let f = Fixture::new();
    f.write("home/i18n/en/flea.json", r#"{"app.name":"Unrelated home catalog"}"#);
    f.write("build/i18n/en/flea.json", r#"{"app.name":"Unrelated build catalog"}"#);
    let source = f.path("build");
    let executable = f.path("home/.local/bin/flea");
    let roots = catalog_dirs(None, Some(&executable), &source, false);
    assert!(!roots.contains(&f.path("home/i18n")), "{roots:?}");
    assert!(!roots.contains(&source.join("i18n")), "{roots:?}");
    assert_eq!(roots, vec![f.path("home/.local/share/flea/i18n"), PathBuf::from("/usr/share/flea/i18n"), PathBuf::from("/usr/local/share/flea/i18n")]);
    let i = I18n::load(Paths {
        omarchy: f.path("omarchy"), language_file: f.path("language"), catalogs: roots,
    });
    assert_eq!(i.tr("app.name"), "Flea");
}

#[test]
fn installed_prefix_catalog_translates_with_flea_i18n_unset() {
    let f = Fixture::new();
    f.registry();
    f.write("config/omarchy/language", "de\n");
    f.write("prefix/share/flea/i18n/de/flea.json", include_str!("../../i18n/de/flea.json"));
    let roots = catalog_dirs(None, Some(&f.path("prefix/bin/flea")), &f.path("build"), false);
    assert_eq!(roots[0], f.path("prefix/share/flea/i18n"));
    // Only that root, so a catalog the host keeps in /usr/share/flea/i18n cannot answer for it.
    let i = I18n::load(Paths {
        omarchy: f.path("omarchy"), language_file: f.path("config/omarchy/language"), catalogs: roots[..1].to_vec(),
    });
    assert_eq!(i.language(), "de");
    assert_eq!(i.tr_args("plugins.error.unknown", &["x"]), "Kein Plugin heißt x");
}

#[test]
fn adv_c_f8_source_catalogs_require_debug_or_that_trees_target() {
    let source = Path::new("/build/flea");
    let external = Path::new("/opt/flea/bin/flea");
    assert!(catalog_dirs(None, Some(external), source, true).contains(&source.join("i18n")));
    assert!(catalog_dirs(None, Some(Path::new("/build/flea/target/release/flea")), source, false).contains(&source.join("i18n")));
    assert!(!catalog_dirs(None, Some(Path::new("/build/flea/target-other/flea")), source, false).contains(&source.join("i18n")));
    assert!(!catalog_dirs(None, None, source, false).contains(&source.join("i18n")));
    assert_eq!(catalog_dirs(Some(PathBuf::from("/operator/catalogs")), Some(external), source, false)[0], Path::new("/operator/catalogs"));
}
