use super::*;

#[test]
fn shared_catalog_worker_reloads_for_two_windows_and_keeps_english_fallback() {
    let root = root();
    std::fs::create_dir_all(root.join("catalog/en")).unwrap();
    std::fs::create_dir_all(root.join("catalog/de")).unwrap();
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"de":{"name":"Deutsch"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "en").unwrap();
    std::fs::write(
        root.join("catalog/de/flea.json"),
        r#"{"window.loading":"Laden"}"#,
    )
    .unwrap();
    let paths = flea::i18n::Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![root.join("catalog")],
    };
    let (service, first) = Service::start(paths).unwrap();
    let initial = first.recv_blocking().unwrap();
    assert_eq!(initial.catalog.language(), "en");
    assert_eq!(initial.catalog.tr("window.loading"), "Loading");
    assert_eq!(
        initial
            .catalog
            .tr_args("window.unknown_flag", &["--literal"]),
        "Unknown flag --literal"
    );
    assert_eq!(initial.catalog.trn("items.count", 1), "1 item");
    assert_eq!(initial.catalog.trn("items.count", 2), "2 items");
    let second = service.subscribe();
    assert_eq!(second.recv_blocking().unwrap().revision, initial.revision);
    std::fs::write(root.join("language"), "de\n").unwrap();
    let mut observed = initial.catalog.as_ref().clone();
    assert!(
        observed.poll_changed(),
        "the core did not observe the language file edit"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let changed = loop {
        assert!(
            Instant::now() < deadline,
            "catalog reload did not arrive: current={:?}, closed={}, receivers={}",
            service
                .current()
                .map(|s| (s.revision, s.catalog.language().to_owned())),
            first.is_closed(),
            first.receiver_count()
        );
        match first.try_recv() {
            Ok(snapshot) => break snapshot,
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    assert!(changed.revision > initial.revision);
    assert_eq!(changed.catalog.language(), "de");
    assert_eq!(changed.catalog.tr("window.loading"), "Laden");
    assert_eq!(
        changed.catalog.tr("window.waiting"),
        "That folder is not responding."
    );
    assert_eq!(second.recv_blocking().unwrap().revision, changed.revision);
    assert_eq!(service.current().unwrap().revision, changed.revision);
}
