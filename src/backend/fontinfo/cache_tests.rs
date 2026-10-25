use super::*;
use crate::backend::archivework::WORK_PREFIX;
use crate::backend::testdir::TestDir;

#[test]
fn the_oldest_entries_go_first_and_only_ours_are_touched() {
    let d = TestDir::new("fontprune");
    let root = d.dir("cache");
    let name = |n: usize| format!("{:032x}-{}.png", n, n % 3);
    let now = SystemTime::now();
    for n in 0..CACHE_MAX_FILES + 5 {
        let f = std::fs::File::create(root.join(name(n))).expect("create");
        std::fs::write(root.join(name(n)), PNG_SIGNATURE).unwrap();
        f.set_modified(now - Duration::from_secs((CACHE_MAX_FILES + 5 - n) as u64)).expect("age");
    }
    d.file("cache/notes.txt", "mine");
    d.file("cache/0123456789abcdef0123456789abcdef-7.png", "not ours: the index is 0 to 2 only");
    prune(&root);
    let left: Vec<String> = std::fs::read_dir(&root).expect("root").flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    let ours = left.iter().filter(|n| n.as_str() != "notes.txt" && !n.starts_with("0123456789abcdef")).count();
    assert_eq!(ours, CACHE_KEEP_FILES, "the cache trims to its keep mark");
    assert!(root.join(name(CACHE_MAX_FILES + 4)).exists(), "the newest stays");
    assert!(!root.join(name(0)).exists(), "the oldest goes");
    assert!(root.join("notes.txt").exists());
    assert!(root.join("0123456789abcdef0123456789abcdef-7.png").exists());
}

#[test]
fn a_stale_work_folder_of_a_killed_run_goes_and_a_fresh_one_stays() {
    let d = TestDir::new("fontstale");
    let root = d.dir("cache");
    let stale = root.join(format!("{}{}-1-1", WORK_PREFIX, WORK_TAG));
    let fresh = root.join(format!("{}{}-1-2", WORK_PREFIX, WORK_TAG));
    let other = root.join(format!("{}arc-1-3", WORK_PREFIX));
    for dir in [&stale, &fresh, &other] {
        std::fs::create_dir(dir).expect("dir");
    }
    let old = SystemTime::now() - STALE_WORK - Duration::from_secs(60);
    std::fs::File::open(&stale).expect("open").set_modified(old).expect("age");
    std::fs::File::open(&other).expect("open").set_modified(old).expect("age");
    prune(&root);
    assert!(!stale.exists());
    assert!(fresh.exists());
    assert!(other.exists(), "another job's folder is not ours to reap");
}

#[test]
fn a_name_with_a_multibyte_char_across_the_hex_digits_is_left_alone_without_a_panic() {
    let d = TestDir::new("fontutf8");
    let root = d.dir("cache");
    let odd = format!("{}\u{e9}x.png", "a".repeat(31));
    assert_eq!(odd.len(), KEY_HEX_LEN + 2 + LOOK_SUFFIX.len(), "the name is exactly a look's length");
    d.file(&format!("cache/{}", odd), "mine");
    prune(&root);
    assert!(root.join(&odd).exists());
}

#[test]
fn the_name_matcher_accepts_exactly_what_entry_writes_for_each_sample() {
    let key = "0123456789abcdef0123456789abcdef";
    for i in 0..SAMPLES {
        assert!(is_look_name(entry(Path::new("/c"), key, i).file_name().expect("name").as_bytes()), "sample {}", i);
    }
    assert!(!is_look_name(entry(Path::new("/c"), key, SAMPLES).file_name().expect("name").as_bytes()), "one past the samples is not ours");
}
