use super::*;

fn ordinary(xy: &str, path: &str) -> String {
    format!("1 {xy} N... 100644 100644 100644 a b {path}\0")
}

#[test]
fn porcelain_letters_folder_ancestors_and_deleted_paths() {
    let mut bytes = b"# branch.oid abc\0# branch.head main\0".to_vec();
    for (xy, path) in [
        (".M", "src/lib.rs"),
        ("A.", "added.txt"),
        (".D", "gone.txt"),
    ] {
        bytes.extend_from_slice(ordinary(xy, path).as_bytes());
    }
    bytes.extend_from_slice(
        b"2 R. N... 100644 100644 100644 a b R100 new name\0old/name\0? untracked\0",
    );
    let snapshot = parse(&bytes).unwrap();
    assert_eq!(snapshot.label(), "main · 5 changed");
    for (path, letter) in [
        ("src/lib.rs", 'M'),
        ("added.txt", 'A'),
        ("gone.txt", 'D'),
        ("new name", 'A'),
        ("untracked", '?'),
    ] {
        assert_eq!(snapshot.mark(Path::new(path)), (Some(letter), false));
    }
    assert_eq!(snapshot.mark(Path::new("src")), (None, true));
    assert_eq!(snapshot.mark(Path::new("old")), (None, true));
    assert_eq!(snapshot.mark(Path::new("clean")), (None, false));
    assert_eq!(
        snapshot
            .changes
            .iter()
            .find(|change| change.original.is_some())
            .unwrap()
            .original
            .as_deref(),
        Some(Path::new("old/name"))
    );
}

#[test]
fn renames_and_copies_use_the_hig_added_letter_and_keep_the_original_path() {
    for status in ["R", "C"] {
        let bytes = format!(
            "# branch.head main\x002 {status}. N... 100644 100644 100644 a b {status}100 new\0old\0"
        );
        let snapshot = parse(bytes.as_bytes()).unwrap();
        assert_eq!(snapshot.mark(Path::new("new")), (Some('A'), false));
        assert_eq!(
            snapshot.changes[0].original.as_deref(),
            Some(Path::new("old"))
        );
        assert_eq!(snapshot.label(), "main · 1 changed");
    }
}

#[test]
fn a_deleted_flag_in_a_rename_or_copy_record_takes_precedence_over_added() {
    let cases = [("RD", "000000", 'D'), ("CD", "000000", 'D'), ("RM", "100644", 'A'), ("CM", "100644", 'A')];
    for (xy, worktree, letter) in cases {
        let kind = &xy[..1];
        let bytes = format!("# branch.head main\x002 {xy} N... 100644 100644 {worktree} a b {kind}100 new\0old\0");
        assert_eq!(parse(bytes.as_bytes()).unwrap().mark(Path::new("new")), (Some(letter), false), "{xy}");
    }
}

#[test]
fn hig_eight_changed_paths_match_the_branch_caption_and_folder_marks() {
    let mut bytes = b"# branch.head main\0".to_vec();
    for (status, path) in [
        (".D", "Code/lib.rs"),
        ("A.", "Docs/budget.xlsx"),
        ("A.", "Docs/garden-report.docx"),
        ("A.", "Docs/garden-report.pdf"),
        ("A.", "Docs/lease.docx"),
        ("A.", "Docs/talk.odp"),
        (".M", "notes.txt"),
    ] {
        bytes.extend_from_slice(ordinary(status, path).as_bytes());
    }
    bytes.extend_from_slice(b"? todo.md\0");
    let snapshot = parse(&bytes).unwrap();
    assert_eq!(snapshot.label(), "main · 8 changed");
    assert_eq!(snapshot.mark(Path::new("Code")), (None, true));
    assert_eq!(snapshot.mark(Path::new("Docs")), (None, true));
    for (path, letter) in [
        ("Code/lib.rs", 'D'),
        ("Docs/budget.xlsx", 'A'),
        ("notes.txt", 'M'),
        ("todo.md", '?'),
    ] {
        assert_eq!(snapshot.mark(Path::new(path)), (Some(letter), false));
    }
    assert!(snapshot
        .changes
        .iter()
        .all(|change| "MAD?".contains(change.status)));
}

#[test]
fn porcelain_preserves_newlines_spaces_and_non_utf8_without_quoting() {
    let snapshot = parse(b"# branch.head main\0? dir/space and\nnewline\0? dir/\xff\0").unwrap();
    assert_eq!(snapshot.changes.len(), 2);
    assert_eq!(
        snapshot.changes[0].path,
        Path::new("dir/space and\nnewline")
    );
    assert_eq!(
        snapshot.changes[1].path,
        PathBuf::from(OsString::from_vec(b"dir/\xff".to_vec()))
    );
    assert_eq!(snapshot.mark(Path::new("dir")), (None, true));
}

#[test]
fn detached_unborn_submodule_and_unmerged_records() {
    assert_eq!(
        parse(b"# branch.head (detached)\0").unwrap().label(),
        "detached · 0 changed"
    );
    let snapshot = parse(b"# branch.oid (initial)\0# branch.head main\0u UU N... 100644 100644 100644 100644 a b c conflict\0").unwrap();
    assert_eq!(snapshot.mark(Path::new("conflict")).0, Some('M'));
    let snapshot =
        parse(b"# branch.head main\x001 .M S.M. 160000 160000 160000 a b module\0").unwrap();
    assert_eq!(snapshot.mark(Path::new("module")), (Some('M'), true));
    assert_eq!(
        parse(b"# branch.head main\0? embedded/\0")
            .unwrap()
            .mark(Path::new("embedded")),
        (Some('?'), true)
    );
}

#[test]
fn malformed_truncated_and_escaping_status_never_becomes_a_partial_snapshot() {
    for bytes in [
        b"# branch.head main".as_slice(),
        b"? a\0",
        b"# branch.head main\0? ../outside\0",
        b"# branch.head main\0? /absolute\0",
        b"# branch.head main\x002 R. N... 1 1 1 a b R100 new\0",
        b"# branch.head main\x001 short\0",
        b"# branch.head main\0x unknown\0",
    ] {
        assert!(parse(bytes).is_err(), "invalid status: {bytes:?}");
    }
    for length in 0..70 {
        let bytes = vec![b'x'; length];
        assert!(std::panic::catch_unwind(|| parse(&bytes)).is_ok());
    }
}

#[test]
fn git_folder_prefix_index_respects_components_and_rename_sources() {
    let snapshot = parse(b"# branch.head main\0? abc/file\0? ab/file\0? a/file\x002 R. N... 1 1 1 a b R100 new/leaf\0old/leaf\0? leaf\0").unwrap();
    for path in ["a", "ab", "abc", "old", "new"] {
        assert_eq!(snapshot.mark(Path::new(path)), (None, true));
    }
    for path in ["", "abcd", "old/lea", "new/leaf/child", "other"] {
        assert_eq!(snapshot.mark(Path::new(path)), (None, false));
    }
    assert_eq!(snapshot.mark(Path::new("leaf")), (Some('?'), false));
    assert_eq!(snapshot.mark(Path::new("old/leaf")), (None, false));
    let paths: Vec<_> = snapshot
        .changes
        .iter()
        .map(|change| change.path.as_path())
        .collect();
    assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn git_deep_paths_keep_only_endpoints_and_refuse_duplicate_records() {
    let path = format!("root/{}leaf", "a/".repeat(16384));
    let snapshot = parse(format!("# branch.head main\0? {path}\0").as_bytes()).unwrap();
    assert_eq!(snapshot.endpoints.len(), 1);
    assert_eq!(snapshot.changes[0].path.as_os_str().len(), path.len());
    assert_eq!(snapshot.mark(Path::new("root/a/a")), (None, true));
    assert_eq!(snapshot.mark(Path::new(&path)), (Some('?'), false));
    assert_eq!(
        parse(b"# branch.head main\0? same\0? same\0").unwrap_err(),
        "Git status returned a duplicate path."
    );
}

#[test]
fn git_index_removal_pairs_coalesce_once_independent_of_record_order() {
    let removed = "1 D. N... 100644 000000 000000 a 000000 same\0";
    for untracked in ["? same\0", "? same/\0"] {
        for records in [
            format!("{removed}{untracked}"),
            format!("{untracked}{removed}"),
        ] {
            let snapshot = parse(format!("# branch.head main\0{records}").as_bytes()).unwrap();
            assert_eq!(snapshot.label(), "main · 1 changed");
            assert_eq!(
                snapshot.mark(Path::new("same")),
                (Some('?'), untracked.contains('/'))
            );
            assert_eq!(snapshot.endpoints.len(), 1);
            assert!(snapshot.changes[0].original.is_none());
        }
    }
    let bytes = format!("# branch.head main\0? before\0{removed}? same\0? z/after\0");
    let snapshot = parse(bytes.as_bytes()).unwrap();
    assert_eq!(snapshot.label(), "main · 3 changed");
    assert!(snapshot
        .changes
        .windows(2)
        .all(|pair| pair[0].path < pair[1].path));
    assert_eq!(snapshot.mark(Path::new("z")), (None, true));
}

#[test]
fn git_impossible_record_pairs_and_triples_still_refuse_the_whole_snapshot() {
    let deleted = "1 D. N... 100644 000000 000000 a 000000 same\0";
    let untracked = "? same\0";
    for other in [
        ordinary(".D", "same"),
        ordinary("M.", "same"),
        ordinary("A.", "same"),
        ordinary("D.", "same"),
        "2 R. N... 100644 100644 100644 a b R100 same\0old\0".into(),
        "2 RD N... 100644 100644 000000 a b R100 same\0old\0".into(),
        "u UD N... 100644 100644 000000 100644 a b 0 same\0".into(),
        untracked.into(),
    ] {
        for records in [format!("{other}{untracked}"), format!("{untracked}{other}")] {
            assert_eq!(
                parse(format!("# branch.head main\0{records}").as_bytes()).unwrap_err(),
                "Git status returned a duplicate path."
            );
        }
    }
    for records in [
        format!("{deleted}{deleted}"),
        format!("{deleted}{untracked}{untracked}"),
        format!("{deleted}{untracked}{deleted}"),
    ] {
        assert_eq!(
            parse(format!("# branch.head main\0{records}").as_bytes()).unwrap_err(),
            "Git status returned a duplicate path."
        );
    }
}
