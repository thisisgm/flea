// Resolves a uid to its login name from /etc/passwd alone, never through getpwuid: that call goes
// through NSS and can wait on a network directory, and the meta thread must never hang the column on one.
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::{collections::HashSet, sync::{Mutex, OnceLock}};

const PASSWD: &str = "/etc/passwd";
// A timed-out stat keeps its slot until it returns: one per mount, eight across all mounts.
const MAX_REMOTE_READS: usize = 8;
fn active_mounts() -> &'static Mutex<HashSet<PathBuf>> {
    static ACTIVE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}
struct ReadSlot(PathBuf);
impl ReadSlot {
    fn take(mount: PathBuf) -> Option<Self> {
        let mut active = active_mounts().lock().unwrap_or_else(|p| p.into_inner());
        if active.len() >= MAX_REMOTE_READS || !active.insert(mount.clone()) { return None; }
        Some(Self(mount))
    }
}
impl Drop for ReadSlot {
    fn drop(&mut self) {
        active_mounts().lock().unwrap_or_else(|p| p.into_inner()).remove(&self.0);
    }
}

// The owner of the path itself, so a symlink reports the link's owner as its row does; empty when unknown.
pub fn of(path: &Path) -> String {
    match path.symlink_metadata() {
        Ok(m) => name(m.uid()),
        Err(_) => String::new(),
    }
}

// None when the stat could not run, because its mount or all eight slots are held or the mount is stuck or past its deadline.
fn read_bounded(path: PathBuf, body: &str) -> Option<String> {
    let slot = if super::iomount::is_remote(&path, body) {
        Some(ReadSlot::take(super::iomount::mount_key(&path, body))?)
    } else { None };
    let work = path.clone();
    super::iomount::call(&path, body, "owner", move || {
        let _slot = slot;
        of(&work)
    }).ok()
}

// The TUI asks serially for visible rows; this path never opens file contents or starts a decoder.
pub fn spawn(row: usize, path: Option<std::path::PathBuf>, token: usize, tx: std::sync::mpsc::Sender<super::opsreq::OpMsg>) {
    let body = super::iomount::mount_body();
    std::thread::spawn(move || {
        let owner = match path { Some(path) => read_bounded(path, &body), None => Some(String::new()) };
        let retry = owner.is_none();
        let line = format!(r#"{{"t":"owner","row":{row},"token":{token},"owner":"{}","retry":{retry}}}"#, crate::json::escape(&owner.unwrap_or_default()));
        let _ = tx.send(super::opsreq::OpMsg::Meta { line });
    });
}

pub fn name(uid: u32) -> String {
    match std::fs::read_to_string(PASSWD) {
        Ok(text) => name_in(&text, uid),
        Err(_) => String::new(),
    }
}

// Sample input: gm:x:1000:1000::/home/gm:/usr/bin/bash
// A uid the file does not list answers the empty string, never the number dressed as a name.
fn name_in(passwd: &str, uid: u32) -> String {
    for line in passwd.lines() {
        let mut fields = line.split(':');
        let login = fields.next().unwrap_or("");
        // Field three is the uid; a line too short or non-numeric there is skipped, not a match.
        if fields.nth(1).and_then(|f| f.parse::<u32>().ok()) == Some(uid) {
            return login.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    static REMOTE_TESTS: Mutex<()> = Mutex::new(());

    const SAMPLE: &str = "root:x:0:0::/root:/usr/bin/bash\n\
bin:x:1:1::/:/usr/bin/nologin\n\
broken line with no fields\n\
short:x\n\
gm:x:1000:1000::/home/gm:/usr/bin/bash\n";

    #[test]
    fn the_uid_column_picks_the_login_and_a_malformed_line_is_skipped_not_matched() {
        assert_eq!(name_in(SAMPLE, 0), "root");
        assert_eq!(name_in(SAMPLE, 1000), "gm");
        assert_eq!(name_in(SAMPLE, 1), "bin");
        assert_eq!(name_in(SAMPLE, 4242), "", "an unknown uid is empty, never a number");
        assert_eq!(name_in("", 0), "");
    }

    // corner: every Linux box lists uid 0 as root in /etc/passwd, so this reads the real file.
    #[test]
    fn uid_zero_resolves_to_root_from_the_real_file() {
        assert_eq!(name(0), "root");
        assert_eq!(name(u32::MAX), "", "no account carries the largest uid");
    }

    // corner: the test runner is a local account on this box, so its own file resolves to a name.
    #[test]
    fn a_path_resolves_through_its_own_uid_and_a_missing_path_resolves_to_nothing() {
        let d = TestDir::new("owner");
        let p = d.file("mine.txt", "body");
        let uid = p.symlink_metadata().unwrap().uid();
        let passwd = std::fs::read_to_string(PASSWD).unwrap();
        assert_eq!(of(&p), name_in(&passwd, uid), "the path answer is the pure parser over the same inputs");
        assert!(!of(&p).is_empty(), "the test runner's uid is a local account on this box");
        assert_eq!(of(&d.join("never-existed")), "");
    }

    #[test]
    fn tui_owner_worker_answers_success_missing_and_out_of_range_with_the_callers_token() {
        let root = TestDir::new("tui-owner-worker");
        let file = root.file("file", "contents are never needed for an owner");
        for (path, expected, retry) in [(Some(file.clone()), of(&file), false), (Some(root.join("missing")), String::new(), false), (None, String::new(), false)] {
            let (tx, rx) = std::sync::mpsc::channel();
            spawn(7, path, 91, tx);
            let super::super::opsreq::OpMsg::Meta { line } = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() else { panic!("owner reply has the wrong transport"); };
            let value = crate::jsondoc::parse(&line).unwrap();
            assert_eq!(value.get("t").and_then(|v| v.as_str()), Some("owner"));
            assert_eq!(value.get("row").and_then(|v| v.as_f64()), Some(7.0));
            assert_eq!(value.get("token").and_then(|v| v.as_f64()), Some(91.0));
            assert_eq!(value.get("owner").and_then(|v| v.as_str()), Some(expected.as_str()));
            assert_eq!(value.get("retry").and_then(|v| v.as_bool()), Some(retry));
        }
    }

    #[test]
    fn abandoned_remote_reads_stay_bounded_without_blocking_local_owners() {
        let _serial = REMOTE_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let root = TestDir::new("owner-slots");
        let mut slots: Vec<_> = (0..MAX_REMOTE_READS).map(|n| ReadSlot::take(root.join(&format!("mount-{n}"))).unwrap()).collect();
        assert!(ReadSlot::take(root.join("mount-0")).is_none(), "a held mount cannot spawn a replacement stat");
        assert!(ReadSlot::take(root.join("overflow")).is_none(), "abandoned calls have a global bound too");
        let file = root.file("healthy", "body");
        assert_eq!(read_bounded(file.clone(), "1 0 8:1 / / rw - ext4 /dev/test rw\n"), Some(of(&file)));
        root.dir("mount-0");
        let same_mount = root.file("mount-0/file", "body");
        let body = format!("1 0 8:1 / / rw - ext4 /dev/test rw\n30 1 0:45 / {} rw - nfs test:/s rw\n31 30 0:46 / {} rw - nfs test:/held rw\n", root.path().display(), root.join("mount-0").display());
        assert!(super::super::iomount::is_remote(&file, &body));
        assert_eq!(read_bounded(file.clone(), &body), None, "the real call refuses a ninth mount");
        drop(slots.pop());
        assert!(ReadSlot::take(root.join(&format!("mount-{}", MAX_REMOTE_READS - 1))).is_some(), "a returned stat releases its slot");
        drop(slots.pop());
        assert!(ReadSlot::take(root.join("mount-0")).is_none(), "another stat returning never frees the held mount");
        assert_eq!(read_bounded(same_mount.clone(), &body), None, "the real call refuses a held mount below global capacity");
        assert_eq!(read_bounded(file.clone(), &body), Some(of(&file)), "a free mount resumes after capacity returns");
        slots.clear();
        assert_eq!(read_bounded(same_mount.clone(), &body), Some(of(&same_mount)), "a returned stat releases its own mount");
    }

    #[test]
    fn a_stuck_owner_mount_still_completes_with_the_callers_row_and_token() {
        let _serial = REMOTE_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let root = TestDir::new("owner-stuck");
        let file = root.file("file", "body");
        let _body = super::super::iomount::test_hold_body(format!("1 0 8:1 / / rw - ext4 /dev/test rw\n30 1 0:45 / {} rw - nfs test:/s rw\n", root.path().display()));
        super::super::iomount::mark_stuck(root.path());
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(17, Some(file), 103, tx);
        let super::super::opsreq::OpMsg::Meta { line } = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap() else { panic!("owner reply has the wrong transport"); };
        let value = crate::jsondoc::parse(&line).unwrap();
        assert_eq!(value.get("row").and_then(|v| v.as_f64()), Some(17.0));
        assert_eq!(value.get("token").and_then(|v| v.as_f64()), Some(103.0));
        assert_eq!(value.get("owner").and_then(|v| v.as_str()), Some(""));
        assert_eq!(value.get("retry").and_then(|v| v.as_bool()), Some(true), "a stuck mount is not an unknown uid");
    }

}
