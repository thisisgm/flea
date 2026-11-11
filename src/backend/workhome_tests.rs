// Issue 370: a staged job publishing onto a copy-fallback mount keeps its work folder on this computer.
use super::*;
use crate::backend::archive::Formats;
use crate::backend::archivework::WORK_PREFIX;
use crate::backend::testdir::TestDir;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const EINVAL: i32 = 22;
const EIO: i32 = 5;
const OWNER_GROUP_OTHER_BITS: u32 = 0o777;
const PRIVATE_FOLDER_MODE: u32 = 0o700;
const SHARED_FOLDER_MODE: u32 = 0o755;
const READ_ONLY_FOLDER_MODE: u32 = 0o555;

// The destination reads as a copy-fallback mount and the job's renames answer as one, so the result lands by the copy.
fn off_the_mount<T>(drive: &Path, local: &Path, run: impl FnOnce() -> T) -> T {
    test_force_off_mount(Some(drive));
    test_local_roots(Some(vec![local.to_path_buf()]));
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_force_copy_fallback(true);
    test_take_last();
    let outcome = run();
    crate::backend::renamecompat::test_force_copy_fallback(false);
    crate::backend::renamecompat::test_force_fallback(false);
    test_local_roots(None);
    test_force_off_mount(None);
    outcome
}

fn work_folders(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(WORK_PREFIX))
        .collect()
}

fn has_tool(tool: &str, probe: &str) -> bool {
    std::process::Command::new(tool)
        .arg(probe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// The staged job ran with its one writable folder under the local root, never on the drive.
fn assert_staged_locally(drive: &TestDir, local: &TestDir) {
    assert_eq!(
        test_take_last().as_deref(),
        Some(local.path()),
        "the stage was made on this computer"
    );
    assert!(
        work_folders(drive.path()).is_empty(),
        "no work folder on the drive: {:?}",
        work_folders(drive.path())
    );
    assert!(
        work_folders(local.path()).is_empty(),
        "the local stage went once the result landed"
    );
}

#[test]
fn the_mount_rule_is_renamecompats_own_list() {
    let d = TestDir::new("workhomerule");
    let directory = d.dir("directory");
    let invalid = std::io::Error::from_raw_os_error(EINVAL);
    let types = [
        "fuse.rclone",
        "fuse.megafs",
        "fuse.sshfs",
        "fuse.gocryptfs",
        "fuse.s3fs",
        "fuse.mergerfs",
        "fuse.portal",
        "ext4",
        "btrfs",
        "nfs4",
        "cifs",
    ];
    for fstype in types {
        let line = format!(
            "1 0 0:1 / {} rw - {} remote: rw\n",
            d.path().display(),
            fstype
        );
        let copies = fstype == "fuse.rclone" || fstype == "fuse.megafs";
        assert_eq!(
            stages_off_in(&directory, &line),
            copies,
            "{fstype} stages off the mount only if it is rclone or megafs"
        );
        assert_eq!(
            crate::backend::renamecompat::needs_fuse_fallback_in(&directory, &invalid, &line),
            copies,
            "{fstype} is a copy mount to renamecompat only if it is rclone or megafs"
        );
    }
    let dav = Path::new("/run/user/1000/gvfs/dav:host=slot,ssl=true/notes");
    assert!(stages_off_in(dav, ""));
    assert!(crate::backend::renamecompat::needs_copy_fallback(
        dav,
        &std::io::Error::from_raw_os_error(EIO)
    ));
    let smb = Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=media/films");
    assert!(!stages_off_in(smb, ""));
}

#[test]
fn an_ordinary_drive_keeps_its_stage_beside_the_destination() {
    let d = TestDir::new("workhomebeside");
    let mut asked = false;
    let work = work_for(d.path(), "ext", "archive", "t.zip", || {
        asked = true;
        0
    })
    .unwrap();
    assert_eq!(work.dir.parent(), Some(d.path()));
    assert!(!asked, "an ordinary drive never sizes the stage");
}

#[test]
fn a_copy_fallback_mount_stages_on_the_first_local_root_that_can_hold_it() {
    let drive = TestDir::new("workhomedrive");
    let blocked = TestDir::new("workhomeblocked");
    let local = TestDir::new("workhomelocal");
    // A plain file where the root should be cannot hold a folder, so the next root is taken.
    let not_a_folder = blocked.file("cache", "");
    test_force_off_mount(Some(drive.path()));
    test_local_roots(Some(vec![not_a_folder, local.path().to_path_buf()]));
    let work = work_for(drive.path(), "ext", "archive", "t.zip", || 1);
    test_local_roots(None);
    test_force_off_mount(None);
    let work = work.unwrap();
    assert_eq!(work.dir.parent(), Some(local.path()));
    assert!(work_folders(drive.path()).is_empty());
}

fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & OWNER_GROUP_OTHER_BITS
}

#[test]
fn a_local_work_folder_is_closed_to_other_users_whatever_the_umask_made_it() {
    use std::os::unix::fs::PermissionsExt;
    let local = TestDir::new("workhomeprivate");
    let work = Work::new(local.path(), "ext").unwrap();
    std::fs::set_permissions(
        &work.dir,
        std::fs::Permissions::from_mode(SHARED_FOLDER_MODE),
    )
    .unwrap();
    let work = private(work).unwrap();
    assert_eq!(mode_of(&work.dir), PRIVATE_FOLDER_MODE);
}

// corner: runs as a plain user, where a folder without its write bit refuses a new child.
#[test]
fn a_root_with_room_that_refuses_the_work_folder_says_why() {
    use std::os::unix::fs::PermissionsExt;
    let drive = TestDir::new("workhomerefuse");
    let local = TestDir::new("workhomerefuselocal");
    let closed = local.dir("closed");
    std::fs::set_permissions(
        &closed,
        std::fs::Permissions::from_mode(READ_ONLY_FOLDER_MODE),
    )
    .unwrap();
    test_force_off_mount(Some(drive.path()));
    test_local_roots(Some(vec![closed.clone()]));
    let refused = work_for(drive.path(), "ext", "archive", "t.zip", || 1);
    test_local_roots(None);
    test_force_off_mount(None);
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(SHARED_FOLDER_MODE)).unwrap();
    let error = refused
        .err()
        .expect("a closed root cannot take the work folder");
    assert!(
        !error.msg.contains(" free on this computer"),
        "the refusal names the folder error, not a size: {}",
        error.msg
    );
    assert!(
        error.path.starts_with(&*closed.to_string_lossy()),
        "{}",
        error.path
    );
}

#[test]
fn a_cancel_stops_a_combine_or_batch_convert_publish_and_lands_nothing() {
    let d = TestDir::new("workhomecancel");
    let staged = d.file("staged.png", "not empty");
    let dest = d.join("out.png");
    crate::backend::renamecompat::test_force_fallback(true);
    crate::backend::renamecompat::test_force_copy_fallback(true);
    let outcome = crate::backend::printtool::publish(&staged, &dest, &AtomicBool::new(true));
    crate::backend::renamecompat::test_force_copy_fallback(false);
    crate::backend::renamecompat::test_force_fallback(false);
    assert!(
        outcome.is_err(),
        "a cancelled publish does not report a result"
    );
    assert!(
        dest.symlink_metadata().is_err(),
        "nothing landed at the destination"
    );
    assert!(
        staged.is_file(),
        "the stage stays for the work folder to remove"
    );
}

#[test]
fn a_job_no_local_root_can_hold_is_refused_with_its_size() {
    let drive = TestDir::new("workhomefull");
    let local = TestDir::new("workhomefulllocal");
    test_force_off_mount(Some(drive.path()));
    test_local_roots(Some(vec![local.path().to_path_buf()]));
    let refused = work_for(drive.path(), "ext", "archive", "big.zip", || u64::MAX);
    test_local_roots(None);
    test_force_off_mount(None);
    let error = refused.err().expect("no root holds u64::MAX bytes");
    assert_eq!(error.where_, "archive");
    assert!(error.msg.starts_with("big.zip needs "), "{}", error.msg);
    assert!(
        error
            .msg
            .contains(" free on this computer before it goes to the drive, and the most free is "),
        "{}",
        error.msg
    );
    assert!(work_folders(drive.path()).is_empty() && work_folders(local.path()).is_empty());
}

#[test]
fn an_extract_onto_a_copy_fallback_mount_never_stages_on_it() {
    if crate::backend::sandboxprobe::skipped() || !has_tool("bsdtar", "--version") {
        return;
    }
    let drive = TestDir::new("workhomeext");
    let local = TestDir::new("workhomeextlocal");
    let source = drive.dir("src");
    std::fs::create_dir(source.join("almappa")).unwrap();
    std::fs::write(source.join("almappa/olvass-el.txt"), b"hello\n").unwrap();
    let archive = drive.join("t370.zip");
    let built = std::process::Command::new("bsdtar")
        .args([
            "-a",
            "-c",
            "-f",
            &archive.to_string_lossy(),
            "-C",
            &source.to_string_lossy(),
            "almappa",
        ])
        .status();
    assert!(built.unwrap().success());
    let dest = drive.join("t370");
    let formats = Formats::from_tools(true, false);
    let outcome = off_the_mount(drive.path(), local.path(), || {
        crate::backend::archiveops::extract(
            &formats,
            &archive,
            &dest,
            &AtomicBool::new(false),
            None,
        )
    });
    assert!(
        outcome.expect("the extract lands through the copy"),
        "verified"
    );
    assert_eq!(
        std::fs::read(dest.join("almappa/olvass-el.txt")).unwrap(),
        b"hello\n"
    );
    assert_staged_locally(&drive, &local);
}

#[test]
fn a_compress_onto_a_copy_fallback_mount_never_stages_on_it() {
    if crate::backend::sandboxprobe::skipped() || !has_tool("bsdtar", "--version") {
        return;
    }
    let drive = TestDir::new("workhomearc");
    let local = TestDir::new("workhomearclocal");
    drive.dir("src");
    drive.file("src/a.txt", "alpha\n");
    let dest = drive.join("packed.zip");
    let formats = Formats::from_tools(true, false);
    off_the_mount(drive.path(), local.path(), || {
        crate::backend::archiveops::compress(
            &formats,
            drive.path(),
            &["src".to_string()],
            "zip",
            &dest,
            &AtomicBool::new(false),
            None,
        )
    })
    .expect("the compress lands through the copy");
    assert!(std::fs::read(&dest).unwrap().starts_with(b"PK"));
    assert_staged_locally(&drive, &local);
}

#[test]
fn a_convert_onto_a_copy_fallback_mount_never_stages_on_it() {
    if crate::backend::sandboxprobe::skipped() || !has_tool("magick", "-version") {
        return;
    }
    let drive = TestDir::new("workhomecvt");
    let local = TestDir::new("workhomecvtlocal");
    let input = drive.file(
        "pic.ppm",
        "P3\n2 2\n255\n255 0 0\n0 255 0\n0 0 255\n255 255 255\n",
    );
    let dest = drive.join("pic.png");
    off_the_mount(drive.path(), local.path(), || {
        crate::backend::archiveops::convert_one(&input, &dest, false, &AtomicBool::new(false))
    })
    .expect("the convert lands through the copy");
    assert!(std::fs::read(&dest).unwrap().starts_with(b"\x89PNG"));
    assert_staged_locally(&drive, &local);
}

fn ppm(dir: &TestDir, name: &str) -> PathBuf {
    dir.file(name, &format!("P3\n4 4\n255\n{}", "200 10 10\n".repeat(16)))
}

fn finished_ok(line: &str) -> bool {
    crate::json::field_bool(line, "ok")
}

#[test]
fn a_combine_and_a_batch_convert_onto_a_copy_fallback_mount_never_stage_on_it() {
    if crate::backend::sandboxprobe::skipped() || !has_tool("magick", "-version") {
        return;
    }
    let drive = TestDir::new("workhomeprint");
    let local = TestDir::new("workhomeprintlocal");
    let first = ppm(&drive, "first.ppm");
    let second = ppm(&drive, "second.ppm");
    let (mime, aliases) = (
        crate::backend::mime::Db::load(),
        crate::backend::aliases::Aliases::load(),
    );
    let (tx, _rx) = std::sync::mpsc::channel();
    let ask = format!(
        r#"{{"c":"combine","paths":["{}","{}"],"format":"png","layout":"Row","spacing":"None","background":"White","name":"both.png"}}"#,
        first.display(),
        second.display()
    );
    let line = off_the_mount(drive.path(), local.path(), || {
        crate::backend::combine::run(
            crate::backend::combine::Ask::parse(&ask),
            Ok(None),
            1,
            &mime,
            &aliases,
            Arc::new(AtomicBool::new(false)),
            &tx,
        )
    });
    assert!(finished_ok(&line), "{line}");
    assert!(std::fs::read(drive.join("both.png"))
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert_staged_locally(&drive, &local);
    let ask = format!(
        r#"{{"c":"convertbatch","paths":["{}"],"format":"png"}}"#,
        first.display()
    );
    let (line, _steps) = off_the_mount(drive.path(), local.path(), || {
        crate::backend::convertbatch::run(
            crate::backend::convertbatch::Ask::parse(&ask),
            Ok(None),
            2,
            &mime,
            &aliases,
            Arc::new(AtomicBool::new(false)),
            &tx,
        )
    });
    assert!(finished_ok(&line), "{line}");
    let output = PathBuf::from(crate::json::field_str(&line, "path").unwrap());
    assert_eq!(output.parent(), Some(drive.path()));
    assert!(std::fs::read(&output).unwrap().starts_with(b"\x89PNG"));
    assert_staged_locally(&drive, &local);
}
