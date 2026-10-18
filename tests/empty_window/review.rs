use super::*;

#[test]
fn a_real_first_listing_records_prefetch_after_empty_windows() {
    let d = TestDir::new("prefetch-first-list");
    let tree = d.dir("tree");
    std::fs::write(tree.join("hello.txt"), "hello").unwrap();
    let mut backend = Backend::spawn(&d, None, true);
    backend.ask(r#"{"c":"window","start":0,"count":0}"#);
    let recorded = d.join("prefetch");
    assert!(!recorded.exists(), "an empty window cannot record a first listing");
    let listed = backend.ask(&format!(r#"{{"c":"list","path":"{}","first":1}}"#, wire_path(&tree)));
    assert!(listed.contains(r#""n":1,"#), "{listed}");
    let rows = backend.read();
    assert!(rows.contains(r#""n":"hello.txt""#), "{rows}");
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let body = loop {
        if let Ok(body) = std::fs::read_to_string(&recorded) { break body; }
        assert!(Instant::now() < deadline, "first listing did not record prefetch ranges");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let mut lines = body.lines();
    assert_eq!(lines.next(), Some("flea-prefetch 2"));
    let parent = std::process::id();
    let stat = std::fs::read_to_string(format!("/proc/{parent}/stat")).unwrap();
    let start_time = stat.rsplit_once(')').unwrap().1.split_whitespace().nth(19).unwrap();
    assert_eq!(lines.next().unwrap(), format!("shell {parent} {start_time}"));
    let ranges: Vec<_> = lines.collect();
    assert!(!ranges.is_empty(), "prefetch records real mapped file pages");
    for range in ranges {
        let mut parts = range.splitn(3, ' ');
        let _offset: u64 = parts.next().unwrap().parse().unwrap();
        let length: u64 = parts.next().unwrap().parse().unwrap();
        assert!(length > 0);
        assert!(Path::new(parts.next().unwrap()).is_absolute());
    }
    backend.quit();
}
