// Properties describe one entry, never an implicit recursive or cloud-status query.
use super::mountinfo::mount_type_in;
use crate::json::escape;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub(super) fn fields(path: &Path, meta: &Metadata) -> Result<String, String> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    fields_in(path, meta, &mountinfo)
}

fn fields_in(path: &Path, meta: &Metadata, mountinfo: &str) -> Result<String, String> {
    let symlink = meta.file_type().is_symlink();
    let kind = if symlink { "Symbolic link" } else if meta.is_dir() { "Directory" }
               else if meta.is_file() { "File" } else { "Special file" };
    let target = if symlink {
        std::fs::read_link(path).map_err(|e| format!("Could not read symlink: {}.", e))?.to_string_lossy().to_string()
    } else { String::new() };
    let filesystem = entry_path(path).and_then(|p| mount_type_in(&p, mountinfo)).unwrap_or_default();
    // Keep bytes as lstat's value for protocol compatibility, but do not advertise it as folder contents.
    Ok(format!(r#""path":"{}","kind":"{}","directory":{},"symlink":{},"target":"{}","bytes":{},"filesystem":"{}","modified":{},"mode":"{:04o}","owner":"{}","uid":{},"gid":{}"#,
        escape(&path.to_string_lossy()), kind, meta.is_dir(), symlink, escape(&target),
        meta.len(), escape(&filesystem), meta.mtime(), meta.mode() & 0o7777,
        escape(&super::owner::name(meta.uid())), meta.uid(), meta.gid()))
}

// Resolve parent aliases, not the final link: Properties describes that link, not its target.
fn entry_path(path: &Path) -> Option<PathBuf> {
    Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::json::{field_bool, field_str, field_usize};
    use std::os::unix::fs::symlink;

    fn inspect(path: &Path, mounts: &str) -> String {
        format!("{{{}}}", fields_in(path, &path.symlink_metadata().unwrap(), mounts).unwrap())
    }

    #[test]
    fn directory_entry_bytes_are_not_contents_but_empty_file_bytes_are_known() {
        let d = TestDir::new("properties-size");
        let folder = d.dir("folder");
        std::fs::write(folder.join("photo"), vec![0; 16_384]).unwrap();
        let folder_reply = inspect(&folder, "");
        assert!(field_bool(&folder_reply, "directory"));
        assert_eq!(field_usize(&folder_reply, "bytes"), Some(folder.symlink_metadata().unwrap().len() as usize));
        let empty = d.file("empty", "");
        let reply = inspect(&empty, "");
        assert_eq!(field_bool(&reply, "directory"), false);
        assert_eq!(field_usize(&reply, "bytes"), Some(0));
    }

    #[test]
    fn filesystem_is_json_escaped_and_an_unresolvable_parent_is_unknown() {
        let d = TestDir::new("properties-unknown");
        let file = d.file("file", "abc");
        let mounts = "1 0 8:1 / / rw - fuse.odd\"name /dev/a rw\n";
        assert_eq!(field_str(&inspect(&file, mounts), "filesystem").as_deref(), Some("fuse.odd\"name"));
        let vanished = d.join("missing/child");
        let reply = format!("{{{}}}", fields_in(&vanished, &file.symlink_metadata().unwrap(), mounts).unwrap());
        assert_eq!(field_str(&reply, "filesystem").as_deref(), Some(""));
    }

    #[test]
    fn mount_detection_follows_parent_aliases_but_not_the_selected_symlink() {
        let d = TestDir::new("properties-mount");
        let drive = d.dir("My Drive");
        let folder = drive.join("photos");
        std::fs::create_dir(&folder).unwrap();
        let mounts = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:9 / {} rw - fuse.rclone remote: rw\n",
            drive.to_string_lossy().replace(' ', "\\040"));
        let alias = d.join("alias");
        symlink(&drive, &alias).unwrap();
        assert_eq!(field_str(&inspect(&alias.join("photos"), &mounts), "filesystem").as_deref(), Some("fuse.rclone"));
        let link = d.join("link");
        symlink(&folder, &link).unwrap();
        let reply = inspect(&link, &mounts);
        assert_eq!(field_str(&reply, "filesystem").as_deref(), Some("ext4"));
        assert_eq!(field_bool(&reply, "symlink"), true);
        assert_eq!(field_bool(&reply, "directory"), false);
        assert_eq!(field_str(&inspect(&folder, "malformed"), "filesystem").as_deref(), Some(""));
    }
}
