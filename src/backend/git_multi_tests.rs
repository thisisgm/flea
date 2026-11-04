use super::*;
use crate::backend::{gitfixture::Fixture, gitstatus};
use crate::json::{field_bool, field_str, field_usize};

fn ask(session: &mut Session, command: &str, d: &Fixture, extra: &str) -> String {
    session.answer(&format!(
        "{{\"c\":{},\"path\":{},\"id\":22{extra}}}",
        quote(command),
        quote(&d.0.to_string_lossy())
    ))
}

#[test]
fn real_git_index_deletions_pair_with_retained_recreated_symlink_and_gitlink_entries() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("dir/retained.txt", b"retained\n");
    d.write("dir/recreated.txt", b"original\n");
    std::os::unix::fs::symlink("retained.txt", d.path("dir/link")).unwrap();
    d.write("module/inside", b"nested repository\n");
    let module = d.path("module");
    let module = module.to_str().unwrap();
    d.git(&["-C", module, "init", "--initial-branch=main", "--quiet"]);
    d.git(&["-C", module, "add", "--", "."]);
    d.git(&[
        "-C",
        module,
        "commit",
        "--quiet",
        "-m",
        "fixture",
        "--author=GM <gianmarcomorales@icloud.com>",
    ]);
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    d.git(&[
        "rm",
        "--cached",
        "--",
        "dir/retained.txt",
        "dir/link",
        "module",
    ]);
    let recreated = d.path("dir/recreated.txt");
    d.git(&["rm", "--", recreated.to_str().unwrap()]);
    d.write("dir/recreated.txt", b"recreated\n");
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let bytes = gitprobe::capture(&gitprobe::argv(&repo), gitprobe::DEADLINE).unwrap();
    println!("GIT_MULTI porcelain={:?}", String::from_utf8_lossy(&bytes));
    let records: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    for path in [
        "dir/retained.txt",
        "dir/recreated.txt",
        "dir/link",
        "module",
    ] {
        assert_eq!(
            records
                .iter()
                .filter(|record| record.starts_with(b"1 D.") && record.ends_with(path.as_bytes()))
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| record
                    .strip_prefix(b"? ")
                    .is_some_and(|raw| raw.strip_suffix(b"/").unwrap_or(raw) == path.as_bytes()))
                .count(),
            1
        );
    }
    let snapshot = gitstatus::parse(&bytes).unwrap();
    assert_eq!(snapshot.label(), "main · 4 changed");
    assert_eq!(snapshot.mark(Path::new("dir")), (None, true));
    for path in [
        "dir/retained.txt",
        "dir/recreated.txt",
        "dir/link",
        "module",
    ] {
        assert_eq!(
            snapshot.mark(Path::new(path)),
            (Some('?'), path == "module")
        );
    }
    let mut session = Session::default();
    let status = ask(&mut session, "gitstatus", &d, ",\"enabled\":true");
    assert!(field_bool(&status, "ok"), "{status}");
    assert_eq!(field_usize(&status, "changed"), Some(4));
    assert_eq!(
        field_str(&status, "label").as_deref(),
        Some("main · 4 changed")
    );
    let rows = ask(
        &mut session,
        "gitrows",
        &d,
        &format!(
            ",\"paths\":[{},{}]",
            quote(&d.path("dir").to_string_lossy()),
            quote(&d.path("module").to_string_lossy())
        ),
    );
    let rows = crate::jsondoc::parse(&rows).unwrap();
    let rows = rows.get("marks").unwrap().as_array().unwrap();
    assert_eq!(rows[0].get("folder").unwrap().as_bool(), Some(true));
    assert_eq!(rows[1].get("status").unwrap().as_str(), Some("?"));
    assert_eq!(rows[1].get("folder").unwrap().as_bool(), Some(true));
    let page = ask(&mut session, "gitchanges", &d, ",\"start\":1,\"count\":2");
    assert_eq!(field_usize(&page, "total"), Some(4));
    let page = crate::jsondoc::parse(&page).unwrap();
    let entries = page.get("entries").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].get("path").unwrap().as_str(),
        Some("dir/recreated.txt")
    );
    assert_eq!(
        entries[1].get("path").unwrap().as_str(),
        Some("dir/retained.txt")
    );
    for entry in entries {
        assert_eq!(entry.get("status").unwrap().as_str(), Some("?"));
        assert_eq!(entry.get("deleted").unwrap().as_bool(), Some(false));
    }
}

#[test]
fn real_git_rename_source_recreation_keeps_two_distinct_changed_paths() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("old.txt", b"rename this file\n");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let old = d.path("old.txt");
    let new = d.path("new.txt");
    d.git(&["mv", "--", old.to_str().unwrap(), new.to_str().unwrap()]);
    d.write("old.txt", b"new untracked file\n");
    let repo = gitprobe::discover(&d.0).unwrap().unwrap();
    let bytes = gitprobe::capture(&gitprobe::argv(&repo), gitprobe::DEADLINE).unwrap();
    println!(
        "GIT_RENAME_RECREATED porcelain={:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(bytes
        .split(|byte| *byte == 0)
        .any(|record| record.starts_with(b"2 R.")));
    let snapshot = gitprobe::status(&repo).unwrap();
    assert_eq!(snapshot.label(), "main · 2 changed");
    assert_eq!(snapshot.mark(Path::new("old.txt")), (Some('?'), false));
    assert_eq!(snapshot.mark(Path::new("new.txt")), (Some('A'), false));
    assert_eq!(
        snapshot.changes[0].original.as_deref(),
        Some(Path::new("old.txt"))
    );
}
