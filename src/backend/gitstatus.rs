// A porcelain snapshot shared by the graphical and terminal clients, with no per-row filesystem work.
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::time::Instant;

#[path = "gitbudget.rs"]
pub(super) mod budget;
use budget::{Budget, Records};

const ORDINARY_FIELDS: usize = 9;
const RENAME_FIELDS: usize = 10;
const UNMERGED_FIELDS: usize = 11;
const RECORD_TYPE_FIELD: usize = 0;
const STATUS_FIELD: usize = 1;
const SUBMODULE_FIELD: usize = 2;
const INDEX_MODE_FIELD: usize = 4;
const WORKTREE_MODE_FIELD: usize = 5;
const UNMERGED_WORKTREE_MODE_FIELD: usize = 6;
const STATUS_BYTES: usize = 2;

#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub status: char,
    pub original: Option<PathBuf>,
    directory: bool,
    index_removed: bool,
}

#[derive(Debug)]
struct Endpoint {
    change: usize,
    original: bool,
}

impl Endpoint {
    fn path<'a>(&self, changes: &'a [Change]) -> &'a Path {
        let change = &changes[self.change];
        if self.original {
            change.original.as_deref().unwrap()
        } else {
            &change.path
        }
    }
}

#[derive(Debug)]
pub struct Snapshot {
    pub branch: String,
    pub changes: Vec<Change>,
    endpoints: Vec<Endpoint>,
}

impl Snapshot {
    pub fn label(&self) -> String {
        format!("{} · {} changed", self.branch, self.changes.len())
    }

    // The caller supplies only visible paths relative to the repository root.
    pub fn mark(&self, path: &Path) -> (Option<char>, bool) {
        let index = self
            .changes
            .binary_search_by(|entry| entry.path.as_path().cmp(path))
            .ok();
        let changed = index.map(|index| self.changes[index].status);
        let start = self
            .endpoints
            .partition_point(|entry| entry.path(&self.changes) <= path);
        let folder = !path.as_os_str().is_empty()
            && (index.is_some_and(|index| self.changes[index].directory)
                || self
                    .endpoints
                    .get(start)
                    .is_some_and(|entry| entry.path(&self.changes).starts_with(path)));
        (changed, folder)
    }
}

// Parser fixture entry point used by gitstatus_tests and the Git request tests.
#[cfg(test)]
pub fn parse(bytes: &[u8]) -> Result<Snapshot, String> {
    let budget = Budget::new(Instant::now() + super::gitprobe::DEADLINE, &|| false);
    budget.claim(bytes.len())?;
    parse_with(bytes, &budget)
}

// Sample input: b"# branch.head main\x001 .M N... 100644 100644 100644 a b src/lib.rs\0".
pub(super) fn parse_with(bytes: &[u8], budget: &Budget<'_>) -> Result<Snapshot, String> {
    budget.check()?;
    let mut records = Records::new(bytes, budget);
    let mut branch = None;
    let mut changes = Vec::new();
    while let Some(record) = records.next()? {
        if record.is_empty() {
            continue;
        }
        if let Some(head) = record.strip_prefix(b"# branch.head ") {
            branch = Some(budget.branch(if head == b"(detached)" {
                b"detached"
            } else {
                head
            })?);
            continue;
        }
        if record.starts_with(b"# ") || record.starts_with(b"! ") {
            continue;
        }
        let (path, status, original, directory, index_removed) = match record[0] {
            b'?' if record.starts_with(b"? ") => (
                relative(&record[2..], budget)?,
                '?',
                None,
                record.ends_with(b"/"),
                false,
            ),
            b'1' | b'2' | b'u' => {
                let count = match record[0] {
                    b'1' => ORDINARY_FIELDS,
                    b'2' => RENAME_FIELDS,
                    _ => UNMERGED_FIELDS,
                };
                let mut fields = [b"".as_slice(); UNMERGED_FIELDS];
                let mut parts = record.splitn(count, |byte| *byte == b' ');
                for field in &mut fields[..count] {
                    *field = parts
                        .next()
                        .ok_or("Git status returned an invalid record.")?;
                }
                if fields[RECORD_TYPE_FIELD] != &record[..1]
                    || fields[STATUS_FIELD].len() != STATUS_BYTES
                    || fields[STATUS_FIELD]
                        .iter()
                        .any(|byte| !b".MADRCUT".contains(byte))
                {
                    return Err("Git status returned an invalid record.".into());
                }
                let xy = fields[STATUS_FIELD];
                // Unmerged modes describe presence; rename and copy destinations count as added.
                let status = if record[0] == b'u' {
                    if fields[UNMERGED_WORKTREE_MODE_FIELD] == b"000000" {
                        'D'
                    } else {
                        'M'
                    }
                } else if xy.contains(&b'D') {
                    'D'
                } else if xy.contains(&b'A') || xy.contains(&b'R') || xy.contains(&b'C') {
                    'A'
                } else {
                    'M'
                };
                let original = if record[0] == b'2' {
                    Some(relative(
                        records
                            .next()?
                            .ok_or("Git status omitted a rename source.")?,
                        budget,
                    )?)
                } else {
                    None
                };
                (
                    relative(fields[count - 1], budget)?,
                    status,
                    original,
                    fields[SUBMODULE_FIELD].starts_with(b"S"),
                    record[0] == b'1'
                        && xy == b"D."
                        && fields[INDEX_MODE_FIELD] == b"000000"
                        && fields[WORKTREE_MODE_FIELD] == b"000000",
                )
            }
            _ => return Err("Git status returned an unknown record.".into()),
        };
        budget.reserve(&mut changes, 1)?;
        changes.push(Change {
            path,
            status,
            original,
            directory,
            index_removed,
        });
    }
    budget::sort(&mut changes, budget, |a, b| a.path.cmp(&b.path))?;
    coalesce(&mut changes, budget)?;
    let mut endpoints = Vec::new();
    for (index, change) in changes.iter().enumerate() {
        budget.check()?;
        budget.reserve(&mut endpoints, 1 + usize::from(change.original.is_some()))?;
        endpoints.push(Endpoint {
            change: index,
            original: false,
        });
        if change.original.is_some() {
            endpoints.push(Endpoint {
                change: index,
                original: true,
            });
        }
    }
    budget::sort(&mut endpoints, budget, |a, b| {
        a.path(&changes).cmp(b.path(&changes))
    })?;
    budget.check()?;
    Ok(Snapshot {
        branch: branch.ok_or("Git status omitted the branch.")?,
        changes,
        endpoints,
    })
}

// Git can report an index deletion and the retained untracked worktree entry at the same path.
fn coalesce(changes: &mut Vec<Change>, budget: &Budget<'_>) -> Result<(), String> {
    let (mut read, mut write) = (0, 0);
    while read < changes.len() {
        budget.check()?;
        let mut end = read + 1;
        while end < changes.len() && changes[read].path == changes[end].path {
            budget.check()?;
            end += 1;
        }
        let keep = match end - read {
            1 => read,
            2 => {
                let untracked = if changes[read].status == '?' {
                    read
                } else {
                    read + 1
                };
                let deleted = read + usize::from(untracked == read);
                let current = &changes[untracked];
                let removed = &changes[deleted];
                let raw = current.path.as_os_str().as_bytes();
                let path = if current.directory {
                    raw.strip_suffix(b"/").unwrap_or(raw)
                } else {
                    raw
                };
                if current.status != '?'
                    || !removed.index_removed
                    || removed.path.as_os_str().as_bytes() != path
                {
                    return Err("Git status returned a duplicate path.".into());
                }
                untracked
            }
            _ => return Err("Git status returned a duplicate path.".into()),
        };
        changes.swap(write, keep);
        write += 1;
        read = end;
    }
    changes.truncate(write);
    budget.check()
}

fn relative(bytes: &[u8], budget: &Budget<'_>) -> Result<PathBuf, String> {
    let path = Path::new(std::ffi::OsStr::from_bytes(bytes));
    if path.as_os_str().is_empty() {
        return Err("Git status returned an invalid path.".into());
    }
    for part in path.components() {
        budget.check()?;
        if !matches!(part, Component::Normal(_)) {
            return Err("Git status returned an invalid path.".into());
        }
    }
    Ok(OsString::from_vec(budget.copy(bytes)?).into())
}

#[cfg(test)]
#[path = "gitstatus_tests.rs"]
mod tests;
