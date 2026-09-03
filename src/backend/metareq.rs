// The per-row extras the preview column names and a listing row does not carry. Like thumb and
// dirsize, only a row a client actually asked for is ever looked at.
use crate::backend::archive::Formats;
use crate::backend::archivelist::{parse_reader, Contents, Entry, ARCHIVE_READ_MS};
use crate::backend::imagesize;
use crate::backend::linecount::count_lines;
use crate::backend::mediaprobe;
use crate::backend::opsreq::OpMsg;
use crate::backend::owner;
use crate::backend::sandbox;
use crate::json::escape;
use std::path::Path;
use std::os::unix::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// std offers no way to kill a child from another thread without owning it, so the signal is declared
// here rather than taking a crate, the same call ops.rs makes for renameat2.
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGKILL: i32 = 9;

pub struct Meta {
    pub width: u32,
    pub height: u32,
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
}

impl Meta {
    fn empty() -> Meta {
        Meta { width: 0, height: 0, duration_ms: 0, sample_rate: 0, entries: 0, unpacked: 0, names: Vec::new(), archive_failed: false, lines: 0, lines_partial: false, lines_failed: false, target: String::new(), target_is_dir: false, owner: String::new() }
    }
}

// text and media are the client's own hints, taken from the row's icon name; the backend does not
// re-classify a row it has already classified once. media costs a subprocess, so it is only ever
// asked for a row whose kind actually names a duration.
pub fn read(path: &Path, text: bool, media: bool, archive: Option<&Formats>) -> Meta {
    let mut m = Meta::empty();
    m.owner = owner::of(path);
    if let Ok(link) = std::fs::read_link(path) {
        m.target = link.to_string_lossy().to_string();
        m.target_is_dir = path.metadata().map(|t| t.is_dir()).unwrap_or(false);
    }
    if let Some((w, h)) = imagesize::dimensions(path) {
        m.width = w;
        m.height = h;
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
    let full = sandbox::wrap_readonly(&inner, path);
    let mut child = match std::process::Command::new(&full[0])
        .args(&full[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        // Its own group, so the watchdog can end the whole tree in one signal.
        .process_group(0)
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return failed,
    };
    // The parser's own deadline cannot fire while it is blocked inside a read, so a tool that stops
    // producing entirely is bounded here instead: one thread, one sleep, one signal, and it stands
    // down the moment the read finishes normally.
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let done = Arc::clone(&done);
        let pid = child.id() as i32;
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
                // The GROUP, not the pid: the listing tool runs under bwrap and prlimit, so killing
                // the direct child leaves a grandchild holding the pipe open and the read still
                // blocks. Measured: pid alone left a stalled tool running and the request unbounded.
                // Safe: the child is still ours until wait() reaps it, and done gates that.
                unsafe { kill(-pid, SIGKILL) };
            }
        })
    };
    let mut contents = match child.stdout.take() {
        Some(out) => parse_reader(std::io::BufReader::new(out), &spec),
        None => {
            done.store(true, Ordering::Relaxed);
            let _ = watchdog.join();
            return failed;
        }
    };
    // A read cut short at its deadline is the one path where the tool is still alive, so the group
    // kill has to happen BEFORE the watchdog is stood down. Storing done first disarmed it exactly
    // there and left only child.kill(), which this file's own comment says is not enough: the tool
    // runs under prlimit and bwrap and a grandchild survives it.
    if contents.failed {
        unsafe { kill(-(child.id() as i32), SIGKILL) };
    }
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    // The tool's own verdict, because an unreadable archive otherwise answers zero entries and the
    // tile cannot tell that apart from a read that has not happened yet.
    match child.wait() {
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

// Sample output: {"t":"meta","row":4,"w":0,"h":0,"ms":0,"rate":0,"entries":214,"unpacked":3400,"afailed":false,"names":[{"n":"ui","d":true}],"lines":0,"partial":false,"lfailed":false,"target":"","targetdir":false,"owner":"gm"}
pub fn meta_line(row: usize, m: &Meta) -> String {
    let names: Vec<String> = m
        .names
        .iter()
        .map(|e| format!(r#"{{"n":"{}","d":{}}}"#, escape(&e.name), e.is_dir))
        .collect();
    format!(
        r#"{{"t":"meta","row":{},"w":{},"h":{},"ms":{},"rate":{},"entries":{},"unpacked":{},"afailed":{},"names":[{}],"lines":{},"partial":{},"lfailed":{},"target":"{}","targetdir":{},"owner":"{}"}}"#,
        row, m.width, m.height, m.duration_ms, m.sample_rate, m.entries, m.unpacked,
        m.archive_failed, names.join(","), m.lines, m.lines_partial, m.lines_failed,
        escape(&m.target), m.target_is_dir, escape(&m.owner)
    )
}

// Answers on a thread, because a media row costs an ffprobe and the loop waits on nothing.
pub fn spawn(row: usize, path: std::path::PathBuf, text: bool, media: bool,
             archive: Option<std::sync::Arc<Formats>>, tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || {
        let line = meta_line(row, &read(&path, text, media, archive.as_deref()));
        let _ = tx.send(OpMsg::Meta { line });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
