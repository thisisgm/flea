//! Marked fixture roots, the B3 synthetic formula, the pinned Flea source tree and the raw-name census.
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const SANDBOX_MARKER: &str = ".flea-test-sandbox";
pub const FIXTURE_MARKER: &str = ".findbench-root";

const TOPS: [&str; 8] = [
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Videos",
    "projects",
    "archive",
    "shared",
];
const EXTENSIONS: [&str; 15] = [
    "rs", "c", "h", "py", "js", "qml", "md", "txt", "pdf", "jpg", "mp4", "flac", "svg", "json",
    "zip",
];
const WORDS: [&str; 12] = [
    "report", "notes", "render", "window", "invoice", "meeting", "holiday", "budget", "parser",
    "model", "test", "draft",
];
const TARGETS: [&str; 3] = [
    "projects/editor/src/components/render_frame.rs",
    "Documents/reference/architecture_diagram.svg",
    "archive/2024/client/reports/quarterly_review.pdf",
];

/// Complete raw-name census: every non-hidden entry as (relative path, kind), sorted.
pub struct Census {
    pub items: Vec<(Vec<u8>, u8)>,
    pub hidden: u64,
}

impl Census {
    pub fn files(&self) -> u64 {
        self.items.iter().filter(|item| item.1 == b'f').count() as u64
    }
    pub fn dirs(&self) -> u64 {
        self.items.iter().filter(|item| item.1 == b'd').count() as u64
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The fairness harness digest: length-prefixed path bytes then the kind byte, over sorted items.
pub fn digest(items: &mut [(Vec<u8>, u8)]) -> String {
    items.sort_unstable();
    let mut hash = Sha256::new();
    for (path, kind) in items.iter() {
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path);
        hash.update([*kind]);
    }
    hash.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

/// Serial read_dir census without symlink descent; hidden names are counted, never listed.
pub fn census(tree: &Path) -> Result<Census, String> {
    let mut pending = vec![tree.to_path_buf()];
    let mut census = Census {
        items: Vec::new(),
        hidden: 0,
    };
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().as_bytes().starts_with(b".") {
                census.hidden += 1;
                continue;
            }
            let is_dir = entry.file_type().map_err(|e| e.to_string())?.is_dir();
            let rel = entry
                .path()
                .strip_prefix(tree)
                .map_err(|e| e.to_string())?
                .as_os_str()
                .as_bytes()
                .to_vec();
            census.items.push((rel, if is_dir { b'd' } else { b'f' }));
            if is_dir {
                pending.push(entry.path());
            }
        }
    }
    census.items.sort_unstable();
    Ok(census)
}

/// An absolute, canonical, existing directory holding the sandbox marker.
pub fn marked_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() || root.as_os_str().is_empty() {
        return Err(format!(
            "root must be absolute and nonempty: {}",
            root.display()
        ));
    }
    let canonical = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    if canonical != root || !canonical.join(SANDBOX_MARKER).is_file() {
        return Err(format!(
            "root is not canonical or lacks {SANDBOX_MARKER}: {}",
            root.display()
        ));
    }
    Ok(canonical)
}

/// Creates ROOT/fixture-LABEL/tree; an existing fixture directory is refused, never reused or replaced.
fn new_fixture(root: &Path, label: &str) -> Result<PathBuf, String> {
    let root = marked_root(root)?;
    let dir = root.join(format!("fixture-{label}"));
    fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::write(dir.join(FIXTURE_MARKER), "flea-walkbench\n").map_err(|e| e.to_string())?;
    let tree = dir.join("tree");
    fs::create_dir(&tree).map_err(|e| e.to_string())?;
    Ok(tree)
}

fn touch(path: &Path) -> Result<(), String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Port of the recovered B3 fixtures.py synthetic(): same names, folders and exact entry count.
pub fn synthetic(root: &Path, entries: usize) -> Result<PathBuf, String> {
    let tree = new_fixture(root, &entries.to_string())?;
    for rel in TARGETS {
        let path = tree.join(rel);
        fs::create_dir_all(path.parent().unwrap_or(&tree)).map_err(|e| e.to_string())?;
        if !path.exists() {
            touch(&path)?;
        }
    }
    let leaf_count = (entries / 32).max(8);
    let mut leaves = Vec::with_capacity(leaf_count);
    for i in 0..leaf_count {
        let (mut digits, mut value) = (Vec::new(), i / TOPS.len());
        while value > 0 {
            digits.push(value % 8);
            value /= 8;
        }
        if digits.is_empty() {
            digits.push(0);
        }
        let mut leaf = tree.join(TOPS[i % TOPS.len()]);
        for digit in digits.iter().rev() {
            leaf.push(format!("folder_{digit:02}"));
        }
        fs::create_dir_all(&leaf).map_err(|e| e.to_string())?;
        leaves.push(leaf);
    }
    let current = census(&tree)?.items.len();
    for i in 0..entries.saturating_sub(current) {
        let mut stem = format!(
            "{}_{}",
            WORDS[i % WORDS.len()],
            WORDS[(i / WORDS.len()) % WORDS.len()]
        );
        stem += &if i % 3 != 0 {
            format!("_{i:08}")
        } else {
            format!("_{:02}", i % 97)
        };
        stem += &"_detail".repeat(i % 5);
        let name = format!("{stem}.{}", EXTENSIONS[i % EXTENSIONS.len()]);
        let leaf = &leaves[(i * 37) % leaves.len()];
        let mut path = leaf.join(&name);
        if path.exists() {
            path = leaf.join(format!("{i:08}_{name}"));
        }
        touch(&path)?;
    }
    let observed = census(&tree)?.items.len();
    if observed != entries {
        return Err(format!(
            "exact fixture count failed: {observed} != {entries}"
        ));
    }
    Ok(tree)
}

/// Copies every non-hidden regular blob of REV from REPO; symlinks and submodules are skipped, nothing is executed.
pub fn flea_source(root: &Path, repo: &Path, rev: &str) -> Result<PathBuf, String> {
    let listing = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-tree", "-r", "-z", rev])
        .output()
        .map_err(|e| e.to_string())?;
    if !listing.status.success() {
        return Err(format!("git ls-tree {rev} failed"));
    }
    let mut blobs = Vec::new();
    for record in listing.stdout.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("malformed ls-tree record")?;
        let meta: Vec<&[u8]> = record[..tab].split(|b| *b == b' ').collect();
        let path = &record[tab + 1..];
        let hidden = path
            .split(|b| *b == b'/')
            .any(|part| part.starts_with(b"."));
        if meta.len() == 3 && (meta[0] == b"100644" || meta[0] == b"100755") && !hidden {
            blobs.push((
                String::from_utf8_lossy(meta[2]).into_owned(),
                PathBuf::from(std::ffi::OsStr::from_bytes(path)),
            ));
        }
    }
    let tree = new_fixture(root, "flea-source")?;
    let mut cat = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = cat.stdin.take().ok_or("no cat-file stdin")?;
    let names: Vec<String> = blobs.iter().map(|b| b.0.clone()).collect();
    let feeder =
        std::thread::spawn(move || names.iter().try_for_each(|sha| writeln!(input, "{sha}")));
    let mut output = BufReader::new(cat.stdout.take().ok_or("no cat-file stdout")?);
    for (_, rel) in &blobs {
        let mut header = String::new();
        output.read_line(&mut header).map_err(|e| e.to_string())?;
        let size: usize = header
            .split_whitespace()
            .nth(2)
            .and_then(|s| s.parse().ok())
            .ok_or("bad cat-file header")?;
        let mut body = vec![0; size + 1];
        output.read_exact(&mut body).map_err(|e| e.to_string())?;
        let path = tree.join(rel);
        fs::create_dir_all(path.parent().unwrap_or(&tree)).map_err(|e| e.to_string())?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut f| f.write_all(&body[..size]))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    feeder
        .join()
        .map_err(|_| "feeder panicked")?
        .map_err(|e| e.to_string())?;
    cat.wait().map_err(|e| e.to_string())?;
    Ok(tree)
}
