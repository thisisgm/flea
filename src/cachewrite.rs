// A render miss may replace an older render, but never an unrelated document at its cache path.
use std::io::Read;
use std::path::Path;

pub(crate) fn allowed(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Err(e) => return e.kind() == std::io::ErrorKind::NotFound,
        Ok(meta) if !meta.is_file() => return false,
        _ => {}
    }
    let Some(file) = crate::backend::regfile::open_regular(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file.take(4096).read_to_end(&mut bytes).is_err() {
        return false;
    }
    // An empty or cut-short render is this writer's damage, so it is a miss and is replaced.
    let ours = |magic: &[u8]| crate::persistdoc::torn(&bytes, magic);
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => ours(b"\x89PNG\r\n\x1a\n"),
        Some("jpg") => ours(b"\xff\xd8\xff"),
        Some("pdf") => ours(b"%PDF-"),
        // Legacy office failure records are plain diagnostic text with no schema.
        Some("txt") => std::str::from_utf8(&bytes)
            .is_ok_and(|s| !s.trim_start().starts_with(['{', '[']) && !s.contains('\0')),
        // A spreadsheet's sheet as LibreOffice's CSV filter writes it: text, never a binary document.
        Some("csv") => !bytes.contains(&0),
        _ => false,
    }
}

pub(crate) fn require(path: &Path) -> std::io::Result<()> {
    if allowed(path) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} is a foreign or unreadable cache document; file was left unchanged",
                path.display()
            ),
        ))
    }
}
