use super::super::*;

const ORIGINALS: &str = "trash:///old-draft.txt\t13\t(regular)\ttrash::orig-path=/fx/v/work/old-draft.txt\ntrash:///special.txt\t17\t(regular)\ttrash::orig-path=/fx/v/work/report trash::deletion-date=literal.txt\n";
// GIO joins requested attributes with spaces inside the last tab-delimited column.
const JOINED_ORIGINALS: &str = "trash:///old-draft.txt\t13\t(regular)\tstandard::target-uri=file:///home/gm/.local/share/Trash/files/old-draft.txt trash::orig-path=/fx/v/work/old-draft.txt\ntrash:///special.txt\t17\t(regular)\tstandard::target-uri=file:///home/gm/.local/share/Trash/files/special.txt trash::orig-path=/fx/v/work/report trash::deletion-date=literal.txt\n";
const METADATA: &str = "trash:///old-draft.txt\t13\t(regular)\tstandard::target-uri=file:///home/gm/.local/share/Trash/files/old-draft.txt trash::deletion-date=2025-06-14T09:30:00\ntrash:///special.txt\t17\t(regular)\tstandard::target-uri=file:///home/gm/.local/share/Trash/files/special.txt trash::deletion-date=2025-06-13T09:30:00\n";
const DATES: &str = "trash:///old-draft.txt\t13\t(regular)\ttrash::deletion-date=2025-06-14T09:30:00\ntrash:///special.txt\t17\t(regular)\ttrash::deletion-date=2025-06-13T09:30:00\n";

#[test]
fn cifix_vident_real_gio_columns_keep_original_paths_in_two_batched_reads() {
    struct NoMounts;
    impl Drop for NoMounts {
        fn drop(&mut self) {
            crate::backend::topdirtrash::test_set_mount_body(None);
        }
    }
    crate::backend::topdirtrash::test_set_mount_body(Some(String::new()));
    let guard = NoMounts;
    let calls = std::cell::Cell::new(0);
    let items = list_with(|args| {
        calls.set(calls.get() + 1);
        let body = match args {
            ["list", "-u", "-h", "-a", "trash::orig-path", "trash:///"] => ORIGINALS,
            ["list", "-u", "-h", "-a", "trash::orig-path,standard::target-uri", "trash:///"] => {
                JOINED_ORIGINALS
            }
            ["list", "-u", "-h", "-a", "standard::target-uri,trash::deletion-date", "trash:///"] => {
                METADATA
            }
            ["list", "-u", "-h", "-a", "trash::deletion-date", "trash:///"] => DATES,
            _ => panic!("unexpected gio request: {args:?}"),
        };
        Ok(body.to_string())
    })
    .unwrap();
    drop(guard);
    assert_eq!(calls.get(), 2, "listing stays batched rather than per item");
    assert_eq!(
        items.len(),
        2,
        "real GIO fields must not drop valid entries"
    );
    assert_eq!(items[0].uri, "trash:///old-draft.txt");
    assert_eq!(items[0].original, "/fx/v/work/old-draft.txt");
    assert_eq!(
        items[1].original, "/fx/v/work/report trash::deletion-date=literal.txt",
        "attribute-like filename text stays part of the original path"
    );
}

#[test]
fn cifix_vident_dates_accept_space_joined_metadata_fields() {
    let dates = parse_dates(METADATA);
    assert_eq!(dates.len(), 2);
    assert_eq!(dates["trash:///old-draft.txt"], "2025-06-14T09:30:00");
    assert_eq!(dates["trash:///special.txt"], "2025-06-13T09:30:00");
}

#[test]
fn cifix_vident_failed_dates_retry_backings_without_silently_duplicating_mount_rows() {
    let calls = std::cell::Cell::new(0);
    let backing = topdir::metadata(&|args| {
        calls.set(calls.get() + 1);
        match args[4] {
            "standard::target-uri,trash::deletion-date" => Err(unreadable("dates unavailable")),
            "standard::target-uri" => Ok(METADATA
                .lines()
                .map(|line| line.split(" trash::deletion-date=").next().unwrap())
                .collect::<Vec<_>>()
                .join("\n")),
            _ => panic!("unexpected metadata query: {args:?}"),
        }
    })
    .unwrap();
    assert_eq!(calls.get(), 2);
    assert!(backing.contains("standard::target-uri=file:///"));
    assert!(parse_dates(&backing).is_empty());
    calls.set(0);
    assert!(topdir::metadata(&|_| {
        calls.set(calls.get() + 1);
        Err(unreadable("provider unavailable"))
    })
    .is_err());
    assert_eq!(calls.get(), 2, "metadata failure has one bounded retry");
    calls.set(0);
    let listed = list_with(|args| {
        calls.set(calls.get() + 1);
        match args[4] {
            "trash::orig-path" => Ok(ORIGINALS.to_string()),
            _ => Err(unreadable("provider unavailable")),
        }
    });
    assert!(
        listed.is_err(),
        "without backing URIs the listing refuses rather than adding mount rows twice"
    );
    assert_eq!(
        calls.get(),
        3,
        "the original paths, then the metadata and its retry"
    );
}
