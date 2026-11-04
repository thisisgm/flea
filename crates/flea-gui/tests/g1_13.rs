//! G1.13: the filter strip and the status bar as the window hosts them, every check pinning host composition only.
use flea::i18n::{I18n, Paths};
use std::{path::PathBuf, process::Command as Process, sync::OnceLock};

#[path = "g1_13/ipc.rs"]
mod ipc;
#[path = "g1_13/locale.rs"]
mod locale;
#[path = "g1_13/status.rs"]
mod status;
#[path = "g1_13/strip.rs"]
mod strip;

pub fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-13");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Process::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-13\n").unwrap();
    root
}

// English from the embedded catalog only, never a catalog installed on the host.
pub fn english() -> I18n {
    static CATALOG: OnceLock<I18n> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let root = root();
            std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
            std::fs::write(
                root.join("omarchy/default/i18n/locales.json"),
                r#"{"en":{}}"#,
            )
            .unwrap();
            std::fs::write(root.join("language"), "en").unwrap();
            I18n::load(Paths {
                omarchy: root.join("omarchy"),
                language_file: root.join("language"),
                catalogs: vec![root.join("catalog")],
            })
        })
        .clone()
}
