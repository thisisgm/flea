use super::*;
use std::process::Command;

#[test]
fn plural_boundaries_include_millions_negatives_and_the_largest_count() {
    for code in ["fr", "pt-BR", "ca", "es-MX", "it", "pt"] {
        assert_eq!(plural::category(code, 1_000_000), "many");
    }
    assert_eq!(plural::category("pt", 0), "other");
    assert_eq!(plural::category("pt-BR", 0), "one");
    assert_eq!(plural::category("ru", -22), "few");
    assert_eq!(plural::category("ar", -2), "two");
    assert_eq!(plural::category("en", i64::MIN), "other");
}

#[test]
fn plural_rules_equal_omarchy_jq_for_every_roster_code_and_count_0_through_10000() {
    match Command::new("jq").arg("--version").output() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP i18n plural equality: jq is absent on the build host");
            return;
        }
        result => assert!(result.expect("run jq --version").status.success(), "jq --version failed"),
    }
    let root = crate::i18n::test_catalogs::english_paths().omarchy.join("default/i18n");
    let (registry, rules, source) = if root.join("locales.json").exists() {
        (
            catalog::read(&root.join("locales.json")).expect("read installed Omarchy roster"),
            catalog::read(&root.join("plural.jq")).expect("read installed Omarchy plural.jq"),
            root.display().to_string(),
        )
    } else {
        (include_str!("fixtures/locales.json").into(), include_str!("fixtures/plural.jq").into(), "Omarchy bf9ee5c fixtures".into())
    };
    let raw = document::parse(&registry).expect("valid Omarchy roster");
    let entries = raw.as_object().expect("Omarchy roster is an object");
    assert!(!entries.is_empty());
    let codes = crate::jsondoc::Json::Arr(entries.iter().map(|(code, _)| {
        assert!(locale::valid_code(code), "unrecognized roster code {code}");
        crate::jsondoc::Json::Str(code.clone())
    }).collect());
    let query = format!("{rules}\n$codes[] as $code | range(0; 10001) as $n | plural_category($code; $n)");
    let output = Command::new("jq").args(["-nr", "--argjson", "codes"])
        .arg(crate::jsondoc::render(&codes)).arg(query).output().expect("run Omarchy plural.jq");
    assert!(output.status.success(), "jq failed: {}", String::from_utf8_lossy(&output.stderr));
    let output = String::from_utf8(output.stdout).unwrap();
    let mut actual = output.lines();
    for (code, _) in entries {
        for count in 0..=10_000 {
            assert_eq!(actual.next(), Some(plural::category(code, count)), "code {code}, count {count}");
        }
    }
    assert_eq!(actual.next(), None);
    eprintln!("i18n plural equality: jq present, {} codes x 10001 counts (0..10000), source: {source}", entries.len());
}
