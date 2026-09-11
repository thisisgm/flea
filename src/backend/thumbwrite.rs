use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// An exclusive create only loses to a name already on disk, so a handful of tries is already a certainty.
const TEMP_TRIES: usize = 8;
// The entries the rest of the desktop wrote here are 0600, so a published thumbnail matches them.
const TEMP_MODE: u32 = 0o600;
const SUFFIX_BYTES: usize = 8;
// This application's name in the PNG Software key, matching the fail/ namespace in thumbcache.
const SOFTWARE: &str = "flea";
// The three keys a published entry may hold only one of, so any the thumbnailer wrote are stripped before ours go in.
const STAMPED_KEYS: [&str; 3] = ["Thumb::URI", "Thumb::MTime", "Software"];
// The reflected polynomial PNG's CRC-32 uses, and the value that both seeds and inverts the register.
const CRC32_POLYNOMIAL: u32 = 0xEDB8_8320;
const CRC32_SEED: u32 = 0xFFFF_FFFF;
const PNG_SIGNATURE_BYTES: usize = 8;
const CHUNK_LENGTH_BYTES: usize = 4;
const CHUNK_HEADER_BYTES: usize = 8;
const CHUNK_CRC_BYTES: usize = 4;

// A 1x1 greyscale PNG, the smallest the format allows, generated and CRC checked on this box rather than transcribed.
const ONE_PIXEL: [u8; 67] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3A, 0x7E, 0x9B,
    0x55, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x60, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x01, 0xE5, 0x27, 0xDE, 0xFC, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

// The pid is in the name so a shutdown sweep can find this process's temps and no other process's.
fn temp_prefix() -> String {
    format!(".flea-{}-", std::process::id())
}

// A predictable destination is never handed to a child; see AGENTS.md "Predictable path writes".
pub fn exclusive_temp(dir: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::OpenOptionsExt;
    for _ in 0..TEMP_TRIES {
        let candidate = dir.join(format!("{}{}.png", temp_prefix(), random_suffix()?));
        let opened = std::fs::OpenOptions::new().write(true).create_new(true).mode(TEMP_MODE).open(&candidate);
        if opened.is_ok() {
            return Some(candidate);
        }
    }
    None
}

// A worker abandoned at shutdown dies between its temp and its own discard, so the process that made them removes its own; see AGENTS.md "Thumbnail requests".
pub fn sweep_own_temps(dir: &Path) {
    let prefix = temp_prefix();
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".png") {
            let _ = std::fs::remove_file(item.path());
        }
    }
}

// std has no RNG, so the kernel's is read directly, with read_exact because /dev/urandom never reaches EOF.
fn random_suffix() -> Option<String> {
    use std::io::Read;
    let mut buf = [0u8; SUFFIX_BYTES];
    let mut f = std::fs::File::open("/dev/urandom").ok()?;
    f.read_exact(&mut buf).ok()?;
    Some(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

// Sample input, one chunk without its length and CRC: the four type bytes "tEXt" followed by the payload "key\0value".
fn crc32(bytes: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut built = [0u32; 256];
        for (i, entry) in built.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { CRC32_POLYNOMIAL ^ (c >> 1) } else { c >> 1 };
            }
            *entry = c;
        }
        built
    });
    let mut crc = CRC32_SEED;
    for b in bytes {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ CRC32_SEED
}

// Sample output, one PNG tEXt chunk: 4 length bytes big-endian, "tEXt", the payload "key\0value", 4 CRC bytes.
fn text_chunk(key: &str, value: &str) -> Vec<u8> {
    let mut typed: Vec<u8> = b"tEXt".to_vec();
    typed.extend_from_slice(key.as_bytes());
    typed.push(0);
    typed.extend_from_slice(value.as_bytes());
    let payload_len = (typed.len() - CHUNK_LENGTH_BYTES) as u32;
    let mut chunk: Vec<u8> = payload_len.to_be_bytes().to_vec();
    chunk.extend_from_slice(&typed);
    chunk.extend_from_slice(&crc32(&typed).to_be_bytes());
    chunk
}

// Sample input, the head of a PNG: 8 signature bytes, then IHDR as 4 length bytes, "IHDR", 13 data bytes and 4 CRC bytes.
fn insert_after_ihdr(png: &[u8], chunks: &[Vec<u8>]) -> Option<Vec<u8>> {
    let sig = PNG_SIGNATURE_BYTES;
    if png.len() < sig + CHUNK_HEADER_BYTES || &png[sig + CHUNK_LENGTH_BYTES..sig + CHUNK_HEADER_BYTES] != b"IHDR" {
        return None;
    }
    let ihdr_len = u32::from_be_bytes([png[sig], png[sig + 1], png[sig + 2], png[sig + 3]]) as usize;
    let after = sig + CHUNK_HEADER_BYTES + ihdr_len + CHUNK_CRC_BYTES;
    if after > png.len() {
        return None;
    }
    let mut out = Vec::with_capacity(png.len() + chunks.iter().map(|c| c.len()).sum::<usize>());
    out.extend_from_slice(&png[..after]);
    for c in chunks {
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&png[after..]);
    Some(out)
}

// Sample input, one chunk: 4 length bytes big-endian, the 4 type bytes "tEXt", the payload "key\0value", 4 CRC bytes.
fn without_text_keys(png: &[u8], keys: &[&str]) -> Vec<u8> {
    let mut out = Vec::with_capacity(png.len());
    let mut at = PNG_SIGNATURE_BYTES.min(png.len());
    out.extend_from_slice(&png[..at]);
    while at + CHUNK_HEADER_BYTES <= png.len() {
        let len = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let end = match at.checked_add(CHUNK_HEADER_BYTES + CHUNK_CRC_BYTES).and_then(|e| e.checked_add(len)) {
            Some(e) if e <= png.len() => e,
            // corner: bytes that do not parse as a chunk are copied through, because this rewrites an entry and does not validate one.
            _ => break,
        };
        let kind = &png[at + CHUNK_LENGTH_BYTES..at + CHUNK_HEADER_BYTES];
        let payload = &png[at + CHUNK_HEADER_BYTES..end - CHUNK_CRC_BYTES];
        if kind != b"tEXt" || !has_key(payload, keys) {
            out.extend_from_slice(&png[at..end]);
        }
        at = end;
    }
    out.extend_from_slice(&png[at..]);
    out
}

// The tEXt payload is "key\0value", so the key is every byte before the first NUL.
fn has_key(payload: &[u8], keys: &[&str]) -> bool {
    match payload.iter().position(|b| *b == 0) {
        Some(nul) => keys.iter().any(|k| payload[..nul] == *k.as_bytes()),
        None => false,
    }
}

// The three keys the shared cache is read by, which Task 5's lookup reads back out of the file itself.
fn write_stamped(path: &Path, png: &[u8], uri: &str, mtime: i64) -> std::io::Result<()> {
    let chunks = vec![
        text_chunk(STAMPED_KEYS[0], uri),
        text_chunk(STAMPED_KEYS[1], &mtime.to_string()),
        text_chunk(STAMPED_KEYS[2], SOFTWARE),
    ];
    // ffmpegthumbnailer writes a Thumb::URI of its own, and two of one key in one entry is a defect whichever one a reader picks; see AGENTS.md "Thumbnail cache".
    let out = insert_after_ihdr(&without_text_keys(png, &STAMPED_KEYS), &chunks)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "not a png"))?;
    std::fs::write(path, out)
}

// The child wrote a bare PNG in place, so it is read back, stamped and written before the rename publishes it.
pub fn stamp(path: &Path, uri: &str, mtime: i64) -> std::io::Result<()> {
    write_stamped(path, &std::fs::read(path)?, uri, mtime)
}

// A fail marker is a valid 1x1 PNG carrying only the metadata, which is what other applications read.
pub fn write_marker(path: &Path, uri: &str, mtime: i64) -> std::io::Result<()> {
    write_stamped(path, &ONE_PIXEL, uri, mtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::thumbcache::png_text;

    const FIXTURE_MTIME: i64 = 1787790423;

    #[test]
    fn the_crc_matches_the_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    // png_text stops at its first match, so this walks to the end and answers what a last-match reader would see.
    fn last_text(bytes: &[u8], key: &str) -> Option<String> {
        let mut at = PNG_SIGNATURE_BYTES;
        let mut found = None;
        while at + CHUNK_HEADER_BYTES <= bytes.len() {
            let len = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
            let end = at + CHUNK_HEADER_BYTES + len;
            if end + CHUNK_CRC_BYTES > bytes.len() {
                break;
            }
            let payload = &bytes[at + CHUNK_HEADER_BYTES..end];
            if &bytes[at + CHUNK_LENGTH_BYTES..at + CHUNK_HEADER_BYTES] == b"tEXt" && has_key(payload, &[key]) {
                let nul = payload.iter().position(|b| *b == 0).unwrap();
                found = String::from_utf8(payload[nul + 1..].to_vec()).ok();
            }
            at = end + CHUNK_CRC_BYTES;
        }
        found
    }

    // A byte search rather than a chunk walk, because the question is whether the key appears twice anywhere in the file.
    fn occurrences(bytes: &[u8], needle: &str) -> usize {
        bytes.windows(needle.len()).filter(|w| *w == needle.as_bytes()).count()
    }

    #[test]
    fn a_stamp_leaves_exactly_one_of_each_key_it_owns() {
        let dir = TestDir::new("restamp");
        let p = dir.join("theirs.png");
        // What ffmpegthumbnailer actually writes on this box: its own Thumb::URI and Thumb::MTime, plus keys nobody else owns.
        let theirs = insert_after_ihdr(
            &ONE_PIXEL,
            &[
                text_chunk("Thumb::URI", "/theirs/clip.mp4"),
                text_chunk("Thumb::MTime", "1"),
                text_chunk("Thumb::Movie", "clip"),
            ],
        )
        .unwrap();
        std::fs::write(&p, &theirs).unwrap();
        stamp(&p, "file:///ours.mp4", FIXTURE_MTIME).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(occurrences(&bytes, "Thumb::URI"), 1, "two Thumb::URI survived one stamp");
        assert_eq!(occurrences(&bytes, "Thumb::MTime"), 1);
        assert_eq!(occurrences(&bytes, "Software"), 1);
        assert_eq!(occurrences(&bytes, "/theirs/clip.mp4"), 0);
        // A first-match reader and a last-match reader must agree, which is the whole point of there being one chunk.
        assert_eq!(png_text(&bytes, "Thumb::URI"), Some("file:///ours.mp4".to_string()));
        assert_eq!(last_text(&bytes, "Thumb::URI"), Some("file:///ours.mp4".to_string()));
        assert_eq!(last_text(&bytes, "Thumb::MTime"), Some(FIXTURE_MTIME.to_string()));
        // A key this application does not own is left exactly where the thumbnailer put it.
        assert_eq!(png_text(&bytes, "Thumb::Movie"), Some("clip".to_string()));
    }

    #[test]
    fn the_sweep_takes_this_process_temps_and_leaves_everything_else() {
        let dir = TestDir::new("sweep");
        let mine = exclusive_temp(dir.path()).unwrap();
        // A published entry, another process's in-flight temp, and another application's dotfile must all survive.
        let published = dir.join("d41d8cd98f00b204e9800998ecf8427e.png");
        let other_pid = dir.join(".flea-1-0011223344556677.png");
        let other_app = dir.join(".gnome-thumbnail-factory.png");
        for p in [&published, &other_pid, &other_app] {
            std::fs::write(p, b"").unwrap();
        }
        dir.assert_contains(dir.path());
        sweep_own_temps(dir.path());
        let gone = !mine.exists();
        let kept = published.exists() && other_pid.exists() && other_app.exists();
        assert!(gone, "the sweep left this process's own temp behind");
        assert!(kept, "the sweep removed a file it did not create");
    }

    #[test]
    fn a_stamped_png_reads_back_through_the_cache_parser() {
        let dir = TestDir::new("thumbwrite");
        let p = dir.join("m.png");
        write_marker(&p, "file:///tmp/a%20b.jpg", FIXTURE_MTIME).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(png_text(&bytes, "Thumb::URI"), Some("file:///tmp/a%20b.jpg".to_string()));
        assert_eq!(png_text(&bytes, "Thumb::MTime"), Some(FIXTURE_MTIME.to_string()));
        assert_eq!(png_text(&bytes, "Software"), Some(SOFTWARE.to_string()));
    }
}
