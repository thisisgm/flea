use super::*;

#[test]
fn a_folder_read_keeps_the_first_entries_and_stops_once_another_is_wanted() {
    let dir = std::env::temp_dir().join(format!("flea-list-side-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Zeta")).unwrap();
    std::fs::create_dir_all(dir.join("alpha")).unwrap();
    for index in 0..MAX_ENTRIES + 20 {
        std::fs::write(dir.join(format!("f{index:04}")), "").unwrap();
    }
    std::fs::write(dir.join(".hidden"), "").unwrap();
    let path = dir.to_string_lossy().into_owned();
    WANTED.store(7, Ordering::Relaxed);
    let Some(Answer::Listed(entries, total)) = read(&path, false, 7, None) else {
        panic!("the folder lists");
    };
    // Every entry counts, but only the first MAX_ENTRIES are held: folders first, then by name without case.
    assert_eq!(total, MAX_ENTRIES + 22);
    assert_eq!(entries.len(), MAX_ENTRIES);
    assert_eq!(entries[0], ("alpha".to_owned(), true));
    assert_eq!(entries[1], ("Zeta".to_owned(), true));
    assert_eq!(entries[2].0, "f0000");
    assert_eq!(
        entries[MAX_ENTRIES - 1].0,
        format!("f{:04}", MAX_ENTRIES - 3)
    );
    // A newer folder wanted: the read gives up rather than finish for nobody.
    WANTED.store(8, Ordering::Relaxed);
    assert!(read(&path, false, 7, None).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_release_lasts_only_its_listing_and_leaving_stops_the_read() {
    PEEK.with(|peek| peek.borrow_mut().released = Some(("/f/a".into(), 3)));
    assert!(released("/f/a", 3));
    // Another listing (a tab switch, a reload) forgets it, and it does not come back.
    assert!(!released("/f/a", 4));
    assert!(!released("/f/a", 3));
    PEEK.with(|peek| {
        let mut peek = peek.borrow_mut();
        peek.shown = Some(Peek {
            path: "/f/a".into(),
            hidden: false,
            released: false,
            generation: 9,
            read_at: std::time::Instant::now(),
            entries: None,
        });
        peek.released = Some(("/f/a".into(), 3));
        peek.next = 9;
    });
    WANTED.store(9, Ordering::Relaxed);
    // A frame with no folder peek: the read for generation 9 is no longer wanted, and the release is gone.
    leave();
    assert_ne!(WANTED.load(Ordering::Relaxed), 9);
    assert!(PEEK.with(|peek| peek.borrow().shown.is_none() && peek.borrow().released.is_none()));
}
