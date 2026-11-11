// The per-row extras the preview column names and a listing row does not carry. Like thumb and
// dirsize, only a row a client actually asked for is ever looked at.
use crate::backend::archive::Formats;
use crate::backend::archivelist::{parse_reader, Contents, Entry, ARCHIVE_READ_MS};
use crate::backend::imagesize;
use crate::backend::linecount::count_lines;
use crate::backend::mediaprobe;
use crate::backend::opsreq::OpMsg;
use crate::backend::owner;
use crate::backend::rawembed::{self, is_raw_name, RawFacts};
use crate::backend::rawpreview::{self, Extracted};
use crate::backend::rawtiff;
use crate::backend::regfile::open_regular;
use crate::backend::sandbox;
use crate::json::escape;
use flea::bytename::ExactText as _;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const SIGKILL: i32 = 9;

// A JPEG's Exif APP1 follows its first segment or two, so the photo facts never read past this.
pub(crate) const JPEG_EXIF_READ: u64 = 65_536;

pub struct Meta {
    pub width: u32,
    pub height: u32,
    // EXIF orientation 1 to 8 from the same header read, 1 for anything that names none; 5 to 8 swap the sides.
    pub orientation: u8,
    // Image descriptors in a GIF, counted without decoding; 0 for any other file, a malformed GIF or one past the walk.
    pub frames: u32,
    // Milliseconds, and the sample rate in hertz; both zero for anything that is not media.
    pub duration_ms: u64,
    pub sample_rate: u32,
    // How many entries an archive holds and how big they are unpacked, both exact; zero otherwise.
    pub entries: usize,
    pub unpacked: u64,
    // The first few entry names, which is what the tile lists above its "+ N more" line.
    pub names: Vec<Entry>,
    // True when the listing tool could not read the archive at all, which is not the same thing as
    // an archive holding nothing: the tile says so instead of drawing blank cells.
    pub archive_failed: bool,
    pub lines: u64,
    // True when the line count stopped at the budget rather than at the end of the file.
    pub lines_partial: bool,
    // True when the file could not be opened at all. Zero lines is a real answer for an empty file,
    // so the refusal needs its own flag, the way an unreadable archive needs archive_failed.
    pub lines_failed: bool,
    pub target: String,
    pub target_is_dir: bool,
    // The owning user's login name, or empty when no local account has that uid; see owner.rs.
    pub owner: String,
    // The copy of a camera RAW's embedded JPEG that Quick Look draws, with its pixels; None for any other file and for a RAW holding none.
    pub raw: Option<Extracted>,
    // The camera, lens and exposure facts of a RAW; all absent for everything else.
    pub photo: RawFacts,
}

impl Meta {
    fn empty() -> Meta {
        Meta { width: 0, height: 0, orientation: 1, frames: 0, duration_ms: 0, sample_rate: 0, entries: 0, unpacked: 0, names: Vec::new(), archive_failed: false, lines: 0, lines_partial: false, lines_failed: false, target: String::new(), target_is_dir: false, owner: String::new(), raw: None, photo: RawFacts::default() }
    }
}

// text and media are the client's own hints, taken from the row's icon name; the backend does not
// re-classify a row it has already classified once. media costs a subprocess, so it is only ever
// asked for a row whose kind actually names a duration.
pub fn read(path: &Path, text: bool, media: bool, archive: Option<&Formats>) -> Meta {
    read_in(path, text, media, archive, &rawpreview::cache_dir())
}

// A camera RAW's copy goes in raw_dir, which a test points at its sandbox instead of the user's cache.
pub(crate) fn read_in(path: &Path, text: bool, media: bool, archive: Option<&Formats>, raw_dir: &Path) -> Meta {
    let mut m = Meta::empty();
    m.owner = owner::of(path);
    m.photo = photo(path, false);
    if let Ok(link) = std::fs::read_link(path) {
        m.target = link.to_string_lossy().to_string();
        m.target_is_dir = path.metadata().map(|t| t.is_dir()).unwrap_or(false);
    }
    if let Some(image) = super::imageformats::header(path) {
        m.width = image.width;
        m.height = image.height;
        m.orientation = image.orientation;
        m.frames = imagesize::gif_frames(path);
    }
    if is_raw_name(&path.to_string_lossy()) {
        m.raw = rawpreview::ensure_in(raw_dir, path);
    }
    if let Some(formats) = archive {
        let listed = list_archive(path, formats);
        m.entries = listed.entries;
        m.unpacked = listed.unpacked;
        m.names = listed.names;
        m.archive_failed = listed.failed;
    }
    if media {
        let probed = mediaprobe::probe(path);
        m.duration_ms = probed.duration_ms;
        m.sample_rate = probed.sample_rate;
        // A still image already answered above; only a video's own container fills these in.
        if m.width == 0 {
            m.width = probed.width;
            m.height = probed.height;
        }
    }
    // A file whose header parsed as an image is an image whatever the client hinted, so it is never
    // counted for lines: the newlines in a bitmap are a number nothing should ever be shown.
    if text && m.width == 0 {
        let counted = count_lines(path);
        m.lines = counted.lines;
        m.lines_partial = counted.partial;
        m.lines_failed = counted.failed;
    }
    m
}

// The photo facts of a camera file: a RAW through the RAW walk, and a JPEG (only when jpeg is set) through its first Exif APP1 and the same reader.
pub fn photo(path: &Path, jpeg: bool) -> RawFacts {
    let name = path.to_string_lossy();
    if rawembed::is_raw_name(&name) {
        return rawembed::inspect(path).map(|(_, facts)| facts).unwrap_or_default();
    }
    let lower = name.to_ascii_lowercase();
    if !jpeg || !(lower.ends_with(".jpg") || lower.ends_with(".jpeg")) {
        return RawFacts::default();
    }
    let mut head = Vec::new();
    let Some(file) = open_regular(path) else {
        return RawFacts::default();
    };
    if file.take(JPEG_EXIF_READ).read_to_end(&mut head).is_err() {
        return RawFacts::default();
    }
    exif_segment(&head).map(|tiff| rawtiff::facts_from_slice(tiff, false)).unwrap_or_default()
}

// Sample input: FF D8 | FF E0 len(2) JFIF... | FF E1 len(2) "Exif\0\0" TIFF...; answers the TIFF, cut at the end of the bytes in hand.
fn exif_segment(b: &[u8]) -> Option<&[u8]> {
    let mut at = 2;
    while b.starts_with(&[0xFF, 0xD8]) && b.get(at) == Some(&0xFF) {
        let marker = *b.get(at + 1)?;
        // Fill bytes and the markers that carry no length step by less than a segment.
        if marker == 0xFF || marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            at += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        let len = u16::from_be_bytes([*b.get(at + 2)?, *b.get(at + 3)?]) as usize;
        if len < 2 {
            return None;
        }
        let body = b.get(at + 4..(at + 2 + len).min(b.len()))?;
        if marker == 0xE1 {
            if let Some(tiff) = body.strip_prefix(b"Exif\0\0") {
                return Some(tiff);
            }
        }
        at += 2 + len;
    }
    None
}

// Sample output: {"make":"Canon","model":"Canon EOS R6","exp":[1,250],"fnum":[28,10],"iso":400,"focal":[35,1],"taken":"2026:09:12 18:31:00","w":6000,"h":4000}; null when the file names none of them.
pub fn photo_json(f: &RawFacts) -> String {
    let text = |key: &str, v: &Option<String>| v.as_ref().map(|s| format!(r#""{}":"{}""#, key, escape(s)));
    let ratio = |key: &str, v: Option<(u32, u32)>| v.map(|(n, d)| format!(r#""{}":[{},{}]"#, key, n, d));
    let fields: Vec<String> = [
        text("make", &f.make), text("model", &f.model), text("lens", &f.lens), ratio("exp", f.exposure),
        ratio("fnum", f.f_number), f.iso.map(|v| format!(r#""iso":{}"#, v)), ratio("focal", f.focal_mm), text("taken", &f.taken),
        f.width.zip(f.height).map(|(w, h)| format!(r#""w":{},"h":{}"#, w, h)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if fields.is_empty() { "null".to_string() } else { format!("{{{}}}", fields.join(",")) }
}

// Listing an archive reads its index and extracts nothing, in the same jail every other delegated
// tool here runs in.
fn list_archive(path: &Path, formats: &Formats) -> Contents {
    let failed = Contents { failed: true, ..Default::default() };
    let (inner, spec) = match formats.list_argv(path) {
        Some(v) => v,
        None => return failed,
    };
    if !sandbox::available() {
        return failed;
    }
    let mut full = sandbox::wrap_readonly(&inner, path);
    sandbox::add_status(&mut full, inner.len());
    let mut jailed = match crate::backend::jail::spawn_jailed(&full, |cmd| {
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());
    }) {
        Ok(j) => j,
        Err(_) => return failed,
    };
    // The parser's own deadline cannot fire while it is blocked inside a read, so a tool that stops
    // producing entirely is bounded here instead: one thread, one sleep, one signal, and it stands
    // down the moment the read finishes normally.
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let done = Arc::clone(&done);
        let pid = jailed.child.id() as i32;
        let sandbox_pid = Arc::clone(&jailed.sandbox_pid);
        std::thread::spawn(move || {
            let step = std::time::Duration::from_millis(50);
            let mut waited = std::time::Duration::ZERO;
            let budget = std::time::Duration::from_millis(ARCHIVE_READ_MS);
            while waited < budget {
                if done.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(step);
                waited += step;
            }
            if !done.load(Ordering::Relaxed) {
                // The group ends bwrap and the sandbox pid ends the tool --new-session hid; the child is still ours until wait() reaps it.
                crate::backend::jail::kill_sandbox(&sandbox_pid, pid);
                let _ = crate::procsignal::signal_group(pid, SIGKILL);
            }
        })
    };
    let mut contents = match jailed.child.stdout.take() {
        Some(out) => parse_reader(std::io::BufReader::new(out), &spec),
        None => {
            done.store(true, Ordering::Relaxed);
            let _ = watchdog.join();
            return failed;
        }
    };
    // A read cut short at its deadline leaves the tool alive, so the kills happen BEFORE done stands the watchdog down.
    if contents.failed {
        crate::backend::jail::kill_sandbox(&jailed.sandbox_pid, jailed.child.id() as i32);
        let _ = crate::procsignal::signal_group(jailed.child.id() as i32, SIGKILL);
    }
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    // The tool's own verdict, because an unreadable archive otherwise answers zero entries and the
    // tile cannot tell that apart from a read that has not happened yet.
    match jailed.child.wait() {
        Ok(status) if status.success() => {}
        _ => contents.failed = true,
    }
    // Cleared again here, because the status is only known after parse_until has already returned:
    // a listing whose tool exited non-zero would otherwise ship a partial count beside afailed.
    if contents.failed {
        crate::backend::archivelist::clear_counts(&mut contents);
    }
    contents
}

// Sample output: {"t":"meta","row":4,"w":0,"h":0,"orient":1,"ms":0,"rate":0,"entries":214,"unpacked":3400,"afailed":false,"names":[{"n":"ui","d":true}],"lines":0,"partial":false,"lfailed":false,"frames":0,"photo":null,"target":"","targetdir":false,"raw":null,"owner":"gm"}
pub fn meta_line(row: usize, m: &Meta) -> String {
    let names: Vec<String> = m
        .names
        .iter()
        .map(|e| format!(r#"{{"n":"{}","d":{}}}"#, escape(&e.name), e.is_dir))
        .collect();
    format!(
        r#"{{"t":"meta","row":{},"w":{},"h":{},"orient":{},"ms":{},"rate":{},"entries":{},"unpacked":{},"afailed":{},"names":[{}],"lines":{},"partial":{},"lfailed":{},"frames":{},"photo":{},"target":"{}","targetdir":{},"raw":{},"owner":"{}"}}"#,
        row, m.width, m.height, m.orientation, m.duration_ms, m.sample_rate, m.entries, m.unpacked,
        m.archive_failed, names.join(","), m.lines, m.lines_partial, m.lines_failed, m.frames, photo_json(&m.photo),
        escape(&m.target), m.target_is_dir, raw_json(&m.raw), escape(&m.owner)
    )
}

// Sample output: {"file":"/home/gm/.cache/flea/raw/b98fa4.jpg","w":1620,"h":1080,"orient":1}, or null.
fn raw_json(raw: &Option<Extracted>) -> String {
    match raw {
        Some(e) => format!(r#"{{"file":"{}","w":{},"h":{},"orient":{}}}"#, escape(&e.file.exact_text()), e.width, e.height, e.orientation),
        None => "null".to_string(),
    }
}

// The caller token identifies the exact request even when a new listing reuses the row index.
pub fn meta_reply(row: usize, meta: &Meta, token: usize) -> String {
    let mut line = meta_line(row, meta);
    line.pop();
    line.push_str(&format!(",\"token\":{}}}", token));
    line
}

// Answers on a thread, because a media row costs an ffprobe and the loop waits on nothing.
pub fn spawn(row: usize, path: std::path::PathBuf, text: bool, media: bool,
             archive: Option<std::sync::Arc<Formats>>, token: usize, tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let line = meta_reply(row, &read(&path, text, media, archive.as_deref()), token);
        let _ = tx.send(OpMsg::Meta { line });
    });
}

#[cfg(test)]
#[path = "metareq_photo_tests.rs"]
mod photo_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_echoes_request_identity() {
        let line = meta_reply(4, &Meta::empty(), 27);
        assert!(line.ends_with(",\"token\":27}"));
        assert!(crate::jsondoc::parse(&line).is_ok());
    }
    #[test]
    fn a_raw_copy_rides_the_meta_line_as_an_object_and_anything_else_as_null() {
        assert!(meta_line(3, &Meta::empty()).contains(r#""targetdir":false,"raw":null,"owner""#));
        let mut m = Meta::empty();
        m.raw = Some(Extracted { file: "/c/a \"b\".jpg".into(), width: 1620, height: 1080, orientation: 6 });
        let line = meta_line(3, &m);
        assert!(line.contains(r#""raw":{"file":"/c/a \"b\".jpg","w":1620,"h":1080,"orient":6},"owner""#), "{}", line);
        assert!(crate::jsondoc::parse(&line).is_ok());
    }

    use crate::backend::fifotest::{mkfifo, peek, FifoWriter, BOUND};
    use crate::backend::linecount::LINE_BUDGET;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;

    // Every test here reads a file, never media, so the probe subprocess is never spawned.
    fn read2(path: &std::path::Path, text: bool) -> Meta {
        read(path, text, false, None)
    }

    #[test]
    fn an_image_reports_its_pixels_and_asks_for_no_line_count() {
        let d = TestDir::new("metaimage");
        let p = d.join("shot.png");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&2560u32.to_be_bytes());
        png.extend_from_slice(&1440u32.to_be_bytes());
        std::fs::write(&p, png).unwrap();
        let m = read(&p, false, false, None);
        assert_eq!((m.width, m.height), (2560, 1440));
        assert_eq!(m.lines, 0, "an image is never counted for lines");
    }

    #[test]
    fn a_gif_reports_its_frames_beside_its_pixels_and_a_png_reports_none() {
        let d = TestDir::new("metagifframes");
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&[4, 0, 4, 0, 0, 0, 0]);
        for _ in 0..3 {
            gif.extend_from_slice(&[0x2C, 0, 0, 0, 0, 4, 0, 4, 0, 0, 2, 1, 7, 0]);
        }
        gif.push(0x3B);
        std::fs::write(d.join("clip.gif"), &gif).unwrap();
        let m = read(&d.join("clip.gif"), false, false, None);
        assert_eq!((m.width, m.height, m.frames), (4, 4, 3));
        assert!(meta_line(0, &m).contains(r#""lfailed":false,"frames":3,"photo":null,"target":"#));
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&8u32.to_be_bytes());
        png.extend_from_slice(&8u32.to_be_bytes());
        std::fs::write(d.join("shot.png"), png).unwrap();
        assert_eq!(read(&d.join("shot.png"), false, false, None).frames, 0);
    }

    #[test]
    fn an_image_is_never_counted_for_lines_even_when_the_client_asks() {
        let d = TestDir::new("metaimagelines");
        let p = d.join("shot.png");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&64u32.to_be_bytes());
        png.extend_from_slice(&64u32.to_be_bytes());
        // Bytes that happen to contain newlines, which is what every real bitmap contains.
        png.extend_from_slice(b"\n\n\n\n\n");
        std::fs::write(&p, png).unwrap();
        let m = read(&p, true, false, None);
        assert_eq!((m.width, m.height), (64, 64));
        assert_eq!(m.lines, 0, "the newlines in a bitmap are a number nothing should be shown");
    }

    #[test]
    fn a_text_file_counts_its_lines_including_one_with_no_trailing_newline() {
        let d = TestDir::new("metalines");
        assert_eq!(read2(&d.file("three.txt", "a\nb\nc\n"), true).lines, 3);
        assert_eq!(read2(&d.file("noeol.txt", "a\nb\nc"), true).lines, 3, "the last line still counts");
        assert_eq!(read2(&d.file("one.txt", "single"), true).lines, 1);
        assert_eq!(read2(&d.file("empty.txt", ""), true).lines, 0, "an empty file has no lines");
        assert_eq!(read2(&d.file("blank.txt", "\n"), true).lines, 1);
    }

    #[test]
    fn a_line_count_that_never_happened_says_so_on_the_wire() {
        let d = TestDir::new("metalinesdenied");
        let path = d.file("denied.txt", "a\nb\nc\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let denied = read2(&path, true);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let empty = read2(&d.file("empty.txt", ""), true);
        // These two carried the same three values until lfailed, which read as "this file is empty".
        assert!(meta_line(1, &empty).contains(r#""lines":0,"partial":false,"lfailed":false"#), "{}", meta_line(1, &empty));
        assert!(meta_line(2, &denied).contains(r#""lines":0,"partial":false,"lfailed":true"#), "{}", meta_line(2, &denied));
        // A row nobody asked for a count of never claims a failure either.
        assert!(!read2(&d.file("unasked.txt", "a\n"), false).lines_failed);
    }

    #[test]
    fn a_huge_file_stops_at_the_budget_and_says_the_count_is_partial() {
        let d = TestDir::new("metabudget");
        let body = "x\n".repeat((LINE_BUDGET as usize) / 2 + 1000);
        let p = d.file("huge.log", &body);
        let m = read(&p, true, false, None);
        assert!(m.lines_partial, "a count that stopped at the budget must say so");
        assert!(m.lines > 0);
    }

    #[test]
    fn a_symlink_reports_its_target_and_whether_that_target_is_a_directory() {
        let d = TestDir::new("metalink");
        d.dir("realdir");
        d.file("real.txt", "body");
        std::os::unix::fs::symlink("realdir", d.join("linkdir")).unwrap();
        std::os::unix::fs::symlink("real.txt", d.join("linkfile")).unwrap();
        std::os::unix::fs::symlink("nowhere", d.join("broken")).unwrap();

        let m = read2(&d.join("linkdir"), false);
        assert_eq!(m.target, "realdir");
        assert!(m.target_is_dir, "the mark follows the target, so the column has to know");

        let m = read2(&d.join("linkfile"), false);
        assert_eq!(m.target, "real.txt");
        assert!(!m.target_is_dir);

        let m = read2(&d.join("broken"), false);
        assert_eq!(m.target, "nowhere", "a broken link still tells the truth about where it points");
        assert!(!m.target_is_dir);
    }

    #[test]
    fn a_row_that_is_none_of_those_answers_zeroes_rather_than_failing() {
        let d = TestDir::new("metanothing");
        let m = read2(&d.join("never-existed"), true);
        assert_eq!((m.width, m.height, m.lines), (0, 0, 0));
        assert!(m.target.is_empty());
        assert!(m.owner.is_empty(), "a path that is not there has no owner to name");
    }

    #[test]
    fn a_file_carries_its_owner_and_the_line_puts_it_on_the_wire_escaped() {
        let d = TestDir::new("metaowner");
        let m = read2(&d.file("mine.txt", "body"), false);
        assert!(!m.owner.is_empty(), "the test runner's uid is a local account on this box");
        assert!(meta_line(1, &m).ends_with(&format!(r#""owner":"{}"}}"#, m.owner)));
        let mut odd = Meta::empty();
        odd.owner = "say \"hi\"".to_string();
        assert!(meta_line(1, &odd).ends_with(r#""owner":"say \"hi\""}"#));
    }

    #[test]
    fn an_archive_puts_its_exact_count_and_its_first_names_on_the_wire() {
        let mut m = Meta::empty();
        m.entries = 214;
        m.unpacked = 3400;
        m.names = vec![
            Entry { name: "ui".to_string(), is_dir: true },
            Entry { name: "say \"hi\".txt".to_string(), is_dir: false },
        ];
        let line = meta_line(2, &m);
        assert!(line.contains(r#""entries":214"#), "the count is exact, never a cap: {}", line);
        assert!(line.contains(r#""names":[{"n":"ui","d":true},{"n":"say \"hi\".txt","d":false}]"#),
                "a name is escaped like every other string on this wire: {}", line);
        assert!(line.contains(r#""afailed":false"#));
    }

    #[test]
    fn a_listing_that_failed_says_so_rather_than_answering_zero_entries() {
        let mut m = Meta::empty();
        m.archive_failed = true;
        assert!(meta_line(2, &m).contains(r#""afailed":true"#));
        assert!(meta_line(2, &Meta::empty()).contains(r#""names":[]"#));
    }

    #[test]
    fn the_line_escapes_a_target_like_every_other_string_on_this_wire() {
        let mut m = Meta::empty();
        m.target = "/tmp/say \"hi\"".to_string();
        assert!(meta_line(4, &m).contains(r#""target":"/tmp/say \"hi\"""#));
        assert!(meta_line(4, &m).starts_with(r#"{"t":"meta","row":4,"w":0,"h":0"#));
    }

    #[test]
    fn a_turned_photo_s_orientation_rides_on_the_same_line_and_as_stored_reads_one() {
        assert!(meta_line(4, &Meta::empty()).contains(r#""h":0,"orient":1,"ms""#));
        let d = crate::backend::testdir::TestDir::new("metaorient");
        // FFD8, APP1 "Exif\0\0" with IFD0's one entry naming orientation 6, then SOF0 carrying 4000 x 3000.
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x22];
        v.extend_from_slice(b"Exif\0\0II\x2A\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00");
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x0B, 0xB8, 0x0F, 0xA0]);
        v.extend_from_slice(&[0u8; 8]);
        let path = d.join("phone.jpg");
        std::fs::write(&path, &v).unwrap();
        let m = read(&path, false, false, None);
        assert_eq!((m.width, m.height, m.orientation), (4000, 3000, 6));
        assert!(meta_line(1, &m).contains(r#""w":4000,"h":3000,"orient":6,"#));
    }

    // The real path a meta request takes: run.rs hands spawn these arguments and reads back one OpMsg.
    fn answered(path: &std::path::Path) -> Option<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(7, path.to_path_buf(), true, false, None, 0, tx);
        match rx.recv_timeout(BOUND) {
            Ok(OpMsg::Meta { line }) => Some(line),
            _ => None,
        }
    }

    #[test]
    fn a_meta_request_on_a_fifo_answers_rather_than_leaving_the_row_waiting_forever() {
        let d = TestDir::new("metafifo");
        let lonely = d.join("lonely");
        mkfifo(&lonely);
        let line = answered(&lonely).expect("a fifo with no writer must still answer");
        assert!(line.contains(r#""w":0,"h":0"#), "{}", line);
        assert!(line.contains(r#""lines":0,"partial":false,"lfailed":true"#), "{}", line);
        let fed = d.join("fed");
        mkfifo(&fed);
        // feeding returns only once the bytes are in the pipe, or this case degenerates into the writerless one.
        let mut writer = FifoWriter::feeding(&fed, "a\nb\nc\n", &d.join("wrote"));
        let answer = answered(&fed);
        let left = peek(fed);
        let stopped = writer.stop();
        let line = answer.expect("a fifo with a writer must answer without reading it");
        assert!(line.contains(r#""lines":0,"partial":false,"lfailed":true"#), "{}", line);
        assert_eq!(left, b"a\nb\nc\n".to_vec(), "a meta request must never eat a pipe someone else is reading");
        assert!(stopped, "the writer this test started is killed by its own pid");
    }
}
