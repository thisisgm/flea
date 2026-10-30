// A copy can also carry one file's own bytes, so a chat app or a browser pastes the file and not its path.
use super::format;
use std::io::Read;
use std::path::Path;

// The payload field that asks for it: a path always starts with '/', so this never reads as one.
pub const FLAG: &str = "+content";
// The owner holds the bytes until a newer copy replaces it, so a larger file is offered by reference only.
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
const UNKNOWN: &str = "application/octet-stream";

// One regular file's MIME type and bytes, or None for anything else: several paths, a folder, a fifo,
// a file past MAX_BYTES, or a type the path list already answers (a .txt is text/plain).
pub fn read(paths: &[String]) -> Option<(String, Vec<u8>)> {
    let [path] = paths else { return None; };
    let mime = crate::backend::mime::Db::load().lookup(path).unwrap_or(UNKNOWN).to_string();
    read_as(Path::new(path), mime)
}

// The same, for a path whose MIME type the caller already has; the tests name a type without the glob table.
pub(crate) fn read_as(path: &Path, mime: String) -> Option<(String, Vec<u8>)> {
    if format::payloads("copy", &[], "").iter().any(|(offered, _)| *offered == mime) {
        return None;
    }
    let (file, meta) = crate::backend::regfile::open_if_regular_with_meta(path, 0).ok()?;
    if meta.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    // A file that grew past its stat is still capped: one byte over the limit refuses it.
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    Some((mime, bytes))
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
