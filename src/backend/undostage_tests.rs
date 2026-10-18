// The staged file text renders keys v, gen, undo, redo in that order, and one batched rebase equals one rebase per pair.
use super::undocodec::encode;
use super::undocost_tests::identity;
use super::undorebase::RebaseMap;
use super::undoshare::{Doc, StoredRedo, MAX_FILE_BYTES};
use super::undostage::Staged;
use crate::backend::link::LinkKind;
use crate::backend::redo::Replay;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::error::FleaError;
use crate::jsondoc::render;
use std::path::PathBuf;

// Written by the code before the staged render, from the document fixture_doc builds.
const GOLDEN: &str = include_str!("../../tests/fixtures/undo-journal-golden.json");

// Multiplier and increment of the test's linear congruential generator (Knuth's MMIX constants).
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const LCG_INCREMENT: u64 = 1442695040888963407;
// The generator's low bits cycle with a short period, so a draw keeps only its top 31 bits.
const LCG_DRAW_SHIFT: u32 = 33;
const LCG_SEED: u64 = 11;
// Trim caps the byte-size test tries are this many bytes apart.
const TRIM_CAP_STRIDE: usize = 37;
// Entries added to the fixture document in the trim test, and how much longer each one's path grows.
const EXTRA_TRIM_ENTRIES: u64 = 6;
const PATH_GROWTH_PER_ENTRY: usize = 40;
// The random rebase test: documents tried, and the size of each one.
const REBASE_ROUNDS: usize = 200;
const ENTRIES_PER_DOC: usize = 3;
const STEPS_PER_ENTRY: usize = 4;
const REDO_STEPS: usize = 3;
// A batch holds 1 to this many pairs, cycling with the round.
const MAX_PAIRS_PER_BATCH: usize = 8;
// A small identity space, so moves collide and chain.
const INODE_CHOICES: u64 = 5;
const LEN_CHOICES: u64 = 2;
const MTIME_CHOICES: i64 = 2;
const BORN_CHOICES: u64 = 3;
const STEP_KINDS: u64 = 5;
// The size cap the staged journal keeps, in MiB.
const MAX_FILE_MIB: u64 = 32;

fn path(text: &str) -> PathBuf {
    PathBuf::from(text)
}

// Every step kind, a birth time known and unknown, escapes in text, and both kinds of redo record.
pub(super) fn fixture_doc() -> Doc {
    let plain = ItemIdentity::from_parts(1, 2, 0o100000, 3, (4, 5), (6, 7), None);
    let steps = vec![
        Step::Moved { from: path("/a/one \"quoted\".txt"), to: path("/b/uni\u{e9}\u{4e2d}.txt"), before: identity(1, 1), after: identity(2, 1), rename_fat: false },
        Step::Created { path: path("/a/made") },
        Step::Linked { path: path("/a/link"), identity: plain.clone(), source: path("/a/one"), kind: LinkKind::Relative },
        Step::Copied { from: path("/a/c"), to: path("/b/c"), source: identity(3, 1), created: plain.clone(), manifest: None, manifest_nonce: Some(7), merge: false, rename_fat: false },
        Step::MadeDir { path: path("/a/dir"), identity: identity(4, 2) },
        Step::MadeFile { path: path("/a/file"), identity: plain.clone() },
        Step::Trashed(crate::backend::trash::Entry { original: path("/a/gone"), uri: "trash:///gone\n2".to_string(), before: Vec::new(), trashed_at: String::new() }),
        Step::Mode { path: path("/a/mode"), before: 0o644, after: 0o600, dev: 9, ino: 10, born: Some((11, 12)), generation: None },
    ];
    let replay = Replay::from_steps("copy".to_string(), vec![
        (steps[3].clone(), Some(identity(3, 1)), Some((path("/b"), identity(5, 1)))),
        (steps[1].clone(), None, None),
    ]);
    Doc {
        undo: vec![
            Entry {
                op: "move".to_string(),
                steps,
            },
            Entry {
                op: "barrier".to_string(),
                steps: vec![Step::Barrier],
            },
        ],
        redo: vec![
            StoredRedo::Ok(replay),
            StoredRedo::Err(FleaError {
                where_: "redo".into(),
                path: "/a".into(),
                msg: "gone".into(),
            }),
        ],
        push_gen: 7,
        job_gen: 0,
    }
}

fn empty() -> Doc {
    Doc {
        undo: Vec::new(),
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    }
}

#[test]
fn the_staged_text_is_the_old_render_byte_for_byte() {
    let doc = fixture_doc();
    let staged = Staged::new(&doc);
    // The v2 to v3 bump moves the version line alone; every other byte stays pinned.
    let want = GOLDEN.replacen("\"v\": 2", "\"v\": 3", 1);
    assert_eq!(staged.text(), want, "the journal file did not change a byte");
    assert_eq!(staged.text(), render(&encode(&doc)));
    assert_eq!(staged.len(), GOLDEN.len() as u64, "the staged size is the written size");
    for doc in [
        empty(),
        Doc {
            undo: Vec::new(),
            redo: fixture_doc().redo,
            push_gen: 1,
            job_gen: 0,
        },
        Doc {
            undo: fixture_doc().undo,
            redo: Vec::new(),
            push_gen: u64::MAX,
            job_gen: 0,
        },
    ] {
        let staged = Staged::new(&doc);
        assert_eq!(staged.text(), render(&encode(&doc)));
        assert_eq!(staged.len(), staged.text().len() as u64);
    }
}

// The reference trim: render the whole document on every turn and drop the oldest until it fits.
fn old_trim(doc: &mut Doc, cap: u64) {
    while render(&encode(doc)).len() as u64 > cap {
        if doc.undo.len() > 1 { doc.undo.remove(0); } else if !doc.redo.is_empty() { doc.redo.remove(0); } else { break; }
    }
}

#[test]
fn trimming_by_piece_sizes_keeps_what_trimming_by_whole_renders_kept() {
    let mut doc = fixture_doc();
    for salt in 0..EXTRA_TRIM_ENTRIES {
        doc.undo.push(Entry { op: format!("op {}", salt), steps: vec![Step::MadeFile { path: path(&format!("/a/{}", "x".repeat(PATH_GROWTH_PER_ENTRY * salt as usize + 1))), identity: identity(salt, 1) }] });
    }
    let whole = render(&encode(&doc)).len() as u64;
    for cap in (0..=whole + 10).step_by(TRIM_CAP_STRIDE) {
        let mut old = Doc {
            undo: doc.undo.clone(),
            redo: Vec::new(),
            push_gen: doc.push_gen,
            job_gen: 0,
        };
        old.redo.extend(fixture_doc().redo);
        let mut staged = Staged::new(&old);
        staged.trim_to(cap);
        old_trim(&mut old, cap);
        assert_eq!(staged.text(), render(&encode(&old)), "cap {}", cap);
        assert_eq!(staged.fits(cap), render(&encode(&old)).len() as u64 <= cap);
    }
    assert_eq!(MAX_FILE_BYTES, MAX_FILE_MIB * 1024 * 1024, "AGENTS.md documents the shared journal cap as 32 MiB");
}

// A small identity space, so moves collide and chain: a new identity is often the next move's old one.
fn lcg(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_INCREMENT);
    *state >> LCG_DRAW_SHIFT
}

fn small_identity(state: &mut u64) -> ItemIdentity {
    let born = match lcg(state) % BORN_CHOICES { 0 => None, 1 => Some((1, 1)), _ => Some((2, 2)) };
    ItemIdentity::from_parts(1, lcg(state) % INODE_CHOICES, 0o100000, lcg(state) % LEN_CHOICES, (lcg(state) as i64 % MTIME_CHOICES, 0), (0, 0), born)
}

fn small_step(state: &mut u64) -> Step {
    let id = small_identity(state);
    match lcg(state) % STEP_KINDS {
        0 => Step::Moved { from: path("/f"), to: path("/t"), before: id, after: small_identity(state), rename_fat: false },
        1 => Step::Copied { from: path("/f"), to: path("/t"), source: id, created: small_identity(state), manifest: None, manifest_nonce: None, merge: false, rename_fat: false },
        2 => Step::MadeFile { path: path("/m"), identity: id },
        3 => Step::MadeDir { path: path("/d"), identity: id },
        _ => Step::Linked { path: path("/l"), identity: id, source: path("/s"), kind: LinkKind::Hard },
    }
}

#[test]
fn one_batched_rebase_equals_one_rebase_per_pair() {
    let mut state = LCG_SEED;
    for round in 0..REBASE_ROUNDS {
        let mut doc = empty();
        for _ in 0..ENTRIES_PER_DOC {
            doc.undo.push(Entry { op: "op".to_string(), steps: (0..STEPS_PER_ENTRY).map(|_| small_step(&mut state)).collect() });
            let saved = (0..REDO_STEPS).map(|_| (small_step(&mut state), Some(small_identity(&mut state)), Some((path("/p"), small_identity(&mut state))))).collect();
            doc.redo.push(StoredRedo::Ok(Replay::from_steps("op".to_string(), saved)));
        }
        let pairs: Vec<_> = (0..1 + round % MAX_PAIRS_PER_BATCH).map(|_| (small_identity(&mut state), small_identity(&mut state))).collect();
        let mut sequential = Doc {
            undo: doc.undo.clone(),
            redo: Vec::new(),
            push_gen: 0,
            job_gen: 0,
        };
        sequential.redo = doc.redo.iter().map(|r| match r {
            StoredRedo::Ok(replay) => { let (op, steps) = replay.steps_data(); StoredRedo::Ok(Replay::from_steps(op, steps)) }
            StoredRedo::Err(e) => StoredRedo::Err(FleaError { where_: e.where_.clone(), path: e.path.clone(), msg: e.msg.clone() }),
        }).collect();
        for (old, new) in &pairs {
            for entry in &mut sequential.undo { entry.rebase(old, new); }
            for stored in &mut sequential.redo { if let StoredRedo::Ok(replay) = stored { replay.rebase(old, new); } }
        }
        let map = RebaseMap::new(pairs.iter().map(|(old, new)| (old, new)));
        for entry in &mut doc.undo { entry.rebase_with(&mut |id| map.moved(id)); }
        for stored in &mut doc.redo {
            if let StoredRedo::Ok(replay) = stored { replay.rebase_with(&mut |id| map.moved(id), &mut |id| map.item(id)); }
        }
        assert_eq!(render(&encode(&doc)), render(&encode(&sequential)), "round {}", round);
    }
}

// A journal one version above this reader, empty: what a newer writer leaves.
const NEWER_JOURNAL: &str = "{\"v\":4,\"undo\":[],\"redo\":[]}";

// A journal the reader ignores (here a symlink) still answers newer when its target is.
#[test]
fn an_ignored_journal_that_a_newer_writer_owns_is_still_refused() {
    let sandbox = crate::backend::testdir::TestDir::new("ustage-newer-link");
    let dir = sandbox.path().join("runtime");
    let shared = super::undoshare::Shared::at(dir.clone()).unwrap();
    let target = sandbox.path().join("newer.json");
    std::fs::write(&target, NEWER_JOURNAL).unwrap();
    std::os::unix::fs::symlink(&target, dir.join(super::undoshare::JOURNAL_FILE)).unwrap();
    let entry = Entry { op: "op".to_string(), steps: vec![Step::Created { path: path("/a") }] };
    assert!(super::undoshare::push_entry(&shared, &entry).is_err(), "a newer file is never rewritten");
    // A wrong store would rename a fresh file onto the journal path, replacing the link.
    let journal = dir.join(super::undoshare::JOURNAL_FILE);
    assert!(journal.symlink_metadata().unwrap().file_type().is_symlink(), "the journal path is still the link");
    assert_eq!(std::fs::read_link(&journal).unwrap(), target, "the link still points at the newer file");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), NEWER_JOURNAL, "the newer file keeps its bytes");
}

#[test]
fn a_journal_with_a_merge_step_is_written_past_v2() {
    let id = identity(9, 1);
    let step = Step::Copied { from: path("/a/trip"), to: path("/b/trip"), source: id.clone(), created: id,
        manifest: None, manifest_nonce: None, merge: true, rename_fat: false };
    let doc = super::undoshare::Doc {
        undo: vec![Entry {
            op: "copy".to_string(),
            steps: vec![step],
        }],
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    let text = Staged::new(&doc).text();
    let version: u64 = text.split("\"v\": ").nth(1).and_then(|rest| rest.split(',').next())
        .and_then(|num| num.trim().parse().ok()).expect("the staged text carries a version");
    assert_eq!(version, 3, "a merge step is written past v2: {text}");
    assert!(version > 2, "a v2-only reader answers this file Newer");
    assert!(matches!(super::undostage::read(&text), super::undostage::Read::Doc(_)), "this reader still reads it");
    for old in [r#"{"v":1,"gen":0,"undo":[],"redo":[]}"#, r#"{"v":2,"gen":0,"undo":[],"redo":[]}"#] {
        assert!(matches!(super::undostage::read(old), super::undostage::Read::Doc(_)), "a v1/v2 journal still reads: {old}");
    }
    assert!(matches!(super::undostage::read(NEWER_JOURNAL), super::undostage::Read::Newer), "one past this reader is Newer");
}
