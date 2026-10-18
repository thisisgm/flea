use super::*;
use crate::backend::testdir::TestDir;

#[test]
#[ignore]
fn search_mount_latency_probe() {
    let d = TestDir::new("mountprobe");
    let root = d.dir("tree");
    for directory in 0..100 {
        let dir = d.dir(&format!("tree/d{directory:03}"));
        for file in 0..999 { std::fs::File::create(dir.join(format!("file{file:04}"))).unwrap(); }
    }
    let local = "1 0 8:1 / / rw - ext4 /dev/a rw\n".to_string();
    let nested = format!("{local}30 1 0:45 / {} rw - nfs server:/share rw\n", root.join("d050").display());
    let mut previous = None;
    for (mode, body) in [("local", local), ("nested", nested)] {
        let started = Instant::now();
        let (index, dirs) = walk(&root, false, false, &body, &WalkProbes::new());
        let elapsed = started.elapsed();
        assert_eq!(index.len(), 100_000);
        assert_eq!(dirs, 101);
        assert_eq!(index.partial, None);
        let mut actual: Vec<_> = (0..index.len()).map(|row| (index.raw(row), index.is_dir(row))).collect();
        actual.sort();
        if let Some(expected) = previous { assert_eq!(actual, expected); }
        previous = Some(actual);
        eprintln!("FB7_MOUNT n=100000 mode={mode} walk_ms={:.3} dirs={dirs} logical={}", elapsed.as_secs_f64() * 1000.0, index.bytes(false));
    }
}
