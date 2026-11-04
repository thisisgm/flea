use super::*;
use crate::backend::{tagreadprobe::Hold, testdir::TestDir};

fn fixture(tag: &str, count: usize) -> (TestDir, Vec<Vec<u8>>) {
    let d = TestDir::new(tag);
    let mut rels = Vec::new();
    for row in 0..count {
        let name = format!("file{row:03}");
        d.file(&name, "");
        rels.push(name.into_bytes());
    }
    (d, rels)
}

fn mount_body(d: &TestDir, remote: bool) -> String {
    let kind = if remote { "nfs" } else { "ext4" };
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - {kind} server:/share rw\n", d.path().display())
}

#[test]
fn search_tag_stop_prevents_the_first_path_and_remote_worker() {
    for remote in [false, true] {
        let (d, rels) = fixture("tagstopped", 3);
        let hold = Hold::new(d.path().to_path_buf());
        hold.release();
        let bound = Bound::new(d.path(), &mount_body(&d, remote));
        let stop = Arc::new(AtomicBool::new(true));
        super::super::iomount::test_reset();
        assert_eq!(bound.read(&rels, &stop), vec![None; 3]);
        assert_eq!(hold.reads(), 0);
        assert_eq!(super::super::iomount::test_calls(), 0);
    }
}

#[test]
fn search_tag_remote_worker_stops_before_the_next_path_and_chunk() {
    let (d, rels) = fixture("tagremotestop", TAG_CHUNK * 2 + 1);
    let bound = Bound::new(d.path(), &mount_body(&d, true));
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let hold = Hold::new(d.path().to_path_buf());
    let worker = std::thread::spawn(move || {
        super::super::iomount::test_reset();
        let result = bound.read(&rels, &worker_stop);
        (result, super::super::iomount::test_calls())
    });
    hold.wait();
    stop.store(true, Ordering::Relaxed);
    hold.release();
    let (result, calls) = worker.join().unwrap();
    assert_eq!(hold.reads(), 1);
    assert_eq!(calls, 1, "only the first remote chunk reached the bound");
    assert_eq!(result, vec![None; TAG_CHUNK * 2 + 1], "discard a read that returned after stop");
}

#[test]
fn search_tag_reread_stops_before_the_next_path_and_preserves_cached_names() {
    for remote in [false, true] {
        let (d, rels) = fixture("tagrereadstop", 21);
        let mut index = Index::new();
        for rel in &rels { assert!(index.push(OsStr::from_bytes(rel), false)); }
        let index = Arc::new(Mutex::new(index));
        let pass = Pass::new(rels.len(), Bound::new(d.path(), &mount_body(&d, remote)));
        *pass.names.lock().unwrap() = vec![Some(vec!["Old".to_string()]); rels.len()];
        let hold = Hold::new(d.path().to_path_buf());
        let worker_pass = Arc::clone(&pass);
        let worker = std::thread::spawn(move || reread(&worker_pass, &index, rels));
        hold.wait();
        pass.stop.store(true, Ordering::Relaxed);
        hold.release();
        worker.join().unwrap();
        assert_eq!(hold.reads(), 1);
        assert!(pass.names.lock().unwrap().iter().all(|names| *names == Some(vec!["Old".to_string()])));
    }
}
