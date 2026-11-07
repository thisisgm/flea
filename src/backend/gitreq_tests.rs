use super::*;
use crate::backend::gitfixture::Fixture;
use crate::json::{field_bool, field_str, field_usize};

fn request(command: &str, path: &Path, extra: &str) -> String {
    format!(
        "{{\"c\":{},\"path\":{},\"id\":7{extra}}}",
        quote(command),
        quote(&path.to_string_lossy())
    )
}

fn enabled(path: &Path) -> String {
    request("gitstatus", path, ",\"enabled\":true")
}

#[test]
fn git_protocol_parses_and_worker_answers_off_without_discovery() {
    let (tx, rx) = crate::backend::events::channel();
    let mut worker = Worker::new(tx);
    for command in ["gitstatus", "gitrows", "gitchanges"] {
        let line = request(command, Path::new("/a/path/that/does/not/exist"), "");
        let super::super::proto::Request::Git { line } = super::super::proto::parse_request(&line)
        else {
            panic!("git request was not parsed")
        };
        worker.request(line);
        let Event::Git(line) = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap() else {
            panic!("wrong event")
        };
        assert_eq!(field_str(&line, "t").as_deref(), Some(command));
        assert_eq!(field_usize(&line, "id"), Some(7));
        assert_eq!(field_bool(&line, "ok"), command == "gitstatus");
        assert!(crate::jsondoc::parse(&line).is_ok());
        if command == "gitstatus" {
            assert!(!field_bool(&line, "enabled"));
        }
    }
}

#[test]
fn discovery_outside_a_repository_does_not_require_the_git_tool_or_sandbox() {
    let d = Fixture::new();
    let mut session = Session::default();
    let reply = session.answer(&enabled(&d.0));
    assert!(field_bool(&reply, "ok"));
    assert_eq!(field_str(&reply, "root").as_deref(), Some(""));
    assert!(session.repo.is_none() && session.snapshot.is_none());
    let failed = session.answer(&enabled(Path::new("relative")));
    assert!(!field_bool(&failed, "ok"));
    assert_eq!(
        session.answer(&enabled(Path::new("relative"))),
        failed,
        "a cached failure must never masquerade as a non-repository"
    );
}

#[test]
fn cached_snapshot_projects_visible_rows_and_pages_without_any_tool() {
    let d = Fixture::new();
    d.write(".git/HEAD", b"ref: refs/heads/main\n");
    d.write("nested/file", b"body\n");
    let directory = d.path("nested");
    let snapshot = super::super::gitstatus::parse(
        b"# branch.head main\x001 .D N... 1 1 1 a b deleted\0? nested/file\0? other/child\0",
    )
    .unwrap();
    let mut session = Session {
        path: directory.to_string_lossy().into_owned(),
        enabled: true,
        repo: gitprobe::discover(&directory).unwrap(),
        snapshot: Some(snapshot),
        error: None,
    };
    let status = session.answer(&enabled(&directory));
    assert_eq!(
        field_str(&status, "label").as_deref(),
        Some("main · 3 changed")
    );
    let rows = session.answer(&request(
        "gitrows",
        &directory,
        &format!(
            ",\"paths\":[{}]",
            quote(&d.path("nested/file").to_string_lossy())
        ),
    ));
    assert!(
        field_bool(&rows, "ok") && rows.contains("\"status\":\"?\""),
        "{rows}"
    );
    let page = session.answer(&request(
        "gitchanges",
        &directory,
        ",\"start\":0,\"count\":1",
    ));
    let parsed = crate::jsondoc::parse(&page).unwrap();
    let entries = parsed.get("entries").unwrap().as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].get("path").unwrap().as_str(), Some("deleted"));
    assert_eq!(entries[0].get("deleted").unwrap().as_bool(), Some(true));
    assert!(entries[0].get("size").is_none() && entries[0].get("mtime").is_none());
    let beyond = session.answer(&request(
        "gitchanges",
        &directory,
        ",\"start\":999999,\"count\":400",
    ));
    assert_eq!(field_usize(&beyond, "start"), Some(3));
    assert!(beyond.contains("\"entries\":[]"));
    assert!(!field_bool(
        &session.answer(&request("gitrows", &d.0, ",\"paths\":[]")),
        "ok"
    ));
    let too_many = vec![quote(&d.path("nested/file").to_string_lossy()); 401].join(",");
    assert!(!field_bool(
        &session.answer(&request(
            "gitrows",
            &directory,
            &format!(",\"paths\":[{too_many}]")
        )),
        "ok"
    ));
    let outside = session.answer(&request(
        "gitrows",
        &directory,
        ",\"paths\":[\"/etc/passwd\"]",
    ));
    assert!(!field_bool(&outside, "ok"));
    let off = session.answer(&request("gitstatus", &directory, ""));
    assert!(field_bool(&off, "ok") && !field_bool(&off, "enabled"));
    assert!(session.snapshot.is_none() && session.repo.is_none());
}

#[test]
fn rows_inside_a_folder_opened_in_place_get_their_marks_and_none_climb_out() {
    let d = Fixture::new();
    d.write(".git/HEAD", b"ref: refs/heads/main\n");
    d.write("nested/deeper/file", b"body\n");
    let directory = d.path("nested");
    let snapshot = super::super::gitstatus::parse(
        b"# branch.head main\x00? nested/deeper/file\0? other/child\0",
    )
    .unwrap();
    let mut session = Session {
        path: directory.to_string_lossy().into_owned(),
        enabled: true,
        repo: gitprobe::discover(&directory).unwrap(),
        snapshot: Some(snapshot),
        error: None,
    };
    let rows = |session: &mut Session, names: &[&str]| {
        let paths: Vec<_> = names
            .iter()
            .map(|name| quote(&format!("{}/{name}", directory.to_string_lossy())))
            .collect();
        session.answer(&request(
            "gitrows",
            &directory,
            &format!(",\"paths\":[{}]", paths.join(",")),
        ))
    };
    let opened = rows(&mut session, &["deeper", "deeper/file"]);
    assert!(field_bool(&opened, "ok"), "{opened}");
    let marks = crate::jsondoc::parse(&opened).unwrap();
    let marks = marks.get("marks").unwrap().as_array().unwrap();
    assert_eq!(marks[0].get("folder").unwrap().as_bool(), Some(true));
    assert_eq!(marks[1].get("status").unwrap().as_str(), Some("?"));
    for escape in ["../other/child", "deeper/../../other/child", ""] {
        let reply = rows(&mut session, &[escape]);
        assert!(!field_bool(&reply, "ok"), "{escape:?} left the directory: {reply}");
    }
}

#[test]
fn real_git_status_cache_refresh_windows_and_changes_include_deleted_and_renamed() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    for name in ["modified", "deleted", "old", "nested/tracked"] {
        d.write(name, b"original\n");
    }
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    d.write("modified", b"new contents\n");
    let deleted = d.path("deleted");
    d.check(&deleted);
    std::fs::remove_file(deleted).unwrap();
    d.git(&["mv", "--", "old", "renamed"]);
    d.write("added", b"staged\n");
    d.git(&["add", "--", "added"]);
    d.write("nested/untracked\nname", b"untracked\n");
    let before = std::fs::read(d.path(".git/index")).unwrap();
    let mut session = Session::default();
    let reply = session.answer(&enabled(&d.0));
    assert!(field_bool(&reply, "ok"), "{reply}");
    assert_eq!(
        field_str(&reply, "label").as_deref(),
        Some("main · 5 changed")
    );
    assert_eq!(
        std::fs::read(d.path(".git/index")).unwrap(),
        before,
        "optional index refresh must stay disabled"
    );
    assert!(!d.path(".git/index.lock").exists());
    let paths = ["modified", "nested", "added", "renamed", "deleted", "clean"]
        .map(|name| quote(&d.path(name).to_string_lossy()))
        .join(",");
    let rows = session.answer(&request("gitrows", &d.0, &format!(",\"paths\":[{paths}]")));
    let parsed = crate::jsondoc::parse(&rows).unwrap();
    let marks = parsed.get("marks").unwrap().as_array().unwrap();
    for (mark, status) in marks.iter().zip(["M", "", "A", "A", "D", ""]) {
        assert_eq!(mark.get("status").unwrap().as_str(), Some(status));
    }
    assert_eq!(marks[1].get("folder").unwrap().as_bool(), Some(true));
    let changes = session.answer(&request("gitchanges", &d.0, ",\"start\":0,\"count\":400"));
    assert_eq!(field_usize(&changes, "total"), Some(5));
    assert!(changes.contains("\"deleted\":true") && changes.contains("nested/untracked\\nname"));
    assert!(changes.contains("\"original\":\"old\""));
    assert!(!changes.contains("\"size\"") && !changes.contains("\"mtime\""));
    let tail = session.answer(&request("gitchanges", &d.0, ",\"start\":4,\"count\":1"));
    assert_eq!(
        crate::jsondoc::parse(&tail)
            .unwrap()
            .get("entries")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    d.write("later", b"new\n");
    assert_eq!(
        session.answer(&enabled(&d.0)),
        reply,
        "same directory reuses the snapshot"
    );
    let refreshed = session.answer(&request(
        "gitstatus",
        &d.0,
        ",\"enabled\":true,\"refresh\":true",
    ));
    assert_eq!(field_usize(&refreshed, "changed"), Some(6));
    let other = session.answer(&request("gitrows", &d.path("nested"), ",\"paths\":[]"));
    assert!(
        !field_bool(&other, "ok"),
        "late requests cannot use another directory's snapshot"
    );
    let outside = session.answer(&request("gitrows", &d.0, ",\"paths\":[\"/etc/passwd\"]"));
    assert!(!field_bool(&outside, "ok"));
    let too_many = vec![quote(&d.path("modified").to_string_lossy()); 401].join(",");
    assert!(!field_bool(
        &session.answer(&request(
            "gitrows",
            &d.0,
            &format!(",\"paths\":[{too_many}]")
        )),
        "ok"
    ));
    let nested = session.answer(&enabled(&d.path("nested")));
    assert_eq!(
        field_usize(&nested, "changed"),
        Some(6),
        "status always covers the repository root"
    );
    let nested_rows = session.answer(&request(
        "gitrows",
        &d.path("nested"),
        &format!(
            ",\"paths\":[{}]",
            quote(&d.path("nested/untracked\nname").to_string_lossy())
        ),
    ));
    assert!(nested_rows.contains("\"status\":\"?\""));
    assert!(field_bool(
        &session.answer(&request("gitstatus", &d.path("nested"), "")),
        "ok"
    ));
    assert!(session.snapshot.is_none() && session.repo.is_none());
}

#[test]
fn linked_worktree_metadata_and_detached_branch_work_in_the_readonly_jail() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("tracked", b"body\n");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let linked = d.path("linked");
    d.git(&[
        "worktree",
        "add",
        "--detach",
        "--",
        linked.to_str().unwrap(),
    ]);
    let repo = gitprobe::discover(&linked).unwrap().unwrap();
    assert_eq!(
        gitprobe::status(&repo).unwrap().label(),
        "detached · 0 changed"
    );
}

#[test]
#[ignore = "10k-file status measurement, run explicitly with --nocapture"]
fn git_10k_first_and_refresh_cost() {
    assert!(Fixture::available(), "the timing needs Git");
    assert!(
        !super::super::sandboxprobe::skipped(),
        "the timing needs the sandbox"
    );
    let d = Fixture::new();
    d.repo();
    for index in 0..10_000 {
        d.write(&format!("file-{index:05}"), b"baseline\n");
    }
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "10k fixture"]);
    d.write("file-00042", b"changed\n");
    let (mut first, mut refresh) = (Vec::new(), Vec::new());
    for _ in 0..3 {
        let mut session = Session::default();
        let start = std::time::Instant::now();
        let reply = session.answer(&enabled(&d.0));
        first.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(field_usize(&reply, "changed"), Some(1), "{reply}");
        let start = std::time::Instant::now();
        let reply = session.answer(&request(
            "gitstatus",
            &d.0,
            ",\"enabled\":true,\"refresh\":true",
        ));
        refresh.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(field_usize(&reply, "changed"), Some(1), "{reply}");
    }
    println!("GIT_10K first_ms={first:.3?} refresh_ms={refresh:.3?}");
}
