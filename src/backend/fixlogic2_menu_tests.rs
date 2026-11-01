// EY-496: the snapshot a menu opens on answers, with both kinds, when the selection holds a folder Flea cannot read.
use crate::backend::menu_actions::Snapshot;
use crate::backend::testdir::TestDir;
use flea::jsondoc::{self, Json};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// A hang guard, far above the microseconds an EACCES costs and far below the harness's 10 s menu wait.
const ANSWER_BOUND: Duration = Duration::from_secs(1);

// Gives the folder its mode back on every exit, so the sandbox's own removal can walk it.
struct Unlock(PathBuf);

impl Drop for Unlock {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn fixlogic2_snapshot_over_a_mode_000_folder_answers_at_once() {
    let dir = TestDir::new("fixlogic2-locked-snapshot");
    let notes = dir.file("notes.txt", "one");
    let locked = dir.dir("Locked");
    dir.file("Locked/inside.txt", "unreadable");
    dir.assert_contains(&locked);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let _unlock = Unlock(locked.clone());
    // As tree_outline_tests does: a reader that bypasses the mode (root) has no locked folder to prove anything on.
    if std::fs::read_dir(&locked).is_ok() {
        return;
    }
    let started = Instant::now();
    let reply = Snapshot::default().handle(
        r#"{"op":"snapshot","id":1}"#,
        vec![locked.display().to_string(), notes.display().to_string()],
    );
    let spent = started.elapsed();
    let reply = jsondoc::parse(&reply).unwrap();
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)), "{reply:?}");
    assert_eq!(
        reply
            .get("kinds")
            .and_then(Json::as_array)
            .map(<[Json]>::len),
        Some(2),
        "both selected items keep their kind: {reply:?}"
    );
    assert!(
        spent < ANSWER_BOUND,
        "the snapshot never waits on the locked folder: {spent:?}"
    );
}
