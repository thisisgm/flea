// What one shared-journal operation costs: work counts per operation, never wall time.
use super::undoprobe::{self, Counts};
use super::undoshare::{claim_redo, claim_undo, finish_redone, finish_undone, live_nonces, push_entry, redo_info, Doc, PushResult, Shared, StoredRedo, JOURNAL_FILE};
use super::undocodec::encode;
use crate::backend::testdir::TestDir;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::error::FleaError;
use crate::jsondoc::render;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

const STEPS_PER_MOVE: usize = 1000;
const DEVICE: u64 = 66;
const REGULAR: u32 = 0o100000;
const BASE_SECONDS: i64 = 1_790_000_000;
// The count tests use a journal of this many entries of FIXTURE_STEPS steps, a few thousand stored steps.
const FIXTURE_ENTRIES: usize = 5;
const FIXTURE_STEPS: usize = 200;
// Inodes per salt: wider than any step index, so two salts never share an inode.
const INODES_PER_SALT: u64 = 1_000_000;
// The salt gap between a step's before identity and its after identity.
const AFTER_SALT_OFFSET: u64 = 500;
// The salt gap that makes a second move's after identity new to the journal.
const MOVE_AGAIN_SALT_OFFSET: u64 = 900;
// A file length base, so every identity carries a length that differs by step.
const FILE_LEN_BASE: u64 = 4096;
const MTIME_NANOS: i64 = 123_456_789;
const CHANGED_NANOS: i64 = 987_654_321;
// A birth time this many seconds before BASE_SECONDS.
const BORN_BEFORE_BASE: u64 = 5;
// Salts for the entries a test adds beside the fixture's own, each outside its salts and offsets.
const UNDONE_MOVE_SALT: u64 = 40;
const RENAME_BEFORE_SALT: u64 = 77;
const RENAME_AFTER_SALT: u64 = 78;

// One identity per (salt, n), so two salts never collide and one pair always matches itself.
pub(super) fn identity(salt: u64, n: u64) -> ItemIdentity {
    let at = salt * INODES_PER_SALT + n;
    ItemIdentity::from_parts(DEVICE, at, REGULAR, FILE_LEN_BASE + n, (BASE_SECONDS + n as i64, MTIME_NANOS),
        (BASE_SECONDS + salt as i64, CHANGED_NANOS), Some((BASE_SECONDS as u64 - BORN_BEFORE_BASE, n as u32)))
}

// The after identity of step n in a move entry of the given salt; a later move starts from it.
fn after_of(salt: u64, n: u64) -> ItemIdentity {
    identity(salt + AFTER_SALT_OFFSET, n)
}

// A bulk move of `steps` files into one folder, as the perf harness's 1,000-file move records it.
fn move_entry(salt: u64, steps: usize) -> Entry {
    let steps = (0..steps as u64).map(|n| Step::Moved {
        from: PathBuf::from(format!("/home/gm/Work/project-{}/assets/images/file-{:05}.png", salt, n)),
        to: PathBuf::from(format!("/home/gm/Work/archive-{}/assets/images/file-{:05}.png", salt, n)),
        before: identity(salt, n),
        after: after_of(salt, n),
        rename_fat: false,
    }).collect();
    Entry { op: format!("move {}", salt), steps }
}

// Writes a journal of these entries in file order.
fn write_journal(shared_dir: &std::path::Path, undo: Vec<Entry>, redo: Vec<StoredRedo>) -> u64 {
    let doc = Doc {
        undo,
        redo,
        push_gen: 3,
        job_gen: 0,
    };
    let text = render(&encode(&doc));
    let path = shared_dir.join(JOURNAL_FILE);
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).unwrap();
    use std::io::Write;
    file.write_all(text.as_bytes()).unwrap();
    text.len() as u64
}

fn fixture(name: &str) -> (TestDir, Shared, u64) {
    let sandbox = TestDir::new(name);
    let shared = Shared::at(sandbox.path().join("runtime")).unwrap();
    let undo = (0..FIXTURE_ENTRIES as u64).map(|salt| move_entry(salt + 1, FIXTURE_STEPS)).collect();
    let size = write_journal(&sandbox.path().join("runtime"), undo, Vec::new());
    (sandbox, shared, size)
}

// A second move of files an earlier entry moved: each of its steps starts where one stored step ended.
fn move_again(steps: usize) -> Entry {
    let steps = (0..steps as u64).map(|n| {
        let salt = n / FIXTURE_STEPS as u64 + 1;
        let at = n % FIXTURE_STEPS as u64;
        Step::Moved {
            from: PathBuf::from(format!("/home/gm/Work/archive-{}/assets/images/file-{:05}.png", salt, at)),
            to: PathBuf::from(format!("/home/gm/Work/second-{}/file-{:05}.png", salt, at)),
            before: after_of(salt, at),
            after: identity(salt + MOVE_AGAIN_SALT_OFFSET, at),
            rename_fat: false,
        }
    }).collect();
    Entry { op: "move again".to_string(), steps }
}

fn rename_entry() -> Entry {
    Entry { op: "rename".to_string(), steps: vec![Step::Moved {
        from: PathBuf::from("/home/gm/Work/a.txt"), to: PathBuf::from("/home/gm/Work/b.txt"),
        before: identity(RENAME_BEFORE_SALT, 1), after: identity(RENAME_AFTER_SALT, 1),
        rename_fat: false,
    }] }
}

fn measured<T>(work: impl FnOnce() -> T) -> (T, Counts) {
    undoprobe::reset();
    let out = work();
    (out, undoprobe::snapshot())
}

fn pairs_of(entry: &Entry) -> Vec<(ItemIdentity, ItemIdentity)> {
    entry.steps.iter().filter_map(|step| match step {
        Step::Moved { before, after, .. } => Some((before.clone(), after.clone())),
        _ => None,
    }).collect()
}

fn file_len(shared_dir: &std::path::Path) -> u64 {
    std::fs::metadata(shared_dir.join(JOURNAL_FILE)).unwrap().len()
}

#[test]
fn one_push_decodes_once_renders_once_and_reads_the_file_once() {
    let (sandbox, shared, size) = fixture("ucost-push-one");
    let (result, counts) = measured(|| push_entry(&shared, &rename_entry()));
    assert!(matches!(result, Ok(PushResult::Stored)));
    assert_eq!(counts.decodes, 1, "one decode of the file");
    assert_eq!(counts.renders, 1, "one render of the document");
    assert_eq!(counts.bytes_read, size, "the file is read once");
    assert_eq!(counts.bytes_written, file_len(&sandbox.path().join("runtime")), "the file is written once");
}

#[test]
fn a_push_of_a_thousand_moves_visits_each_stored_step_once() {
    let (_sandbox, shared, _) = fixture("ucost-push-move");
    let stored = (FIXTURE_ENTRIES * FIXTURE_STEPS) as u64;
    let entry = move_again(STEPS_PER_MOVE);
    let (result, counts) = measured(|| push_entry(&shared, &entry));
    assert!(matches!(result, Ok(PushResult::Stored)));
    assert_eq!(counts.visits, stored, "rebase visits every stored step once");
    // Each stored identity is compared with the moves that name its own inode only; these moves do name stored inodes.
    assert!(counts.compares > 0, "the second move names stored inodes, so some identity comparison ran");
    assert!(counts.compares <= 2 * stored, "{} identity comparisons for {} stored steps", counts.compares, stored);
    assert_eq!((counts.decodes, counts.renders), (1, 1));
}

#[test]
fn an_undo_of_a_thousand_moves_visits_each_stored_step_once() {
    let (_sandbox, shared, _) = fixture("ucost-undo");
    let big = move_entry(UNDONE_MOVE_SALT, STEPS_PER_MOVE);
    assert!(matches!(push_entry(&shared, &big), Ok(PushResult::Stored)));
    let stored = (FIXTURE_ENTRIES * FIXTURE_STEPS) as u64;
    let (claimed, counts) = measured(|| claim_undo(&shared));
    let (entry, gen) = claimed.unwrap().unwrap();
    assert_eq!((counts.decodes, counts.renders), (1, 1), "a claim decodes and renders once");
    let changes: Vec<_> = pairs_of(&entry).into_iter().map(|(old, new)| (new, old)).collect();
    let (done, counts) = measured(|| finish_undone(&shared, Some(entry), &changes, gen));
    assert!(done.is_ok());
    // The undone entry is gone before the rebase walks, so it visits exactly the fixture's steps.
    assert_eq!(counts.visits, stored, "rebase visits every stored step once");
    // The undone move shares no inode with the fixture, so nothing is compared.
    assert_eq!(counts.compares, 0, "no stored step names the undone move's inodes");
    assert_eq!((counts.decodes, counts.renders), (1, 1), "a finish decodes and renders once");
}

#[test]
fn a_finished_redo_visits_each_stored_step_once() {
    let (_sandbox, shared, _) = fixture("ucost-redo");
    let stored = (FIXTURE_ENTRIES * FIXTURE_STEPS) as u64;
    let entry = move_again(STEPS_PER_MOVE);
    let changes = pairs_of(&entry);
    let (done, counts) = measured(|| finish_redone(&shared, entry, &changes, false));
    assert!(done.is_ok());
    assert_eq!(counts.visits, stored, "rebase visits every stored step once");
    assert!(counts.compares > 0, "the redone move names stored inodes, so some identity comparison ran");
    assert!(counts.compares <= 2 * stored, "{} identity comparisons for {} stored steps", counts.compares, stored);
    assert_eq!((counts.decodes, counts.renders), (1, 1));
}

#[test]
fn a_read_only_question_decodes_once_and_never_renders() {
    let (sandbox, shared, size) = fixture("ucost-read");
    let (live, counts) = measured(|| live_nonces(&shared));
    assert!(live.is_some());
    assert_eq!((counts.decodes, counts.renders, counts.bytes_read, counts.bytes_written), (1, 0, size, 0));
    write_journal(&sandbox.path().join("runtime"), Vec::new(),
        vec![StoredRedo::Err(FleaError { where_: "redo".into(), path: String::new(), msg: "nothing recorded".into() })]);
    let size = file_len(&sandbox.path().join("runtime"));
    let (info, counts) = measured(|| redo_info(&shared));
    assert!(info.is_err());
    assert_eq!((counts.decodes, counts.renders, counts.bytes_read, counts.bytes_written), (1, 0, size, 0));
    let (claimed, counts) = measured(|| claim_redo(&shared));
    assert!(matches!(claimed, Ok(Some(StoredRedo::Err(_)))));
    assert_eq!((counts.decodes, counts.renders, counts.bytes_read), (1, 1, size));
}
