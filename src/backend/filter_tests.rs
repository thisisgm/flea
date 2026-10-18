use crate::json::{field_str, field_usize, field_usize_array};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CHILD: &str = "FLEA_FILTER_PROTOCOL_CHILD";
const CHILD_TEST: &str = "backend::filter_tests::backend_child";

// Re-execute the real CLI entry point with --backend in an isolated process.
#[test]
fn backend_child() {
    if std::env::var_os(CHILD).is_some() { crate::main(); }
}

struct Fixture { root: PathBuf, tree: PathBuf }

impl Fixture {
    fn new(count: usize) -> Self {
        let output = Command::new("mktemp").args(["-d", "-t", "flea-filter-XXXXXX"]).output().expect("mktemp");
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute() && !root.as_os_str().is_empty());
        std::fs::write(root.join(".flea-test-sandbox"), "filter protocol fixture\n").unwrap();
        let tree = root.join("tree");
        std::fs::create_dir(&tree).unwrap();
        for row in 0..count {
            let ext = if row % 2 == 0 { "png" } else { "txt" };
            std::fs::File::create_new(tree.join(format!("item-{row:06}.{ext}"))).unwrap();
        }
        Self { root, tree }
    }

    fn run(&self, requests: &str) -> Vec<String> {
        let mut child = Command::new("timeout").arg("30").arg(std::env::current_exe().unwrap())
            .args([CHILD_TEST, "--exact", "--nocapture", "--", "--backend"])
            .env(CHILD, "1").env("HOME", &self.root)
            .env("XDG_CACHE_HOME", self.root.join("cache")).env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_STATE_HOME", self.root.join("state")).env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_RUNTIME_DIR", &self.root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().expect("backend child");
        child.stdin.take().unwrap().write_all(requests.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap().lines()
            .filter_map(|line| line.find("{\"t\":").map(|at| line[at..].to_string())).collect()
    }

    fn list(&self) -> String {
        format!("{{\"c\":\"list\",\"path\":\"{}\",\"first\":90}}\n", crate::json::escape(self.tree.to_str().unwrap()))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.root.is_absolute() && !self.root.as_os_str().is_empty()
            && self.root.file_name().unwrap().to_str().unwrap().starts_with("flea-filter-")
            && self.root.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.root).expect("remove only the marked mktemp root");
    }
}

fn typed<'a>(lines: &'a [String], kind: &str) -> Vec<&'a str> {
    lines.iter().filter(|line| field_str(line, "t").as_deref() == Some(kind)).map(String::as_str).collect()
}

#[test]
fn filter_finds_row_1250_outside_the_first_90_held_rows() {
    let fixture = Fixture::new(1300);
    let lines = fixture.run(&(fixture.list() + "{\"c\":\"filter\",\"query\":\"ITEM-001250.PNG\",\"id\":7}\n{\"c\":\"paths\",\"rows\":[1250],\"listing\":1}\n{\"c\":\"quit\"}\n"));
    let rows = typed(&lines, "rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].matches("\"n\":").count(), 90);
    assert!(!rows[0].contains("item-001250.png"));
    let filtered = typed(&lines, "filtered");
    assert_eq!(filtered.len(), 1, "the backend must answer a filter outside its held window: {lines:?}");
    assert_eq!(field_usize_array(filtered[0], "rows"), vec![1250]);
    assert_eq!(field_usize(filtered[0], "n"), Some(1300));
    assert_eq!(field_usize(filtered[0], "id"), Some(7));
    assert_eq!(field_usize(filtered[0], "listing"), Some(1));
    assert!(typed(&lines, "paths")[0].contains("item-001250.png"), "indices keep their original file identity");
}

#[test]
fn one_clear_keeps_numbering_and_allows_an_explicit_top_window_refill() {
    let fixture = Fixture::new(1300);
    let lines = fixture.run(&(fixture.list()
        + "{\"c\":\"window\",\"start\":450,\"count\":90}\n{\"c\":\"filter\",\"query\":\"item-000500\",\"id\":1}\n"
        + "{\"c\":\"filter\",\"query\":\"\",\"id\":2}\n{\"c\":\"window\",\"start\":0,\"count\":90}\n{\"c\":\"quit\"}\n"));
    let filtered = typed(&lines, "filtered");
    assert_eq!(filtered.len(), 2, "narrow and clear each answer once: {lines:?}");
    assert_eq!(field_usize_array(filtered[0], "rows"), vec![500]);
    assert!(filtered[1].contains("\"rows\":null"), "empty query restores the integer-count model without an identity vector");
    let rows = typed(&lines, "rows");
    assert_eq!(rows.len(), 3, "filters do not fetch metadata; only list and window do");
    assert_eq!(field_usize(rows[1], "start"), Some(450));
    assert_eq!(field_usize(rows[2], "start"), Some(0));
    assert!(rows[2].contains("item-000000.png") && rows[2].contains("item-000089.txt"));
    assert!(rows.iter().all(|line| field_usize(line, "listing") == Some(1)));
}

#[test]
#[ignore = "three-run protocol latency evidence, run explicitly with --ignored --nocapture"]
fn filter_latency() {
    for count in [1300, 100_000] {
        let fixture = Fixture::new(count);
        for run in 1..=3 {
            let lines = fixture.run(&(fixture.list() + "{\"c\":\"filter\",\"query\":\"item-001250\",\"id\":3}\n{\"c\":\"quit\"}\n"));
            let filtered = typed(&lines, "filtered");
            println!("FILTER_LATENCY count={count} run={run} reply={}", filtered.first().unwrap_or(&"UNSUPPORTED"));
        }
    }
}
