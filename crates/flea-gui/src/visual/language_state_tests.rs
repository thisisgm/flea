//! A language scene in a running window: the receipt comes only from the catalog the scene names.
use super::visual_tests::{apply, draw_frame, expire, root_shell};
use super::*;
use flea::jsondoc::Json;

/// Runs the window on a catalog whose language is `code`, from a marked private root; the host's locale files are never read.
fn run_catalog(shell: &mut Shell, code: &str, revision: u64) {
    let root = std::env::temp_dir().join(format!("flea-scene-lang.{}.{code}", std::process::id()));
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(root.join(".flea-test-sandbox"), "SCENE-LANG catalog\n").unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        format!(r#"{{"{code}":{{"name":"{code}","formatLocale":"{code}"}}}}"#),
    )
    .unwrap();
    std::fs::write(root.join("language"), format!("{code}\n")).unwrap();
    let catalog = flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("i18n")],
    });
    if root.join(".flea-test-sandbox").is_file() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    assert_eq!(catalog.language(), code);
    shell.locale = crate::window::locale::Snapshot {
        revision,
        catalog: std::sync::Arc::new(catalog),
    };
}

#[gpui::test]
fn a_language_scene_earns_a_receipt_only_from_the_catalog_it_names(cx: &mut TestAppContext) {
    let (entity, cx, _requests) = root_shell(cx);
    let name = "i50.fit.de.list.rest.gm.r0";
    let spec = || {
        crate::visual::contract::Spec::parse(
            &std::process::id().to_string(),
            name,
            r#"{"path":"/files"}"#,
        )
        .unwrap()
    };
    entity.update(cx, |shell, _| run_catalog(shell, "ja", 2));
    let receive = apply(&entity, cx, spec());
    expire(&entity, cx, &receive);
    entity.update(cx, |shell, _| {
        assert!(shell.visual.receipt.is_none());
        run_catalog(shell, "de", 3);
    });
    let receive = apply(&entity, cx, spec());
    draw_frame(&entity, cx);
    let receipt = flea::jsondoc::parse(&receive.try_recv().unwrap().reply.stdout).unwrap();
    assert_eq!(receipt.get("scene").and_then(Json::as_str), Some(name));
    assert_eq!(receipt.get("language").and_then(Json::as_str), Some("de"));
    assert_eq!(
        receipt.get("locale_revision").and_then(Json::as_f64),
        Some(3.)
    );
}
