use super::*;

// proc(5) fields after ')' begin at field 3, so starttime (field 22) has zero-based index 19.
const STAT_STARTTIME_AFTER_NAME: usize = 19;

#[test]
fn a_real_first_listing_records_prefetch_after_empty_windows() {
    let d = TestDir::new("prefetch-first-list");
    let tree = d.dir("tree");
    std::fs::write(tree.join("hello.txt"), "hello").unwrap();
    let (library, counts) = observer(&d);
    let mut backend = Backend::spawn(&d, Some((&library, &counts)), true);
    for _ in 0..2 {
        backend.ask(r#"{"c":"window","start":0,"count":0}"#);
    }
    // This later reply follows both completed handlers, including recorder starts after a rows flush.
    assert_eq!(
        backend.ask(r#"{"c":"paths","rows":[]}"#),
        r#"{"t":"paths","paths":[]}"#
    );
    // pthread_create is counted before it returns, independently of when the recorder writes its file.
    assert_eq!(
        counters(&counts)[THREAD_STARTS],
        reader_only_counts()[THREAD_STARTS],
        "completed empty-window handlers must not start a recorder"
    );
    let recorded = d.join("prefetch");
    assert!(
        !recorded.exists(),
        "an empty window cannot record a first listing"
    );
    let listed = backend.ask(&format!(
        r#"{{"c":"list","path":"{}","first":1}}"#,
        wire_path(&tree)
    ));
    assert!(listed.contains(r#""n":1,"#), "{listed}");
    let rows = backend.read();
    assert!(rows.contains(r#""n":"hello.txt""#), "{rows}");
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let body = loop {
        if let Ok(body) = std::fs::read_to_string(&recorded) {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "first listing did not record prefetch ranges"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let mut lines = body.lines();
    assert_eq!(lines.next(), Some("flea-prefetch 2"));
    let parent = std::process::id();
    let stat = std::fs::read_to_string(format!("/proc/{parent}/stat")).unwrap();
    // Sample input: "4242 (qs (x) y) S 1 4242 ... 0 5561234 ..."; the final ')' ends comm.
    let start_time = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .nth(STAT_STARTTIME_AFTER_NAME)
        .unwrap();
    assert_eq!(
        lines.next().unwrap(),
        format!("shell {parent} {start_time}")
    );
    let ranges: Vec<_> = lines.collect();
    assert!(
        !ranges.is_empty(),
        "prefetch records real mapped file pages"
    );
    for range in ranges {
        // Sample input: "0 32768 /usr/lib/libQt6Qml.so.6.11.2": offset, length and absolute path.
        let mut parts = range.splitn(3, ' ');
        let _offset: u64 = parts.next().unwrap().parse().unwrap();
        let length: u64 = parts.next().unwrap().parse().unwrap();
        assert!(length > 0);
        assert!(Path::new(parts.next().unwrap()).is_absolute());
    }
    backend.quit();
}
