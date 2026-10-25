use super::*;

#[test]
fn foreign_and_newer_documents_survive_adoption_publication_and_sweep() {
    let (_dir, runtime) = runtime_fixture("gvfs-preserve");
    for original in [
        "{\"foreign\":true}\n",
        "{\"schemaVersion\":999,\"rows\":[]}\n",
        "unparsable",
    ] {
        let dest = write_prefetch(&runtime, "gvfs-preserve.list", original);
        assert!(adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime)).is_none());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), original);
        assert!(publish(&dest, TWO_ROWS.as_bytes()).is_err());
        sweep_dir(&runtime, SystemTime::now() + Duration::from_secs(120));
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), original);
    }
}

#[test]
fn garbage_is_refused_and_preserved_so_the_scan_lists_itself() {
    let (_dir, runtime) = runtime_fixture("gvfs-garbage");
    let dest = write_prefetch(&runtime, "gvfs-1.list", "this is not a gio line\n");
    assert!(adopt_in(SHARE, false, &dest, live_deadline(), 0, Some(&runtime)).is_none());
    assert_eq!(
        std::fs::read_to_string(&dest).unwrap(),
        "this is not a gio line\n"
    );
    assert!(!claimed_path(&dest).exists());
}

#[test]
fn an_empty_or_cut_short_listing_is_swept() {
    let (_dir, runtime) = runtime_fixture("gvfs-torn");
    for (name, torn) in [
        ("gvfs-empty.list", ""),
        ("gvfs-marker.list", "flea-gvfs-f"),
        ("gvfs-rows.list", &TWO_ROWS[..TWO_ROWS.len() - 9]),
    ] {
        let dest = write_prefetch(&runtime, name, torn);
        sweep_dir(&runtime, SystemTime::now() + Duration::from_secs(120));
        assert!(
            !dest.exists(),
            "{torn:?}: a torn listing ages out like a whole one"
        );
    }
}
