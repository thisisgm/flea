use super::opsname::name_refusal;
use super::undo::{ItemIdentity, Step};
use crate::error::{from_io, FleaError};
use std::path::{Path, PathBuf};

const NAME_TRIES: usize = 999;
const NEW_FOLDER: &str = "New Folder";

fn named(where_: &str, path: &Path, msg: &str) -> FleaError {
    FleaError {
        where_: where_.into(),
        path: path.to_string_lossy().into_owned(),
        msg: msg.into(),
    }
}

// Explicit names remain exact; an empty name takes the first free default across all siblings.
pub fn mkdir(parent: &Path, name: &str) -> Result<(PathBuf, Vec<Step>), FleaError> {
    mkdir_with_base(parent, name, None)
}

// Only automatic names use the supplied base; an explicit name always remains exact.
pub fn mkdir_with_base(
    parent: &Path,
    name: &str,
    default_base: Option<&str>,
) -> Result<(PathBuf, Vec<Step>), FleaError> {
    // Relative would resolve against the backend's own working directory, which no listing ever names.
    if !parent.is_absolute() {
        return Err(named("mkdir", parent, "a parent must be an absolute path"));
    }
    let base = if name.is_empty() {
        default_base.unwrap_or(NEW_FOLDER)
    } else {
        name
    };
    if let Some(refusal) = name_refusal(base) {
        return Err(named("mkdir", parent, refusal));
    }
    if let Some(refusal) = super::fsname::refuse_in(parent, base) {
        return Err(named("mkdir", parent, &refusal));
    }
    let dir = if name.is_empty() {
        match free_new_folder(parent, base) {
            Some(d) => d,
            None => {
                return Err(named(
                    "mkdir",
                    parent,
                    "every default folder name here is already taken",
                ))
            }
        }
    } else {
        parent.join(name)
    };
    match std::fs::create_dir(&dir) {
        Ok(()) => Ok((
            dir.clone(),
            vec![Step::MadeDir {
                path: dir.clone(),
                identity: ItemIdentity::inspect(&dir)?,
            }],
        )),
        // create_dir, never create_dir_all: a name already taken is a collision and must never merge.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(named(
            "mkdir",
            &dir,
            "a folder or file with that name already exists",
        )),
        Err(e) => Err(from_io("mkdir", &dir.to_string_lossy(), &e)),
    }
}

// The base and its suffixes skip every entry kind; exclusive creation still decides races.
fn free_new_folder(parent: &Path, base: &str) -> Option<PathBuf> {
    for n in 1..=NAME_TRIES {
        let name = if n <= 1 {
            base.to_string()
        } else {
            format!("{} {}", base, n)
        };
        let candidate = parent.join(name);
        if candidate.symlink_metadata().is_err() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn newfolder_racing_creates_never_share_or_overwrite_a_directory() {
        let d = TestDir::new("newfolder-race");
        let barrier = std::sync::Barrier::new(16);
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..16)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        mkdir_with_base(d.path(), "", Some("新建文件夹"))
                    })
                })
                .collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        let mut made = std::collections::HashSet::new();
        for result in results {
            match result {
                Ok((path, steps)) => {
                    assert!(path.is_dir() && made.insert(path.clone()));
                    assert_eq!(
                        steps,
                        vec![Step::MadeDir {
                            path: path.clone(),
                            identity: ItemIdentity::inspect(&path).unwrap()
                        }]
                    );
                }
                Err(error) => {
                    assert_eq!(error.msg, "a folder or file with that name already exists")
                }
            }
        }
        assert!(!made.is_empty());
    }
}
