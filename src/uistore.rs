// Where ui.json lives and the one way it is rewritten: the lock, the re-read, the temp and the rename.
use crate::jsondoc::{self, Json};
use crate::uischema;
use crate::uistate;
use crate::userfile;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const DIR: &str = "flea";
const FILE: &str = "ui.json";
const LOCK: &str = "ui.json.lock";
const LEGACY: &str = "view.json";

// The state file is one user's, so it is created owner-only in the open() call and never chmod'd after.
const OWNER_ONLY_FILE: u32 = 0o600;
const OWNER_ONLY_DIR: u32 = 0o700;
// open(2) O_NOFOLLOW on Linux: a planted symlink at the lock is an error rather than a redirect.
const O_NOFOLLOW: i32 = 0o400000;

// The one shared update path both front ends go through, and the paths it works on.
pub struct Store {
    file: PathBuf,
    lock: PathBuf,
    legacy: PathBuf,
}

impl Store {
    pub fn user() -> Result<Store, String> {
        Ok(Store::at(&state_home()?, &userfile::config_home()?))
    }

    pub fn at(state_dir: &Path, config_dir: &Path) -> Store {
        Store {
            file: state_dir.join(DIR).join(FILE),
            lock: state_dir.join(DIR).join(LOCK),
            legacy: config_dir.join(DIR).join(LEGACY),
        }
    }

    #[cfg(test)]
    pub fn file(&self) -> &Path {
        &self.file
    }

    #[cfg(test)]
    pub fn lock_file(&self) -> &Path {
        &self.lock
    }

    #[cfg(test)]
    pub fn legacy(&self) -> &Path {
        &self.legacy
    }

    // Never fails, so a front end can read it before first paint without a branch for a broken file.
    pub fn read(&self) -> Json {
        if let Ok(text) = fs::read_to_string(&self.file) {
            return uistate::from_file(&text);
        }
        // Only while ui.json is absent: once it exists it is the state file and 0.1.3's is never read again.
        match fs::read_to_string(&self.legacy) {
            Ok(text) => uistate::from_view_json(&text),
            Err(_) => uischema::defaults(),
        }
    }

    // The lock is held across the re-read, the validation, the merge, the temp write and the rename,
    // so a second Flea cannot land between this one's read and its write.
    pub fn update(&self, patch: &Json) -> Result<Json, String> {
        let dir = self.file.parent().ok_or_else(|| format!("{} has no directory to write in", self.file.display()))?;
        make_dir(dir)?;
        let lock = take_lock(&self.lock)?;
        let next = uistate::patched(&self.read(), patch)?;
        self.write(&next)?;
        lock.unlock().map_err(|e| format!("{} could not be unlocked ({:?})", self.lock.display(), e.kind()))?;
        Ok(next)
    }

    // Before the window, because the window reads this file with its own FileView and applies no
    // schema of its own: an empty patch rewrites whatever is here through the same per-key
    // validation a patch gets, so a refused value cannot be what the first paint draws. It is also
    // where 0.1.3's view.json becomes ui.json, since read() falls back to it while ui.json is
    // absent, and a ui.json that exists means view.json is never read again.
    pub fn settle(&self) -> Result<(), String> {
        if fs::symlink_metadata(&self.file).is_err() && fs::symlink_metadata(&self.legacy).is_err() {
            return Ok(());
        }
        self.update(&Json::Obj(Vec::new())).map(|_| ())
    }

    // AGENTS.md "Predictable path writes": unlink this pid's own leftover, create exclusively, rename last.
    fn write(&self, value: &Json) -> Result<(), String> {
        refuse_a_link(&self.file)?;
        let tmp = PathBuf::from(format!("{}.{}.tmp", self.file.display(), std::process::id()));
        let _ = fs::remove_file(&tmp);
        let written = write_new(&tmp, &jsondoc::render(value)).and_then(|()| {
            fs::rename(&tmp, &self.file)
                .map_err(|e| format!("{} could not replace {} ({:?})", tmp.display(), self.file.display(), e.kind()))
        });
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written
    }
}

fn state_home() -> Result<PathBuf, String> {
    Ok(state_dir(userfile::env_dir("XDG_STATE_HOME"), &userfile::home()?))
}

// The environment is read by the caller above and never here, so the rule can be tested without
// mutating a process-wide variable that another test in this binary is reading at the same time.
fn state_dir(from_env: Option<PathBuf>, home: &Path) -> PathBuf {
    from_env.unwrap_or_else(|| home.join(".local").join("state"))
}

fn make_dir(dir: &Path) -> Result<(), String> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(OWNER_ONLY_DIR)
        .create(dir)
        .map_err(|e| format!("{} could not be created ({:?})", dir.display(), e.kind()))
}

// flock(2) through std: advisory, exclusive, cross-process, and released when this file closes or the process dies.
fn take_lock(path: &Path) -> Result<fs::File, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(OWNER_ONLY_FILE)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|e| format!("{} could not be opened as the ui.json lock ({:?})", path.display(), e.kind()))?;
    file.lock()
        .map_err(|e| format!("{} could not be locked ({:?})", path.display(), e.kind()))?;
    Ok(file)
}

// The state file's path is predictable, so a link planted at it is refused rather than written through.
fn refuse_a_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(format!("{} is a symbolic link, so the state file was not written", path.display()))
        }
        Ok(meta) if !meta.file_type().is_file() => {
            Err(format!("{} is not a regular file, so the state file was not written", path.display()))
        }
        _ => Ok(()),
    }
}

fn write_new(tmp: &Path, text: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY_FILE)
        .open(tmp)
        .map_err(|e| format!("{} could not be created ({:?})", tmp.display(), e.kind()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("{} could not be written ({:?})", tmp.display(), e.kind()))
}

#[cfg(test)]
mod tests {
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
    fn a_malformed_file_reads_the_full_default_shape_rather_than_throwing() {
        let d = TestDir::new("uistore-malformed");
        let s = store(&d);
        fs::create_dir_all(s.file().parent().expect("parent")).expect("state dir");
        fs::write(s.file(), "{ this is not json").expect("write");
        assert_eq!(jsondoc::render(&s.read()), jsondoc::render(&defaults()));
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
        fs::write(s.file(), r#"{"columns":["name","size","owner"],"density":"compact","fromANewerFlea":{"a":1}}"#).expect("write");
        s.settle().expect("settle");
        let body = fs::read_to_string(s.file()).expect("read back");
        assert!(!body.contains("owner"), "the refused column must not survive the settle: {}", body);
        let stored = jsondoc::parse(&body).expect("valid JSON on disk");
        let cols: Vec<&str> = stored.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"], "the refused array falls back to the shipped one");
        assert_eq!(stored.get("density").and_then(Json::as_str), Some("compact"), "a good key beside it stands");
        assert!(stored.get("fromANewerFlea").is_some(), "a newer Flea's own key still survives");
        let settled = fs::read_to_string(s.file()).expect("settled");
        s.settle().expect("second settle");
        assert_eq!(fs::read_to_string(s.file()).expect("again"), settled, "a settled file settles to itself");
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
        holder.lock().expect("hold the lock");
        std::thread::scope(|scope| {
            let waiting = scope.spawn(|| s.update(&patch(r#"{"view":"columns"}"#)));
            std::thread::sleep(std::time::Duration::from_millis(300));
            assert!(
                fs::read_to_string(s.file()).expect("during").contains("\"grid\""),
                "the second writer must not have written while the lock was held"
            );
            holder.unlock().expect("release");
            waiting.join().expect("thread").expect("update");
        });
        assert!(fs::read_to_string(s.file()).expect("after").contains("\"columns\""));
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
}
