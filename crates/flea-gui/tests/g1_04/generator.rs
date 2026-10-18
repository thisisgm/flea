use super::*;

fn generate(root: &std::path::Path, source: &std::path::Path) -> std::process::Output {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Command::new("python3")
        .arg("-B")
        .arg(repository.join("tools/flea-keymap-gen"))
        .arg(root.join("Keymap.js"))
        .arg(source)
        .arg("--rust")
        .arg(root.join("keys.rs"))
        .arg("--catalog")
        .arg(root.join("catalog.json"))
        .output()
        .unwrap()
}

#[test]
fn generation_is_reproducible_and_rejects_bad_or_duplicate_keys() {
    let root = root();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = repository.join("keys.toml");
    std::fs::copy(
        repository.join("i18n/en/flea.json"),
        root.join("catalog.json"),
    )
    .unwrap();
    let valid = generate(&root, &source);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    assert_eq!(
        std::fs::read(root.join("Keymap.js")).unwrap(),
        std::fs::read(repository.join("ui/js/Keymap.js")).unwrap()
    );
    assert_eq!(
        std::fs::read(root.join("keys.rs")).unwrap(),
        std::fs::read(repository.join("src/keys_generated.rs")).unwrap()
    );
    assert_eq!(
        std::fs::read(root.join("catalog.json")).unwrap(),
        std::fs::read(repository.join("i18n/en/flea.json")).unwrap()
    );
    let original = std::fs::read_to_string(source).unwrap();
    let cases = [
        (original.replacen("key = \"D\"", "key = \"Downn\"", 1), "invalid key name: Downn"),
        (format!("{original}\n[[preset]]\nname = \"mac\"\nmods = \"ctrl\"\nkey = \"1\"\nkeys = \"ctrl-1\"\naction = \"viewGrid\"\nlabel = \"grid\"\n"), "the mac preset claims ctrl-1 twice"),
        (format!("{original}\n[[ctrl]]\nkey = \"D\"\naction = \"copy\"\n"), "shared listing key ctrl-d claims pageDown and copy"),
        (original.replacen("mods = \"ctrl\"", "mods = \"hyper\"", 1), "invalid preset or modifiers"),
        (original.replacen("from = 1", "from = 10", 1), "single-digit integer bounds"),
    ];
    for (text, message) in cases {
        let invalid = root.join("invalid.toml");
        std::fs::write(&invalid, text).unwrap();
        let result = generate(&root, &invalid);
        assert!(!result.status.success(), "accepted {message}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read(root.join("keys.rs")).unwrap(),
            std::fs::read(repository.join("src/keys_generated.rs")).unwrap()
        );
    }
}
