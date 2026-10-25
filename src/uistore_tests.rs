use super::*;
use crate::backend::testdir::TestDir;
use crate::jsondoc::{self, Json};
use crate::uischema::defaults;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn store(d: &TestDir) -> Store {
    Store::at(&d.dir("state"), &d.dir("config"))
}

fn patch(text: &str) -> Json {
    jsondoc::parse(text).expect("the patch parses")
}

#[test]
fn a_missing_file_reads_the_full_default_shape() {
    let d = TestDir::new("uistore-missing");
    let s = store(&d);
    assert!(!s.file().exists());
    assert_eq!(jsondoc::render(&s.read()), jsondoc::render(&defaults()));
}

#[test]
fn language_marks_heal_on_and_saved_off_survives_settle() {
    let d = TestDir::new("uistore-langmarks");
    let s = store(&d);
    assert_eq!(s.read().get("langMarks").and_then(Json::as_bool), Some(true));
    fs::create_dir_all(s.file().parent().expect("state dir")).expect("state dir");
    fs::write(s.file(), r#"{"langMarks":"false"}"#).expect("invalid preference");
    s.settle().expect("heal before GUI launch");
    let healed = fs::read_to_string(s.file()).expect("healed document");
    assert_eq!(jsondoc::parse(&healed).expect("JSON").get("langMarks").and_then(Json::as_bool), Some(true));
    fs::write(s.file(), r#"{"langMarks":false}"#).expect("saved off");
    s.settle().expect("settle saved off");
    s.update(&patch(r#"{"hidden":true}"#)).expect("unrelated preference");
    let kept = fs::read_to_string(s.file()).expect("kept document");
    assert_eq!(jsondoc::parse(&kept).expect("JSON").get("langMarks").and_then(Json::as_bool), Some(false));
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
    s.update(&patch(r#"{"hidden":true}"#)).expect("create first backup");
    assert_eq!(fs::read_to_string(&backup).expect("first backup"), previous);
    for original in ["{\"places\":\n", "", "17\n", "[1]\n", "{\"label\":\"caf\u{e9}\"\n"] {
        fs::write(s.file(), original).expect("malformed state");
        s.settle().expect("startup retains original");
        assert_eq!(fs::read_to_string(s.file()).expect("original"), original);
        assert_eq!(fs::read_to_string(&backup).expect("old backup"), previous);
        assert!(s.update(&patch(r#"{"notASetting":true}"#)).is_err());
        assert_eq!(fs::read_to_string(&backup).expect("refused patch backup"), previous);
        let mut held = fs::File::open(&backup).expect("hold old backup");
        s.update(&patch(r#"{"hidden":true}"#)).expect("native writer recovery");
        assert_eq!(fs::read_to_string(&backup).expect("exact backup"), original);
        let mut held_bytes = String::new();
        held.read_to_string(&mut held_bytes).expect("read held backup");
        assert_eq!(held_bytes, previous, "backup replacement must not truncate old inode");
        assert_eq!(fs::metadata(&backup).expect("backup mode").permissions().mode() & 0o777, 0o600);
        assert_eq!(s.read().get("hidden").and_then(Json::as_bool), Some(true));
        s.update(&patch(r#"{"hidden":false}"#)).expect("valid later write");
        assert_eq!(fs::read_to_string(&backup).expect("backup survives valid write"), original);
        previous = original.to_string();
    }
    let mut files: Vec<_> = fs::read_dir(backup.parent().expect("parent")).expect("directory")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned()).collect();
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
        let error = s.update(&patch(r#"{"hidden":true}"#)).expect_err("backup must succeed first");
        assert!(error.contains("ui.json.broken"), "{}: {}", blocked, error);
        assert_eq!(fs::read_to_string(s.file()).expect("untouched original"), original);
        assert_eq!(fs::read_to_string(&kept).expect("kept target"), "previous original bytes");
        if blocked != "directory" {
            assert_eq!(fs::read_to_string(&backup).expect("untouched backup"), "previous original bytes");
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
    for original in [r#"{"foreign":true}"#, r#"{"stateVersion":999,"view":"grid"}"#] {
        fs::write(s.file(), original).expect("unsupported state");
        assert_eq!(jsondoc::render(&s.read()), jsondoc::render(&defaults()));
        s.settle().expect("launch uses defaults without writing");
        assert_eq!(fs::read_to_string(s.file()).expect("after launch"), original);
        let why = s.update(&patch(r#"{"hidden":true}"#)).expect_err("write is refused");
        assert!(why.contains("left unchanged"), "{}", why);
        assert_eq!(fs::read_to_string(s.file()).expect("after write attempt"), original);
        assert_eq!(fs::read_to_string(&backup).expect("previous backup"), "earlier original");
    }
}

#[test]
fn a_view_json_beside_a_missing_ui_json_is_migrated_on_read() {
    let d = TestDir::new("uistore-migrate");
    let s = store(&d);
    fs::create_dir_all(d.join("config").join("flea")).expect("config dir");
    fs::write(s.legacy(), r#"{"hiddenCols":["kind","mode"],"uiScale":1.4}"#).expect("write");
    let read = s.read();
    let cols: Vec<&str> = read.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
    assert_eq!(cols, ["name", "size", "date"]);
    assert!(read.get("uiScale").is_none());
    // Once ui.json exists it is the only state file, and view.json is never read again.
    s.update(&patch(r#"{"view":"grid"}"#)).expect("update");
    fs::write(s.legacy(), r#"{"hiddenCols":["size","date","kind","mode"]}"#).expect("rewrite");
    let reread = s.read();
    let after: Vec<&str> = reread.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
    assert_eq!(after, ["name", "size", "date"]);
}

#[test]
fn per_folder_views_survive_a_store_round_trip() {
    let d = TestDir::new("uistore-folder-views");
    let s = store(&d);
    s.update(&patch(r#"{"rememberViewPerFolder":true,"folderViews":{"/home/gm":"grid","/home/gm/Downloads":"list","/home/gm/Pictures":"grid","/home/gm/Documents":"list","/home/gm/Work":"columns"}}"#)).expect("store");
    let reopened = s.read();
    assert_eq!(reopened.get("rememberViewPerFolder").and_then(Json::as_bool), Some(true));
    let views = reopened.get("folderViews").and_then(Json::as_object).expect("folderViews");
    assert_eq!(views.iter().map(|(path, mode)| format!("{}={}", path, mode.as_str().unwrap_or(""))).collect::<Vec<_>>(), ["/home/gm=grid", "/home/gm/Downloads=list", "/home/gm/Pictures=grid", "/home/gm/Documents=list", "/home/gm/Work=columns"]);
}

#[test]
fn the_migration_runs_once_and_only_when_there_is_something_to_migrate() {
    let d = TestDir::new("uistore-migrate-once");
    let s = store(&d);
    s.settle().expect("nothing to migrate");
    assert!(!s.file().exists(), "no view.json means no state file is seeded");
    fs::create_dir_all(d.join("config").join("flea")).expect("config dir");
    fs::write(s.legacy(), r#"{"hiddenCols":["kind"],"uiScale":1.4}"#).expect("write");
    s.settle().expect("migrate");
    let migrated = s.read();
    let cols: Vec<&str> = migrated.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
    assert_eq!(cols, ["name", "mode", "size", "date"]);
    assert!(!fs::read_to_string(s.file()).expect("state file").contains("uiScale"));
    // A second run must not re-derive over what the user has since changed.
    s.update(&patch(r#"{"columns":["name"]}"#)).expect("user change");
    s.settle().expect("second settle");
    let after = s.read();
    let kept: Vec<&str> = after.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
    assert_eq!(kept, ["name"], "the second settle must not re-derive from view.json");
}

// The window applies no schema of its own, so what settle leaves on disk is what the first paint
// reads: a value this Flea refuses has to be gone before the FileView ever sees it.
#[test]
fn a_settle_rewrites_a_refused_value_out_of_the_file_and_keeps_its_neighbours() {
    let d = TestDir::new("uistore-settle");
    let s = store(&d);
    fs::create_dir_all(d.join("state").join("flea")).expect("state dir");
    fs::write(s.file(), r#"{"columns":["name","size","owner"],"density":"comfortable","fromANewerFlea":{"a":1}}"#).expect("write");
    s.settle().expect("settle");
    let body = fs::read_to_string(s.file()).expect("read back");
    assert!(!body.contains("owner"), "the refused column must not survive the settle: {}", body);
    let stored = jsondoc::parse(&body).expect("valid JSON on disk");
    let cols: Vec<&str> = stored.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
    assert_eq!(cols, ["name", "size", "date"], "the refused array falls back to the shipped one");
    assert_eq!(stored.get("density").and_then(Json::as_str), Some("comfortable"), "a good key beside it stands");
    assert!(stored.get("fromANewerFlea").is_some(), "a newer Flea's own key still survives");
    let settled = fs::read_to_string(s.file()).expect("settled");
    let ino = fs::metadata(s.file()).expect("meta").ino();
    s.settle().expect("second settle");
    assert_eq!(fs::read_to_string(s.file()).expect("again"), settled, "a settled file settles to itself");
    assert_eq!(fs::metadata(s.file()).expect("meta").ino(), ino, "and is not rewritten to say so");
}

#[test]
fn an_unknown_key_is_rewritten_untouched() {
    let d = TestDir::new("uistore-unknown");
    let s = store(&d);
    fs::create_dir_all(d.join("state").join("flea")).expect("state dir");
    fs::write(s.file(), r#"{"fromANewerFlea":{"a":[1,2]},"view":"grid"}"#).expect("write");
    s.update(&patch(r#"{"hidden":true}"#)).expect("update");
    let body = fs::read_to_string(s.file()).expect("read back");
    let stored = jsondoc::parse(&body).expect("valid JSON on disk");
    assert_eq!(stored.get("view").and_then(Json::as_str), Some("grid"));
    assert_eq!(stored.get("hidden").and_then(Json::as_bool), Some(true));
    assert_eq!(
        jsondoc::render(stored.get("fromANewerFlea").expect("the newer key survived")),
        "{\n  \"a\": [\n    1,\n    2\n  ]\n}\n"
    );
}

// A truncating write would show the new bytes through an fd opened before it and keep the inode.
#[test]
fn the_write_replaces_the_file_rather_than_truncating_it() {
    let d = TestDir::new("uistore-atomic");
    let s = store(&d);
    s.update(&patch(r#"{"view":"grid"}"#)).expect("first update");
    let before = fs::read_to_string(s.file()).expect("before");
    let before_ino = fs::metadata(s.file()).expect("meta").ino();
    let mut held = fs::File::open(s.file()).expect("hold the old file open");
    s.update(&patch(r#"{"view":"columns"}"#)).expect("second update");
    let mut through_held = String::new();
    held.read_to_string(&mut through_held).expect("read the held fd");
    assert_eq!(through_held, before, "the old file must be intact behind the rename");
    assert_ne!(fs::metadata(s.file()).expect("meta").ino(), before_ino, "a rename gives a new inode");
    assert!(fs::read_to_string(s.file()).expect("after").contains("\"columns\""));
}

#[test]
fn the_file_and_its_directory_are_owner_only_and_no_temp_is_left_behind() {
    let d = TestDir::new("uistore-modes");
    let s = store(&d);
    s.update(&patch(r#"{"hidden":true}"#)).expect("update");
    assert_eq!(fs::metadata(s.file()).expect("file").permissions().mode() & 0o777, 0o600);
    let dir = s.file().parent().expect("parent").to_path_buf();
    assert_eq!(fs::metadata(&dir).expect("dir").permissions().mode() & 0o777, 0o700);
    // ls -A: the state directory holds ui.json and ui.json.lock, and no temp file.
    let mut left: Vec<String> = fs::read_dir(&dir).expect("dir").map(|e| e.expect("entry").file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["ui.json", "ui.json.lock"]);
}

#[test]
fn a_symlink_at_the_target_is_refused_and_what_it_points_at_is_untouched() {
    let d = TestDir::new("uistore-symlink");
    let s = store(&d);
    let dir = s.file().parent().expect("parent").to_path_buf();
    fs::create_dir_all(&dir).expect("state dir");
    let planted = d.file("planted.json", "planted\n");
    std::os::unix::fs::symlink(&planted, s.file()).expect("symlink");
    let message = s.update(&patch(r#"{"hidden":true}"#)).expect_err("a symlink must be refused");
    assert!(message.contains("symbolic link"), "{}", message);
    assert_eq!(fs::read_to_string(&planted).expect("planted"), "planted\n");
    assert!(fs::symlink_metadata(s.file()).expect("link").file_type().is_symlink());
}

#[test]
fn a_symlink_at_the_lock_is_refused() {
    let d = TestDir::new("uistore-locklink");
    let s = store(&d);
    let dir = s.file().parent().expect("parent").to_path_buf();
    fs::create_dir_all(&dir).expect("state dir");
    std::os::unix::fs::symlink(d.file("planted.lock", ""), s.lock_file()).expect("symlink");
    assert!(s.update(&patch(r#"{"hidden":true}"#)).is_err());
    assert!(!s.file().exists(), "nothing is written when the lock cannot be taken");
}

#[test]
fn a_second_writer_waits_for_the_exclusive_lock() {
    let d = TestDir::new("uistore-lock");
    let s = store(&d);
    s.update(&patch(r#"{"view":"grid"}"#)).expect("seed");
    let holder = fs::OpenOptions::new().read(true).write(true).open(s.lock_file()).expect("open the lock");
    lock_exclusive(&holder).expect("hold the lock");
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| s.update(&patch(r#"{"view":"columns"}"#)));
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            fs::read_to_string(s.file()).expect("during").contains("\"grid\""),
            "the second writer must not have written while the lock was held"
        );
        unlock(&holder).expect("release");
        waiting.join().expect("thread").expect("update");
    });
    assert!(fs::read_to_string(s.file()).expect("after").contains("\"columns\""));
}

// try_lock_exclusive is what open_inactive reads a live Trash review through: false while held.
#[test]
fn a_nonblocking_lock_reports_a_held_lock_rather_than_waiting() {
    let d = TestDir::new("uistore-trylock");
    let s = store(&d);
    s.update(&patch(r#"{"view":"grid"}"#)).expect("seed");
    let held = take_lock(s.lock_file()).expect("hold the lock");
    let probe = fs::OpenOptions::new().read(true).write(true).open(s.lock_file()).expect("open the lock");
    assert!(!try_lock_exclusive(&probe).expect("a held lock answers"), "a lock another handle holds is not taken");
    drop(held);
    assert!(try_lock_exclusive(&probe).expect("a freed lock locks"), "a released lock is taken");
    unlock(&probe).expect("release");
}

// No environment at all: XDG_CONFIG_HOME and XDG_STATE_HOME are process wide, cargo runs tests
// in threads, and src/userfile.rs already owns the one test that mutates XDG_CONFIG_HOME.
#[test]
fn the_paths_hang_off_the_state_home_and_the_config_home() {
    let home = PathBuf::from("/home/nobody");
    assert_eq!(state_dir(Some(PathBuf::from("/tmp/flea-test-state")), &home), PathBuf::from("/tmp/flea-test-state"));
    assert_eq!(state_dir(None, &home), PathBuf::from("/home/nobody/.local/state"));
    let s = Store::at(&state_dir(None, &home), Path::new("/tmp/flea-test-config"));
    assert_eq!(s.file(), Path::new("/home/nobody/.local/state/flea/ui.json"));
    assert_eq!(s.lock_file(), Path::new("/home/nobody/.local/state/flea/ui.json.lock"));
    assert_eq!(s.legacy(), Path::new("/tmp/flea-test-config/flea/view.json"));
}
