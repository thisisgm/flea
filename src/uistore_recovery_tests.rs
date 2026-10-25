use super::*;

#[test]
fn language_marks_heal_on_and_saved_off_survives_settle() {
    let d = TestDir::new("uistore-langmarks");
    let s = store(&d);
    assert_eq!(
        s.read().get("langMarks").and_then(Json::as_bool),
        Some(true)
    );
    fs::create_dir_all(s.file().parent().expect("state dir")).expect("state dir");
    fs::write(s.file(), r#"{"langMarks":"false"}"#).expect("invalid preference");
    s.settle().expect("heal before GUI launch");
    let healed = fs::read_to_string(s.file()).expect("healed document");
    assert_eq!(
        jsondoc::parse(&healed)
            .expect("JSON")
            .get("langMarks")
            .and_then(Json::as_bool),
        Some(true)
    );
    fs::write(s.file(), r#"{"langMarks":false}"#).expect("saved off");
    s.settle().expect("settle saved off");
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("unrelated preference");
    let kept = fs::read_to_string(s.file()).expect("kept document");
    assert_eq!(
        jsondoc::parse(&kept)
            .expect("JSON")
            .get("langMarks")
            .and_then(Json::as_bool),
        Some(false)
    );
}

#[test]
fn a_malformed_file_reads_the_full_default_shape_rather_than_throwing() {
    let d = TestDir::new("uistore-malformed");
    let s = store(&d);
    fs::create_dir_all(s.file().parent().expect("parent")).expect("state dir");
    fs::write(s.file(), "{ this is not json").expect("write");
    assert_eq!(jsondoc::render(&s.read()), jsondoc::render(&defaults()));
}

#[test]
fn malformed_recovery_atomically_keeps_one_exact_backup() {
    let d = TestDir::new("uistore-backup");
    let s = store(&d);
    d.assert_contains(s.file());
    s.update(&patch(r#"{"hidden":true}"#)).expect("seed");
    let backup = s.file().with_file_name("ui.json.broken");
    assert!(!backup.exists(), "valid writes do not create a backup");
    let mut previous = "previous malformed state".to_string();
    fs::write(s.file(), &previous).expect("first malformed state");
    s.update(&patch(r#"{"hidden":true}"#))
        .expect("create first backup");
    assert_eq!(fs::read_to_string(&backup).expect("first backup"), previous);
    for original in [
        "{\"places\":\n",
        "",
        "17\n",
        "[1]\n",
        "{\"label\":\"caf\u{e9}\"\n",
    ] {
        fs::write(s.file(), original).expect("malformed state");
        s.settle().expect("startup retains original");
        assert_eq!(fs::read_to_string(s.file()).expect("original"), original);
        assert_eq!(fs::read_to_string(&backup).expect("old backup"), previous);
        assert!(s.update(&patch(r#"{"notASetting":true}"#)).is_err());
        assert_eq!(
            fs::read_to_string(&backup).expect("refused patch backup"),
            previous
        );
        let mut held = fs::File::open(&backup).expect("hold old backup");
        s.update(&patch(r#"{"hidden":true}"#))
            .expect("native writer recovery");
        assert_eq!(fs::read_to_string(&backup).expect("exact backup"), original);
        let mut held_bytes = String::new();
        held.read_to_string(&mut held_bytes)
            .expect("read held backup");
        assert_eq!(
            held_bytes, previous,
            "backup replacement must not truncate old inode"
        );
        assert_eq!(
            fs::metadata(&backup)
                .expect("backup mode")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(s.read().get("hidden").and_then(Json::as_bool), Some(true));
        s.update(&patch(r#"{"hidden":false}"#))
            .expect("valid later write");
        assert_eq!(
            fs::read_to_string(&backup).expect("backup survives valid write"),
            original
        );
        previous = original.to_string();
    }
    let mut files: Vec<_> = fs::read_dir(backup.parent().expect("parent"))
        .expect("directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    assert_eq!(files, ["ui.json", "ui.json.broken", "ui.json.lock"]);
}

#[test]
fn unavailable_backup_preserves_original_and_previous_backup() {
    for blocked in ["directory", "symlink", "temporary"] {
        let d = TestDir::new("uistore-backup-refusal");
        let s = store(&d);
        d.assert_contains(s.file());
        s.update(&patch(r#"{"hidden":true}"#)).expect("seed");
        let original = "{\"places\":\n";
        fs::write(s.file(), original).expect("malformed state");
        let backup = s.file().with_file_name("ui.json.broken");
        let kept = d.file("previous-backup", "previous original bytes");
        match blocked {
            "directory" => fs::create_dir(&backup).expect("blocked backup"),
            "symlink" => std::os::unix::fs::symlink(&kept, &backup).expect("linked backup"),
            _ => {
                fs::copy(&kept, &backup).expect("prior backup");
                let tmp = PathBuf::from(format!("{}.{}.tmp", backup.display(), std::process::id()));
                fs::create_dir(tmp).expect("blocked exclusive temp");
            }
        }
        let error = s
            .update(&patch(r#"{"hidden":true}"#))
            .expect_err("backup must succeed first");
        assert!(error.contains("ui.json.broken"), "{}: {}", blocked, error);
        assert_eq!(
            fs::read_to_string(s.file()).expect("untouched original"),
            original
        );
        assert_eq!(
            fs::read_to_string(&kept).expect("kept target"),
            "previous original bytes"
        );
        if blocked != "directory" {
            assert_eq!(
                fs::read_to_string(&backup).expect("untouched backup"),
                "previous original bytes"
            );
        }
    }
}

#[test]
fn foreign_and_newer_objects_and_previous_backups_survive_settle_and_write() {
    let d = TestDir::new("uistore-preserve");
    let s = store(&d);
    fs::create_dir_all(s.file().parent().expect("parent")).expect("state dir");
    let backup = s.file().with_file_name("ui.json.broken");
    fs::write(&backup, "earlier original").expect("previous backup");
    // A malformed file is backed up and replaced as 0.3.12 did; an object this version cannot read is not.
    for original in [
        r#"{"foreign":true}"#,
        r#"{"stateVersion":999,"view":"grid"}"#,
    ] {
        fs::write(s.file(), original).expect("unsupported state");
        assert_eq!(jsondoc::render(&s.read()), jsondoc::render(&defaults()));
        s.settle().expect("launch uses defaults without writing");
        assert_eq!(
            fs::read_to_string(s.file()).expect("after launch"),
            original
        );
        let why = s
            .update(&patch(r#"{"hidden":true}"#))
            .expect_err("write is refused");
        assert!(why.contains("left unchanged"), "{}", why);
        assert_eq!(
            fs::read_to_string(s.file()).expect("after write attempt"),
            original
        );
        assert_eq!(
            fs::read_to_string(&backup).expect("previous backup"),
            "earlier original"
        );
    }
}
