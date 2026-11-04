use super::{md5, thumbcache::Cache};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;

#[cfg(test)]
thread_local! {
    pub(super) static DIRECTORY_READS: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0; 2]) };
}

// These are the backing file URIs used by Trash thumbnail jobs, not their original locations.
pub(crate) fn purge(uris: &[String]) {
    let hashes: HashSet<_> = uris.iter().map(|uri| md5::hex(uri.as_bytes())).collect();
    if hashes.is_empty() {
        return;
    }
    let cache = Cache::new();
    // Unit tests may purge only the marked TestDir that owns their filesystem changes.
    #[cfg(test)]
    if !cache.large_dir().ancestors().any(super::testdir::removable) {
        return;
    }
    let failed = cache.fail_path("").parent().unwrap().to_path_buf();
    for dir in [cache.large_dir(), failed] {
        #[cfg(test)]
        DIRECTORY_READS.with(|reads| {
            let mut counts = reads.get();
            counts[usize::from(dir != cache.large_dir())] += 1;
            reads.set(counts);
        });
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                if error.kind() != ErrorKind::NotFound {
                    eprintln!(
                        "flea: could not read Trash thumbnail cache {}: {error}",
                        dir.display()
                    );
                }
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!("flea: could not read a Trash thumbnail cache entry: {error}");
                    continue;
                }
            };
            let name = entry.file_name();
            let bytes = name.as_bytes();
            let prefix = b"trash-";
            let hash_end = prefix.len() + 32;
            if !bytes.starts_with(prefix)
                || bytes.get(hash_end) != Some(&b'-')
                || !bytes.ends_with(b".png")
            {
                continue;
            }
            let Ok(hash) = std::str::from_utf8(&bytes[prefix.len()..hash_end]) else {
                continue;
            };
            if !hashes.contains(hash) {
                continue;
            }
            if let Err(error) = std::fs::remove_file(entry.path()) {
                if error.kind() != ErrorKind::NotFound {
                    eprintln!(
                        "flea: could not remove Trash thumbnail {}: {error}",
                        entry.path().display()
                    );
                }
            }
        }
    }
}
