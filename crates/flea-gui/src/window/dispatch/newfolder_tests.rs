use super::*;

struct CatalogRoot(std::path::PathBuf);
impl Drop for CatalogRoot {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with("/tmp")
                && self.0.join(".newfolder-root").is_file()
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[gpui::test]
fn newfolder_gui_key_sends_the_explicit_catalog_base(cx: &mut TestAppContext) {
    let out = std::process::Command::new("mktemp")
        .args(["-d", "/tmp/flea-newfolder-gui.XXXXXX"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let root = CatalogRoot(std::path::PathBuf::from(
        String::from_utf8(out.stdout).unwrap().trim(),
    ));
    assert!(
        root.0.is_absolute()
            && root.0.parent() == Some(std::path::Path::new("/tmp"))
            && root.0.is_dir()
    );
    assert!(root
        .0
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("flea-newfolder-gui."));
    std::fs::write(root.0.join(".newfolder-root"), "owned").unwrap();
    std::fs::create_dir_all(root.0.join("en")).unwrap();
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let mut sent = wire(&shell, cx);
    for base in [
        "Nouveau dossier",
        "新建文件夹",
        "مجلد جديد",
        "नया फ़ोल्डर",
        "Новая папка",
        "say \"hi\"",
    ] {
        std::fs::write(
            root.0.join("en/flea.json"),
            format!(r#"{{"prompt.newFolder":"{}"}}"#, flea::json::escape(base)),
        )
        .unwrap();
        let catalog = flea::i18n::I18n::load(flea::i18n::Paths {
            omarchy: root.0.join("omarchy"),
            language_file: root.0.join("language"),
            catalogs: vec![root.0.clone()],
        });
        shell.update(cx, |shell, _| {
            shell.locale.catalog = std::sync::Arc::new(catalog)
        });
        cx.simulate_keystrokes("ctrl-shift-n");
        let lines = sent();
        assert_eq!(lines.len(), 1);
        assert_eq!(
            flea::json::field_str(&lines[0], "defaultBase").as_deref(),
            Some(base)
        );
        assert_eq!(
            flea::json::field_str(&lines[0], "path").as_deref(),
            Some("/files")
        );
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
