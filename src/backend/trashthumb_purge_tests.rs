use super::super::*;
use crate::backend::testdir::TestDir;
use crate::backend::thumbcache::{uri_for, Cache};
use std::path::Path;

fn write(home: &Path, name: &str, body: &[u8]) -> PathBuf {
    let path = home.join(name);
    assert!(home.is_absolute() && home.join(".flea-test-sandbox").is_file());
    assert!(
        path.is_absolute() && path.starts_with(home) && !name.split('/').any(|part| part == "..")
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut file, body).unwrap();
    path
}

fn fixture(home: &Path) -> (Vec<PathBuf>, Vec<Vec<PathBuf>>, Vec<PathBuf>) {
    let cache = Cache::new();
    let mut payloads = Vec::new();
    let mut remove = Vec::new();
    let mut keep = Vec::new();
    for name in ["a photo.png", "z other.png"] {
        let original = home.join("original").join(name);
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        let path = write(home, &format!("data/Trash/files/{name}"), b"image bytes");
        write(
            home,
            &format!("data/Trash/info/{name}.trashinfo"),
            format!(
                "[Trash Info]\nPath={}\nDeletionDate=2026-10-18T09:00:00\n",
                &uri_for(&original)[7..]
            )
            .as_bytes(),
        );
        let uri = uri_for(&path);
        let mut owned = Vec::new();
        for identity in ["old-reviewed-object", "current-reviewed-object"] {
            for file in [
                cache.large_path_for(&uri, Some(identity)),
                cache.fail_path_for(&uri, Some(identity)),
            ] {
                let relative = file.strip_prefix(home).unwrap().to_str().unwrap();
                write(home, relative, b"owned thumbnail");
                owned.push(file);
            }
        }
        for file in [cache.large_path(&uri), cache.fail_path(&uri)] {
            write(
                home,
                file.strip_prefix(home).unwrap().to_str().unwrap(),
                b"keep",
            );
            keep.push(file);
        }
        let hash = crate::backend::md5::hex(uri.as_bytes());
        for dir in [
            cache.large_dir(),
            cache.fail_path(&uri).parent().unwrap().to_path_buf(),
        ] {
            for name in [
                format!("trash-{hash}-old.txt"),
                format!("trash-{hash}0-old.png"),
                format!("unrelated-trash-{hash}-old.png"),
            ] {
                let file = dir.join(name);
                write(
                    home,
                    file.strip_prefix(home).unwrap().to_str().unwrap(),
                    b"keep",
                );
                keep.push(file);
            }
        }
        payloads.push(path);
        remove.push(owned);
    }
    for file in [
        cache.large_path_for("file:///unrelated", Some("identity")),
        cache.fail_path_for("file:///unrelated", Some("identity")),
    ] {
        write(
            home,
            file.strip_prefix(home).unwrap().to_str().unwrap(),
            b"keep",
        );
        keep.push(file);
    }
    (payloads, remove, keep)
}

fn run_case(case: &str, test: &str) {
    if std::env::var("FLEA_TRASH_PURGE_CASE").ok().as_deref() != Some(case) {
        let d = TestDir::new("trashthumb-purge-home");
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(d.join("home.XXXXXXXX"))
            .output()
            .unwrap();
        assert!(made.status.success());
        let home = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        d.assert_contains(&home);
        std::fs::write(home.join(".flea-test-sandbox"), b"private test HOME\n").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        let test = test.split_once("::").unwrap().1;
        child
            .args(["--exact", test, "--nocapture"])
            .env("FLEA_TRASH_PURGE_CASE", case)
            .env("HOME", &home)
            // Checked in, not written into HOME: package check() keeps TMPDIR on noexec /dev/shm.
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("src/backend/trashbrowse/fixtures/bin")
                        .display(),
                    std::env::var("PATH").unwrap()
                ),
            );
        for (key, name) in [
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_RUNTIME_DIR", "run"),
        ] {
            let path = home.join(name);
            d.assert_contains(&path);
            std::fs::create_dir_all(&path).unwrap();
            child.env(key, path);
        }
        let result = child.output().unwrap();
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("running 1 test"),
            "the private-HOME test must execute: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(
            result.status.success(),
            "private HOME {case} failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    assert!(home.is_absolute() && home.join(".flea-test-sandbox").is_file());
    let (payloads, owned, keep) = fixture(&home);
    // Beside the two listed items: a file with no record, one whose record has no Path, a record with no file and a file that is no record.
    let empty = case == "empty" || case == "sweep";
    let left_out = [
        write(&home, "data/Trash/files/no record.txt", b"x"),
        write(&home, "data/Trash/files/no path.txt", b"x"),
        write(
            &home,
            "data/Trash/info/no path.txt.trashinfo",
            b"[Trash Info]\nDeletionDate=2026-10-18T09:00:00\n",
        ),
        write(
            &home,
            "data/Trash/info/gone.txt.trashinfo",
            b"[Trash Info]\nPath=/gone.txt\nDeletionDate=2026-10-18T09:00:00\n",
        ),
    ];
    let notes = write(&home, "data/Trash/info/notes", b"x");
    if case != "sweep" {
        for path in &left_out {
            std::fs::remove_file(path).unwrap();
        }
    }
    let scratch = home.join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut session = Session {
        scratch,
        ..Session::default()
    };
    let listed = session.handle(r#"{"c":"trashbrowse","op":"list","id":1}"#);
    assert!(crate::json::field_bool(&listed, "ok"), "{listed}");
    assert_eq!(crate::json::field_usize(&listed, "total"), Some(2));
    let first = session
        .items
        .iter()
        .find(|item| item.uri.contains("a%20photo.png"))
        .unwrap();
    let current = detail(first).unwrap();
    let selection = format!(
        r#""uris":["{}"],"identities":["{}"]"#,
        first.uri,
        escape(&selection_identity(&current))
    );
    let cancelled = case == "cancel-empty";
    let batch = case.starts_with("batch-") || case.starts_with("partial-") || empty || cancelled;
    let partial = case.starts_with("partial-");
    let restoring = matches!(
        case,
        "restore" | "blocked" | "batch-restore" | "partial-restore"
    );
    if case == "blocked" || case == "partial-restore" {
        write(&home, "original/a photo.png", b"destination bytes");
    }
    crate::backend::trashthumbcache::DIRECTORY_READS.with(|reads| reads.set([0; 2]));
    crate::backend::trashdelete::FolderSyncs::take_synced();
    let result = if restoring {
        let target = if batch { r#""all":true"# } else { &selection };
        session.handle(&format!(
            r#"{{"c":"trashbrowse","op":"restore","id":2,{target}}}"#
        ))
    } else {
        let target = if empty || batch {
            r#""all":true,"emptyTrash":true"#
        } else {
            &selection
        };
        let prepared = session.handle(&format!(
            r#"{{"c":"trashbrowse","op":"prepare","id":2,{target}}}"#
        ));
        assert!(crate::json::field_bool(&prepared, "ok"), "{prepared}");
        let token = crate::json::field_usize(&prepared, "token").unwrap();
        if cancelled {
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let cancellation = Cancellation::operation(flag.clone());
            let fields = job::delete_with(
                session.confirmation.as_ref().unwrap(),
                &crate::backend::trashdelete::recovery_root(),
                &cancellation,
                |done, item, _| {
                    if done == 1 && item.is_none() {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                },
            );
            let result = format!(r#"{{"ok":true,{fields}}}"#);
            assert!(field_bool(&result, "cancelled"), "{result}");
            assert_eq!(field_usize(&result, "skipped"), Some(1), "{result}");
            result
        } else if case == "partial-delete" {
            let kept = home.join("reviewed-payload");
            for path in [&payloads[0], &kept] {
                assert!(home.is_absolute() && home.join(".flea-test-sandbox").is_file());
                assert!(path.is_absolute() && path.starts_with(&home));
            }
            std::fs::rename(&payloads[0], &kept).unwrap();
            write(&home, "data/Trash/files/a photo.png", b"replacement bytes");
            // Model a change after the session's last validation, inside the per-item delete loop.
            let fields = delete_confirmed(
                session.confirmation.as_ref().unwrap(),
                &crate::backend::trashdelete::recovery_root(),
            );
            assert_eq!(std::fs::read(&kept).unwrap(), b"image bytes");
            format!(r#"{{"ok":true,{fields}}}"#)
        } else {
            session.handle(&format!(
                r#"{{"c":"trashbrowse","op":"delete","id":3,"token":{token}}}"#
            ))
        }
    };
    assert!(crate::json::field_bool(&result, "ok"), "{result}");
    assert_eq!(
        crate::json::field_usize(&result, "done"),
        Some(if case == "blocked" {
            0
        } else if !cancelled && (empty || (batch && !partial)) {
            2
        } else {
            1
        }),
        "{result}"
    );
    assert_eq!(
        crate::json::field_usize(&result, "failed"),
        Some(usize::from(case == "blocked" || partial)),
        "{result}"
    );
    for (index, files) in owned.iter().enumerate() {
        let refused = index == 0 && (case == "blocked" || partial);
        let gone = !refused && (index == 0 || (!cancelled && batch));
        assert_eq!(payloads[index].exists(), !gone);
        for file in files {
            assert_eq!(
                file.exists(),
                !gone,
                "{case}: wrong cache lifetime for {}",
                file.display()
            );
        }
    }
    for file in keep {
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"keep",
            "unrelated cache entry changed: {}",
            file.display()
        );
    }
    if restoring {
        for (index, payload) in payloads.iter().enumerate() {
            if !payload.exists() {
                assert_eq!(
                    std::fs::read(home.join("original").join(payload.file_name().unwrap()))
                        .unwrap(),
                    b"image bytes"
                );
            } else if index == 0 && (case == "blocked" || partial) {
                assert_eq!(
                    std::fs::read(home.join("original/a photo.png")).unwrap(),
                    b"destination bytes"
                );
            }
        }
    }
    for path in left_out.iter().filter(|_| case == "sweep") {
        assert!(
            !path.exists(),
            "Empty Trash removes what the listing left out: {}",
            path.display()
        );
    }
    assert!(notes.exists(), "{case}: a file that is no record stays");
    if case == "batch-restore" {
        let mut once = vec![
            home.join("original"),
            std::fs::canonicalize(home.join("data/Trash/files")).unwrap(),
            home.join("state/flea/recovery"),
        ];
        once.sort();
        assert_eq!(
            crate::backend::trashdelete::FolderSyncs::take_synced(),
            once,
            "Restore all syncs each folder once, not once per item"
        );
    }
    if batch {
        assert_eq!(
            crate::backend::trashthumbcache::DIRECTORY_READS.with(|reads| reads.get()),
            [1, 1],
            "one request must read large and fail/flea exactly once"
        );
    }
}

macro_rules! purge_case {
    ($name:ident, $case:literal) => {
        #[test]
        fn $name() {
            run_case($case, concat!(module_path!(), "::", stringify!($name)));
        }
    };
}
purge_case!(
    advloop_f5_refused_restore_keeps_its_owned_thumbnails,
    "blocked"
);
purge_case!(
    advloop_f5_restore_purges_only_its_owned_thumbnails,
    "restore"
);
purge_case!(
    advloop_f5_permanent_delete_purges_only_its_owned_thumbnails,
    "delete"
);
purge_case!(
    advloop_f5_empty_trash_purges_only_its_owned_thumbnails,
    "empty"
);
purge_case!(
    advloop_f7_multi_delete_reads_each_cache_directory_once,
    "batch-delete"
);
purge_case!(
    advloop_f7_failed_delete_keeps_its_thumbnails,
    "partial-delete"
);
purge_case!(
    advloop_f7_multi_restore_reads_each_cache_directory_once,
    "batch-restore"
);
purge_case!(
    advloop_f7_failed_restore_keeps_its_thumbnails,
    "partial-restore"
);
purge_case!(
    cancelled_empty_trash_purges_only_removed_items_once,
    "cancel-empty"
);
purge_case!(
    empty_trash_removes_what_the_listing_left_out_as_gio_does,
    "sweep"
);
