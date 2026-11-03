use crate::backend::md5;
use std::path::{Path, PathBuf};

// This application's name in the shared cache's fail/ namespace, per the freedesktop thumbnail spec.
const APP_NAME: &str = "flea";
const LARGE_DIR: &str = "large";
const FAIL_DIR: &str = "fail";
const CACHE_DIR: &str = "thumbnails";
// The PNG text key a fail marker carries to name the thumbnailer table that made it; see AGENTS.md "Thumbnail cache".
pub const TABLE_KEY: &str = "X-Flea-Table";
// The bytes GLib leaves literal in a file URI, measured here across every printable ASCII byte, plus the "/" that separates components and is structure; see AGENTS.md "Thumbnail cache".
const URI_LITERAL_BYTES: &[u8] = b"!$&'()*+,-./:=@_~";
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const CHUNK_LENGTH_BYTES: usize = 4;
const CHUNK_TYPE_BYTES: usize = 4;
const CHUNK_HEADER_BYTES: usize = CHUNK_LENGTH_BYTES + CHUNK_TYPE_BYTES;
const CHUNK_CRC_BYTES: usize = 4;

pub enum Hit {
    Ready(PathBuf),
    Failed,
    Miss,
}

pub struct Cache {
    root: PathBuf,
}

// The shared cache every desktop application on this box reads and writes.
pub fn default_root() -> PathBuf {
    crate::userfile::cache_home().join(CACHE_DIR)
}

impl Cache {
    pub fn new() -> Cache {
        Cache::at(default_root())
    }

    // A caller names the root so a test never writes markers into the operator's real cache.
    pub fn at(root: PathBuf) -> Cache {
        Cache { root }
    }

    // The published entries and the in-flight temps share this directory, and a shutdown sweep needs it without a URI.
    pub fn large_dir(&self) -> PathBuf {
        self.root.join(LARGE_DIR)
    }

    pub fn large_path(&self, uri: &str) -> PathBuf {
        self.large_path_for(uri, None)
    }

    pub fn fail_path(&self, uri: &str) -> PathBuf {
        self.fail_path_for(uri, None)
    }

    // Reviewed Trash objects keep separate immutable image paths; ordinary cache names stay standard.
    pub fn large_path_for(&self, uri: &str, identity: Option<&str>) -> PathBuf {
        self.large_dir().join(cache_name(uri, identity))
    }

    pub fn fail_path_for(&self, uri: &str, identity: Option<&str>) -> PathBuf {
        self.root
            .join(FAIL_DIR)
            .join(APP_NAME)
            .join(cache_name(uri, identity))
    }

    // Order matters: a recorded failure wins over a stale thumbnail; a marker naming another table or no table is retried.
    pub fn lookup(&self, path: &Path, mtime: i64, table: &str) -> Hit {
        self.lookup_for(path, mtime, table, None)
    }

    pub fn lookup_for(&self, path: &Path, mtime: i64, table: &str, identity: Option<&str>) -> Hit {
        let uri = uri_for(path);
        let failed = self.fail_path_for(&uri, identity);
        if let Ok(bytes) = std::fs::read(&failed) {
            // A marker made under another table, or under none, is a miss, so the file is tried again.
            if stamped_mtime(&bytes) == Some(mtime) && png_text(&bytes, TABLE_KEY).as_deref() == Some(table) {
                return Hit::Failed;
            }
        }
        let large = self.large_path_for(&uri, identity);
        if let Ok(bytes) = std::fs::read(&large) {
            if stamped_mtime(&bytes) == Some(mtime) {
                return Hit::Ready(large);
            }
        }
        Hit::Miss
    }
}

fn cache_name(uri: &str, identity: Option<&str>) -> String {
    let key = md5::hex(uri.as_bytes());
    match identity {
        Some(identity) => format!("trash-{}-{}.png", key, md5::hex(identity.as_bytes())),
        None => format!("{}.png", key),
    }
}

fn stamped_mtime(bytes: &[u8]) -> Option<i64> {
    png_text(bytes, "Thumb::MTime").and_then(|v| v.parse().ok())
}

// Sample output, the key GNOME itself wrote for "CleanShot 2026-08-17 at 18.25.25@2x.png": file:///home/gm/Downloads/CleanShot%202026-08-17%20at%2018.25.25@2x.png, so the literal set is GLib's measured one and not RFC 3986 unreserved; see AGENTS.md "Thumbnail cache".
pub fn uri_for(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::from("file://");
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || URI_LITERAL_BYTES.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

// Sample input, a PNG chunk: 4 bytes big-endian length, 4 bytes type "tEXt", the payload "key\0value", 4 bytes CRC.
pub fn png_text(bytes: &[u8], key: &str) -> Option<String> {
    if bytes.len() < PNG_SIGNATURE.len() || bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return None;
    }
    let mut at = PNG_SIGNATURE.len();
    while at + CHUNK_HEADER_BYTES <= bytes.len() {
        let len = u32::from_be_bytes([
            bytes[at],
            bytes[at + 1],
            bytes[at + 2],
            bytes[at + 3],
        ]) as usize;
        let kind = &bytes[at + CHUNK_LENGTH_BYTES..at + CHUNK_HEADER_BYTES];
        let start = at + CHUNK_HEADER_BYTES;
        let end = match start.checked_add(len) {
            Some(e) if e <= bytes.len() => e,
            _ => return None,
        };
        // corner: only uncompressed tEXt is read, zTXt and iTXt are skipped; see AGENTS.md "Thumbnail cache".
        if kind == b"tEXt" {
            let payload = &bytes[start..end];
            if let Some(nul) = payload.iter().position(|b| *b == 0) {
                if &payload[..nul] == key.as_bytes() {
                    return String::from_utf8(payload[nul + 1..].to_vec()).ok();
                }
            }
        }
        if kind == b"IDAT" || kind == b"IEND" {
            return None;
        }
        at = end + CHUNK_CRC_BYTES;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::thumbwrite;
    use std::path::Path;

    #[test]
    fn a_uri_is_percent_encoded_and_absolute() {
        assert_eq!(uri_for(Path::new("/tmp/a.jpg")), "file:///tmp/a.jpg");
        assert_eq!(
            uri_for(Path::new("/tmp/two words.jpg")),
            "file:///tmp/two%20words.jpg"
        );
        assert_eq!(
            uri_for(Path::new("/tmp/a;b#c?d.jpg")),
            "file:///tmp/a%3Bb%23c%3Fd.jpg"
        );
        // A slash separating components is structure and is never encoded.
        assert_eq!(uri_for(Path::new("/a/b/c.jpg")), "file:///a/b/c.jpg");
        assert_eq!(uri_for(Path::new("/tmp/~a.jpg")), "file:///tmp/~a.jpg");
        // A non-ASCII name is escaped one UTF-8 byte at a time, which is what GLib does.
        assert_eq!(uri_for(Path::new("/tmp/caf\u{e9}.jpg")), "file:///tmp/caf%C3%A9.jpg");
    }

    #[test]
    fn a_uri_matches_the_one_gnome_wrote_for_a_real_entry() {
        // GNOME leaves "@" and "(" literal, so an RFC 3986 unreserved set misses this entry outright.
        let p = Path::new("/home/gm/Downloads/CleanShot 2026-08-17 at 18.25.25@2x.png");
        let uri = uri_for(p);
        assert_eq!(
            uri,
            "file:///home/gm/Downloads/CleanShot%202026-08-17%20at%2018.25.25@2x.png"
        );
        assert_eq!(
            Cache::new().large_path(&uri).file_name().unwrap().to_string_lossy(),
            "67471ae1929105f9f387addc0af2eb20.png"
        );
        assert_eq!(uri_for(Path::new("/tmp/a (1).jpg")), "file:///tmp/a%20(1).jpg");
    }

    #[test]
    fn the_cache_filename_is_the_md5_of_the_uri() {
        let c = Cache::new();
        let uri = "file:///home/gm/Videos/screen-recording-2020-01-02_03-04-05.mp4";
        let p = c.large_path(uri);
        assert_eq!(
            p.file_name().unwrap().to_string_lossy(),
            "4b971187c53a6a6ff1925d1147d8dacf.png"
        );
        assert!(p.to_string_lossy().ends_with("/thumbnails/large/4b971187c53a6a6ff1925d1147d8dacf.png"));
    }

    #[test]
    fn the_fail_path_is_namespaced_by_this_application() {
        let c = Cache::new();
        let p = c.fail_path("file:///tmp/a.jpg");
        assert!(p.to_string_lossy().contains("/thumbnails/fail/flea/"));
        assert!(p.to_string_lossy().ends_with(".png"));
    }

    #[test]
    fn a_cache_can_be_rooted_at_a_given_directory() {
        let c = Cache::at(PathBuf::from("/x/y/thumbnails"));
        assert_eq!(
            c.large_path("file:///tmp/a.jpg").parent().unwrap(),
            Path::new("/x/y/thumbnails/large")
        );
        assert_eq!(
            c.fail_path("file:///tmp/a.jpg").parent().unwrap(),
            Path::new("/x/y/thumbnails/fail/flea")
        );
        assert!(default_root().ends_with("thumbnails"));
    }

    #[test]
    fn a_text_chunk_is_read_by_key() {
        // A minimal PNG: signature, then one tEXt chunk holding "Thumb::MTime\x00123".
        let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let payload = b"Thumb::MTime\x00123";
        png.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(payload);
        png.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(png_text(&png, "Thumb::MTime"), Some("123".to_string()));
        assert_eq!(png_text(&png, "Thumb::URI"), None);
    }

    #[test]
    fn a_text_chunk_behind_an_earlier_chunk_is_still_found() {
        // The walk must step over a whole chunk, so the tEXt here sits behind a real 13-byte IHDR.
        let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let ihdr = [0u8; 13];
        png.extend_from_slice(&(ihdr.len() as u32).to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr);
        png.extend_from_slice(&[0, 0, 0, 0]);
        let payload = b"Thumb::MTime\x00123";
        png.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(payload);
        png.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(png_text(&png, "Thumb::MTime"), Some("123".to_string()));
    }

    #[test]
    fn a_truncated_png_is_none_not_a_panic() {
        assert_eq!(png_text(&[], "Thumb::MTime"), None);
        assert_eq!(png_text(&[0x89, b'P', b'N', b'G'], "Thumb::MTime"), None);
        let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&(9999u32).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(b"short");
        assert_eq!(png_text(&png, "Thumb::MTime"), None);
    }

    #[test]
    fn a_missing_entry_is_a_miss() {
        let c = Cache::new();
        assert!(matches!(
            c.lookup(Path::new("/definitely/not/here.jpg"), 0, TABLE),
            Hit::Miss
        ));
    }

    const FIXTURE_SRC: &str = "/home/gm/fixture (1).jpg";

    // A PNG carrying one tEXt chunk, hand assembled because this module has no chunk writer yet.
    fn stamped_png(mtime: i64) -> Vec<u8> {
        let payload = format!("Thumb::MTime\0{}", mtime).into_bytes();
        let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(&payload);
        png.extend_from_slice(&[0, 0, 0, 0]);
        png
    }

    // The table every fixture marker is written under and looked up with, unless a test names another.
    const TABLE: &str = "table-a";

    // A real fail marker for FIXTURE_SRC, written through the production writer; None leaves the table key out, the shape before this key existed.
    fn marker(c: &Cache, mtime: i64, table: Option<&str>) {
        let uri = uri_for(Path::new(FIXTURE_SRC));
        let entry = c.fail_path(&uri);
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        thumbwrite::write_marker(&entry, &uri, mtime, table).unwrap();
    }

    fn lookup_fixture(tag: &str, fail_at: Option<i64>, large_at: Option<i64>, ask: i64) -> (TestDir, Hit) {
        let root = TestDir::new(tag);
        let c = Cache::at(root.path().to_path_buf());
        if let Some(mtime) = fail_at {
            marker(&c, mtime, Some(TABLE));
        }
        if let Some(mtime) = large_at {
            let entry = c.large_path(&uri_for(Path::new(FIXTURE_SRC)));
            std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
            std::fs::write(&entry, stamped_png(mtime)).unwrap();
        }
        (root, c.lookup(Path::new(FIXTURE_SRC), ask, TABLE))
    }

    #[test]
    fn a_recorded_failure_wins_over_a_present_thumbnail() {
        let (_root, hit) = lookup_fixture("thumbcache-order", Some(7), Some(7), 7);
        assert!(matches!(hit, Hit::Failed));
    }

    #[test]
    fn a_thumbnail_stamped_with_another_mtime_is_a_miss() {
        let (_root, hit) = lookup_fixture("thumbcache-stale", None, Some(7), 8);
        assert!(matches!(hit, Hit::Miss));
    }

    #[test]
    fn a_matching_thumbnail_is_ready_and_names_its_file() {
        let (root, hit) = lookup_fixture("thumbcache-ready", None, Some(7), 7);
        let want = Cache::at(root.path().to_path_buf()).large_path(&uri_for(Path::new(FIXTURE_SRC)));
        let got = match hit {
            Hit::Ready(p) => Some(p),
            _ => None,
        };
        assert_eq!(got, Some(want));
    }

    #[test]
    fn a_marker_counts_only_for_the_table_that_made_it() {
        let root = TestDir::new("thumbcache-table");
        let c = Cache::at(root.path().to_path_buf());
        marker(&c, 7, Some("table-a"));
        let src = Path::new(FIXTURE_SRC);
        assert!(matches!(c.lookup(src, 7, "table-a"), Hit::Failed));
        assert!(matches!(c.lookup(src, 7, "table-b"), Hit::Miss));
        assert!(matches!(c.lookup(src, 8, "table-a"), Hit::Miss), "the table matching does not excuse another mtime");
    }

    #[test]
    fn a_marker_with_no_table_key_is_a_miss() {
        let root = TestDir::new("thumbcache-legacy");
        let c = Cache::at(root.path().to_path_buf());
        marker(&c, 7, None);
        assert!(matches!(c.lookup(Path::new(FIXTURE_SRC), 7, "table-a"), Hit::Miss));
    }
}
