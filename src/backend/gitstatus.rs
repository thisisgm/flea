// A porcelain snapshot shared by the graphical and terminal clients, with no per-row filesystem work.
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::time::Instant;

#[path = "gitbudget.rs"]
pub(super) mod budget;
use budget::{Budget, Records};

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
                    b'1' => 9,
                    b'2' => 10,
                    _ => 11,
                };
                let mut fields = [b"".as_slice(); 11];
                let mut parts = record.splitn(count, |byte| *byte == b' ');
                for field in &mut fields[..count] {
                    *field = parts
                        .next()
                        .ok_or("Git status returned an invalid record.")?;
                }
                if fields[0] != &record[..1]
                    || fields[1].len() != 2
                    || fields[1].iter().any(|byte| !b".MADRCUT".contains(byte))
                {
                    return Err("Git status returned an invalid record.".into());
                }
                let xy = fields[1];
                // A rename or copy adds its destination; the source remains metadata.
                let status = if xy.contains(&b'D') {
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
                    fields[2].starts_with(b"S"),
                    record[0] == b'1'
                        && xy == b"D."
                        && fields[4] == b"000000"
                        && fields[5] == b"000000",
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
