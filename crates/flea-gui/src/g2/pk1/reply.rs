//! The portal reply's atomic write: a sibling created exclusively, synced, then renamed over the reply path.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const ATTEMPTS: usize = 16;

// The portal reads the reply only after this process exits, so the file it finds is whole or absent.
pub fn write(path: &Path, text: &str) -> io::Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the reply path names no file",
        ));
    };
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let mut last = io::Error::new(io::ErrorKind::AlreadyExists, "no free temporary name");
    for _ in 0..ATTEMPTS {
        let mut temporary = PathBuf::from(parent);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        temporary.push(format!(
            ".{}.{}.{serial}.tmp",
            name.to_string_lossy(),
            std::process::id()
        ));
        // Created exclusively and owner-only, so a planted name or link is never written through.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last = error;
                continue;
            }
            Err(error) => return Err(error),
        };
        let written = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .and_then(|()| fs::rename(&temporary, path));
        if written.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return written;
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn a_reply_replaces_the_old_file_whole_and_leaves_no_temporary() {
        let root = std::env::temp_dir().join(format!("flea-pk1-reply-{}", std::process::id()));
        fs::create_dir(&root).expect("a fresh scratch root");
        fs::write(root.join(".flea-test-sandbox"), "").expect("the marker");
        let path = root.join("reply.json");
        fs::write(&path, "{\"response\":1").expect("a torn earlier reply");
        // Held open, so the old inode cannot be handed to the replacement.
        let mut old = fs::File::open(&path).expect("the earlier reply");
        let old_inode = old.metadata().expect("its inode").ino();
        write(&path, "{\"response\":0,\"uris\":[]}").expect("the reply is written");
        let mut kept = String::new();
        old.read_to_string(&mut kept).expect("the old bytes");
        assert_eq!(
            kept, "{\"response\":1",
            "the old file was never written in place"
        );
        assert_ne!(
            fs::metadata(&path).expect("the reply").ino(),
            old_inode,
            "the reply arrived by rename"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("the reply"),
            "{\"response\":0,\"uris\":[]}"
        );
        let mut names: Vec<_> = fs::read_dir(&root)
            .expect("the root")
            .flatten()
            .map(|e| e.file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [".flea-test-sandbox", "reply.json"],
            "no temporary is left beside the reply"
        );
        if root.is_absolute()
            && root.starts_with(std::env::temp_dir())
            && root.join(".flea-test-sandbox").is_file()
        {
            let _ = fs::remove_dir_all(&root);
        }
    }
}
