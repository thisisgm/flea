use super::*;

#[test]
fn an_empty_or_cut_short_entry_is_replaced_as_0312_did() {
    let d = crate::backend::testdir::TestDir::new("thumb-torn");
    let cache = Cache::at(d.path().to_path_buf());
    let uri = "file:///source";
    let target = cache.large_path(uri);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    for torn in [&b""[..], &b"\x89PN"[..]] {
        std::fs::write(&target, torn).unwrap();
        assert!(
            matches!(cache.lookup(Path::new("/source"), 1, "table"), Hit::Miss),
            "{torn:?}: a miss, so the thumbnailer runs"
        );
        let temp = d.join("new.png");
        crate::backend::thumbwrite::write_marker(&temp, uri, 1, Some("table")).unwrap();
        assert!(
            matches!(publish(&temp, uri, 1, target.clone()), Outcome::Ready(_)),
            "{torn:?}: publish replaces the damage"
        );
        assert!(
            matches!(
                cache.lookup(Path::new("/source"), 1, "table"),
                Hit::Ready(_)
            ),
            "{torn:?}: and the next lookup hits"
        );
    }
}

#[test]
fn a_foreign_entry_is_refused_before_the_thumbnailer_runs() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let seconds = format!("3.{}", std::process::id());
    let body = format!("[Thumbnailer Entry]\nTryExec=/usr/bin/sleep\nExec=/usr/bin/sleep {}\nMimeType=image/jpeg;\n", seconds);
    let entries = [("guard.thumbnailer".to_string(), body)];
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::from_entries(&entries, &aliases));
    let d = TestDir::new("thumbs-guard-first");
    let tables = Tables {
        aliases,
        specs,
        cache: Cache::at(d.path().to_path_buf()),
        worker: None,
    };
    let target = tables
        .cache
        .large_path(&uri_for(Path::new("/usr/bin/sleep")));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "{\"foreign\":true}").unwrap();
    let started = std::time::Instant::now();
    let outcome = run_one_in(&tables, &mut job("/usr/bin/sleep"), &d.join("raw"));
    assert!(matches!(outcome, Outcome::Failed));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the thumbnailer ran for {:?} before the guard",
        started.elapsed()
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "{\"foreign\":true}"
    );
}
