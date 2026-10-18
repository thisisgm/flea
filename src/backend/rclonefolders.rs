// Remote capabilities decide whether selected folders can be delivered without dropping empty descendants.
use crate::jsondoc::{parse, Json};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(super) struct Features {
    pub hashes: bool,
    pub empty_folders: bool,
}

impl Features {
    pub fn read(remote: &str, cancel: &AtomicBool) -> Result<Self, String> {
        let output = super::rcloneproc::run(
            &[
                "backend".into(),
                "features".into(),
                "--".into(),
                format!("{}:", remote),
            ],
            cancel,
            Some(Duration::from_secs(15)),
            true,
            |_| {},
        )?;
        let value =
            parse(&output.text).map_err(|_| "Could not read rclone remote capabilities.")?;
        if !output.success {
            return Err("Could not read rclone remote capabilities.".into());
        }
        let hashes = value
            .get("Hashes")
            .and_then(Json::as_array)
            .ok_or("Could not read rclone checksum support.")?;
        let hashes = hashes.iter().filter_map(Json::as_str).any(|hash| {
            matches!(
                hash,
                "md5"
                    | "sha1"
                    | "sha256"
                    | "crc32"
                    | "whirlpool"
                    | "dropbox"
                    | "mailru"
                    | "quickxor"
                    | "sha512"
                    | "xxh3"
                    | "xxh128"
                    | "blake3"
            )
        });
        let empty_folders = value
            .get("Features")
            .and_then(|features| features.get("CanHaveEmptyDirectories"))
            .and_then(Json::as_bool)
            .unwrap_or(false);
        Ok(Self {
            hashes,
            empty_folders,
        })
    }

    pub fn folder(&self, path: &Path, remote: &str, cancel: &AtomicBool) -> Result<(), String> {
        validate_folder(path, remote, self.empty_folders, cancel)
    }

    pub fn create(&self, target: &str, cancel: &AtomicBool) -> Result<(), String> {
        if !self.empty_folders {
            return Ok(());
        }
        let output = super::rcloneproc::run(
            &[
                "mkdir".into(),
                "--use-json-log".into(),
                "--dry-run=false".into(),
                "--".into(),
                target.into(),
            ],
            cancel,
            None,
            false,
            |_| {},
        )?;
        if output.success {
            Ok(())
        } else {
            Err(format!("Could not create the folder {}.", target))
        }
    }
}

fn validate_folder(
    path: &Path,
    remote: &str,
    empty_folders: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut pending = vec![path.to_path_buf()];
    while let Some(path) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err("Send cancelled.".into());
        }
        let mut empty = true;
        let error = |cause| format!("Could not inspect {}: {}.", path.display(), cause);
        for entry in std::fs::read_dir(&path).map_err(error)? {
            if cancel.load(Ordering::Relaxed) {
                return Err("Send cancelled.".into());
            }
            let entry = entry.map_err(error)?;
            empty = false;
            let kind = entry.file_type().map_err(error)?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if !kind.is_file() {
                return Err(format!(
                    "Cannot send {}: rclone supports only regular files and folders.",
                    entry.path().display()
                ));
            }
        }
        if empty && !empty_folders {
            return Err(format!(
                "{} is an empty folder that {} cannot store.",
                path.display(),
                remote
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn rclone_refuses_empty_folders_when_the_remote_cannot_store_them() {
        let root = TestDir::new("rclone-empty");
        let selected = root.dir("selected");
        let cancel = AtomicBool::new(false);
        let remote = Features {
            hashes: true,
            empty_folders: false,
        };
        assert!(remote
            .folder(&selected, "object-store", &cancel)
            .unwrap_err()
            .contains("selected"));
        root.file("selected/payload", "content");
        assert!(remote.folder(&selected, "object-store", &cancel).is_ok());
        root.dir("selected/child/empty");
        assert!(remote.folder(&selected, "object-store", &cancel).is_err());
        let directories = Features {
            hashes: true,
            empty_folders: true,
        };
        assert!(directories.folder(&selected, "local", &cancel).is_ok());
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            remote
                .folder(&selected, "object-store", &cancel)
                .unwrap_err(),
            "Send cancelled."
        );
    }
}
