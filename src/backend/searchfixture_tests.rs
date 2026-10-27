// The deterministic synthetic inventory used by the shared search benchmark, without creating files.
use super::*;
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;

const TOPS: [&str; 8] = ["Documents", "Downloads", "Pictures", "Music", "Videos", "projects", "archive", "shared"];
const WORDS: [&str; 12] = ["report", "notes", "render", "window", "invoice", "meeting", "holiday", "budget", "parser", "model", "test", "draft"];
const EXTENSIONS: [&str; 15] = ["rs", "c", "h", "py", "js", "qml", "md", "txt", "pdf", "jpg", "mp4", "flac", "svg", "json", "zip"];
const TARGETS: [&str; 3] = [
    "projects/editor/src/components/render_frame.rs",
    "Documents/reference/architecture_diagram.svg",
    "archive/2024/client/reports/quarterly_review.pdf",
];

fn add_ancestors(path: &str, dirs: &mut BTreeSet<String>) {
    let mut parent = Path::new(path).parent();
    while let Some(dir) = parent.filter(|dir| !dir.as_os_str().is_empty()) {
        dirs.insert(dir.to_str().unwrap().to_string());
        parent = dir.parent();
    }
}

pub(crate) fn synthetic(count: usize, audit: bool) -> (Index, Vec<(Vec<u8>, bool)>) {
    let leaf_count = (count / 32).max(8);
    let mut dirs = BTreeSet::new();
    for target in TARGETS { add_ancestors(target, &mut dirs); }
    let mut leaves = Vec::new();
    for i in 0..leaf_count {
        let mut value = i / TOPS.len();
        let mut digits = Vec::new();
        while value != 0 {
            digits.push(value % 8);
            value /= 8;
        }
        if digits.is_empty() { digits.push(0); }
        let mut leaf = TOPS[i % TOPS.len()].to_string();
        for digit in digits.into_iter().rev() { leaf.push_str(&format!("/folder_{digit:02}")); }
        dirs.insert(leaf.clone());
        add_ancestors(&leaf, &mut dirs);
        leaves.push(leaf);
    }
    let mut index = Index::new();
    let mut expected = Vec::new();
    for dir in &dirs {
        assert!(index.push(dir, true));
        if audit { expected.push((dir.as_bytes().to_vec(), true)); }
    }
    for target in TARGETS {
        assert!(index.push(target, false));
        if audit { expected.push((target.as_bytes().to_vec(), false)); }
    }
    let files = count - dirs.len() - TARGETS.len();
    let mut first = vec![usize::MAX; leaf_count];
    for i in 0..leaf_count { first[(i * 37) % leaf_count] = i; }
    assert!(first.iter().all(|&i| i != usize::MAX), "the benchmark scales use a permutation of leaves");
    for (leaf, first) in leaves.iter().zip(first) {
        let mut entries = Vec::new();
        let mut present = HashSet::new();
        for i in (first..files).step_by(leaf_count) {
            let stem = format!("{}_{}", WORDS[i % WORDS.len()], WORDS[(i / WORDS.len()) % WORDS.len()]);
            let suffix = if i % 3 != 0 { format!("_{i:08}") } else { format!("_{:02}", i % 97) };
            let name = format!("{stem}{suffix}{}.{}", "_detail".repeat(i % 5), EXTENSIONS[i % EXTENSIONS.len()]);
            let name = if present.insert(name.clone()) { name } else { format!("{i:08}_{name}") };
            if audit { expected.push((format!("{leaf}/{name}").into_bytes(), false)); }
            entries.push((OsString::from(name), false));
        }
        assert!(index.push_directory(OsStr::new(leaf), &entries));
    }
    index.finish();
    (index, expected)
}

#[test]
fn search_shared_formula_has_exact_100k_and_1m_raw_inventories_and_seeded_targets() {
    for count in [100_000, 1_000_000] {
        let (index, mut expected) = synthetic(count, true);
        assert_eq!(index.len(), count);
        assert_eq!(index.partial, None);
        let mut actual: Vec<_> = (0..index.len()).map(|row| (index.raw(row), index.is_dir(row))).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
        let positions: HashMap<_, _> = actual.iter().map(|(path, kind)| (path.as_slice(), *kind)).collect();
        for target in TARGETS { assert_eq!(positions.get(target.as_bytes()), Some(&false)); }
        assert!(index_bytes(&index) <= 90 * count, "{} bytes at {count}", index_bytes(&index));
    }
}
