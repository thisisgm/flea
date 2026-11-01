//! Validate container bytes through the descriptor already held by the Office worker.
use std::{fs::File, os::unix::fs::FileExt, path::Path};

pub fn real(file: &File, path: &Path, size: u64) -> bool {
    flea::ui_logic_pv2::office_sniff::is_real_container(
        &path.to_string_lossy(),
        size,
        &mut |at, length| {
            let mut bytes = vec![0; length];
            let read = file.read_at(&mut bytes, at).ok()?;
            bytes.truncate(read);
            Some(bytes)
        },
    )
}
