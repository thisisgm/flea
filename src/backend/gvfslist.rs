// A gvfs FUSE directory lists through one gio child; see AGENTS.md "Two-phase listing".
use crate::backend::child::{wait_for_exit, Woke};
use crate::backend::extclass;
use crate::backend::listing::{GioMeta, GioTarget, Listing};
use flea::bytename::ExactText as _;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

// Sample input: gio list -u -a standard::type,standard::size,time::modified,standard::symlink-target,unix::mode --nofollow-symlinks -h <path>
const GIO_ATTRS: &str = "standard::type,standard::size,time::modified,standard::symlink-target,unix::mode";
// A 10k NAS folder answers in about 1.1 s; 15 s of no output bounds a hung daemon, never a large listing.
pub const GIO_TIMEOUT: Duration = Duration::from_secs(15);
// Fallback modes for a gio that omits unix::mode, matching what the SMB FUSE stat reports.
const MODE_FILE: u32 = 0o100700;
const MODE_DIR: u32 = 0o40700;
const MODE_LINK: u32 = 0o120777;

pub struct GvfsRow {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mtime: i64,
    pub target: String,
    // Decimal st_mode as gio reported it, None when the backend omitted the key.
    pub unix_mode: Option<u32>,
}

// One prefix check, so local and USB paths take exactly today's code past it.
pub fn is_gvfs(path: &Path) -> bool {
    if extclass::gvfs_class(path).is_some() {
        return true;
    }
    // extclass recognises /run/user/*/gvfs; a session with XDG_RUNTIME_DIR elsewhere keeps the same root.
    match std::env::var("XDG_RUNTIME_DIR").ok().filter(|v| !v.is_empty()) {
        Some(root) => path.exact_text().starts_with(&format!("{}/gvfs/", root)),
        None => false,
    }
}

// The test seam for the gio binary: FLEA_GIO_BIN names a fake in tests, "gio" otherwise.
pub fn gio_bin() -> String {
    std::env::var("FLEA_GIO_BIN").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "gio".to_string())
}

pub fn mode_for(is_dir: bool, is_symlink: bool) -> u32 {
    if is_dir {
        MODE_DIR
    } else if is_symlink {
        MODE_LINK
    } else {
        MODE_FILE
    }
}

// Sample input: "sp%20ace.txt" answers "sp ace.txt"; "%FF" answers lossy rather than failing.
pub fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// Sample input: "tab\\x5ctname" answers "tab\\tname"; a plain "sp ace" passes through.
fn unescape_target(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
            let hi = (bytes[i + 2] as char).to_digit(16);
            let lo = (bytes[i + 3] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// Sample input: "smb://192.168.21.25/isos/flea-b036-nas-1790537810/fix-10k/file-00001.txt\t10\t(regular)\ttime::modified=1790537811"
pub fn parse_line(line: &str, hidden: bool) -> Result<Option<GvfsRow>, String> {
    let mut parts = line.splitn(4, '\t');
    let uri = parts.next().ok_or_else(|| "gio line has no uri".to_string())?;
    let size_str = parts.next().ok_or_else(|| format!("gio line has no size: {}", line))?;
    let type_str = parts.next().ok_or_else(|| format!("gio line has no type: {}", line))?;
    let attrs = parts.next().ok_or_else(|| format!("gio line has no attrs: {}", line))?;
    // The URI is encoded, so a tab or newline in a name cannot split this line; decode only the last segment.
    let slash = uri.rfind('/').ok_or_else(|| format!("gio uri has no slash: {}", uri))?;
    let segment = &uri[slash + 1..];
    if segment.is_empty() {
        return Err(format!("gio uri names nothing: {}", uri));
    }
    let name = percent_decode(segment);
    if name.is_empty() {
        return Err(format!("gio uri decodes to nothing: {}", uri));
    }
    // corner: a dot-prefixed name is dropped before any stat, the same rule scan.rs applies.
    if !hidden && name.starts_with('.') {
        return Ok(None);
    }
    let size: u64 = size_str.parse().map_err(|_| format!("gio size is not a number: {}", size_str))?;
    let (is_dir, is_symlink) = match type_str {
        "(directory)" => (true, false),
        "(symlink)" => (false, true),
        "(regular)" | "(special)" | "(shortcut)" | "(mountable)" | "(unknown)" => (false, false),
        _ => return Err(format!("gio type is not known: {}", type_str)),
    };
    // gio prints standard::* before time::* before unix::*, so the real keys follow the target.
    let mut target = String::new();
    let keys = match (is_symlink, attrs.find("standard::symlink-target=")) {
        (true, Some(at)) => {
            let rest = &attrs[at + "standard::symlink-target=".len()..];
            match rest.rfind(" time::modified=") {
                Some(rel) => {
                    target = unescape_target(&rest[..rel]);
                    &rest[rel + 1..]
                }
                None => {
                    target = unescape_target(rest);
                    ""
                }
            }
        }
        _ => attrs,
    };
    let mtime_key = "time::modified=";
    let mtime_at = keys.find(mtime_key).ok_or_else(|| format!("gio attrs name no mtime: {}", attrs))?;
    let mtime_rest = &keys[mtime_at + mtime_key.len()..];
    let mtime_end = mtime_rest.find(' ').unwrap_or(mtime_rest.len());
    let mtime: i64 =
        mtime_rest[..mtime_end].parse().map_err(|_| format!("gio mtime is not a number: {}", mtime_rest))?;
    // corner: only a parsed decimal unix::mode replaces the fallback; absent or garbled keeps it.
    let mode_key = "unix::mode=";
    let unix_mode = keys.find(mode_key).and_then(|at| {
        let rest = &keys[at + mode_key.len()..];
        rest[..rest.find(' ').unwrap_or(rest.len())].parse::<u32>().ok()
    });
    Ok(Some(GvfsRow { name, is_dir, is_symlink, size, mtime, target, unix_mode }))
}

// One gio child per listing; any failure falls back to readdir and says nothing to the user.
pub fn list_via_gio(path: &str, hidden: bool, gio: &str, timeout: Duration) -> Result<(Listing, f64), String> {
    list_via_gio_at(path, hidden, gio, timeout, Instant::now())
}

fn list_via_gio_at(path: &str, hidden: bool, gio: &str, timeout: Duration, t: Instant) -> Result<(Listing, f64), String> {
    let bytes = raw_output(path, gio, timeout)?;
    let text = String::from_utf8(bytes).map_err(|_| "gio output is not UTF-8".to_string())?;
    Ok((build_listing(&text, hidden, path)?, t.elapsed().as_secs_f64() * 1000.0))
}

// Test builds only: the blocking-wait count, so a test pins the drain to the channel events; a release build has none of it.
#[cfg(test)]
thread_local! {
    static RECV_WAITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_recv() {
    RECV_WAITS.with(|v| v.set(v.get() + 1));
}

#[cfg(not(test))]
fn count_recv() {}

// The child half the prefetch subcommand shares: the same argv, the same idle deadline, raw bytes out.
// Sample input: path "/run/user/1000/gvfs/smb-share:server=x,share=y/dir".
pub(crate) fn raw_output(path: &str, gio: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    // -h rides on every call so gio's own hidden rule never forks the rows; parse_line's dot filter stays the only hidden rule.
    let mut argv = vec!["list".to_string(), "-u".to_string(), "-a".to_string(), GIO_ATTRS.to_string(), "--nofollow-symlinks".to_string(), "-h".to_string()];
    argv.push(path.to_string());
    let mut child = crate::backend::locale::command(gio)
        .args(flea::bytename::args(&argv))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("gio did not start: {}", e))?;
    // The reader sends one message per chunk and closes at EOF, so the wait below is exact.
    let stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        if let Some(mut out) = stdout {
            use std::io::Read;
            let mut chunk = [0u8; 8192];
            loop {
                match out.read(&mut chunk) {
                    Ok(0) => break,
                    // A gone receiver means the idle deadline fired; the kill below ends this read.
                    Ok(n) => {
                        if tx.send(chunk[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
    });
    // Each recv rearms the idle deadline, so a large listing never trips it, only a quiet one.
    let mut bytes = Vec::new();
    loop {
        count_recv();
        match rx.recv_timeout(timeout) {
            Ok(chunk) => bytes.extend_from_slice(&chunk),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            // A hung daemon must not strand the listing: kill and reap, then refuse.
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("gio list timed out".to_string());
            }
        }
    }
    // EOF comes with gio's exit; one that closed its output and lingers is killed at the same deadline.
    let woke = wait_for_exit(&child, Instant::now() + timeout);
    if !matches!(woke, Woke::Exited) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(if let Woke::Failed(e) = woke { format!("gio wait failed: {}", e) } else { "gio list timed out".to_string() });
    }
    // The pidfd says only that gio exited, so the status itself still comes from wait.
    let status = child.wait().map_err(|e| format!("gio wait failed: {}", e))?;
    // The sender is dropped, so a join that fails means the reader itself panicked.
    reader.join().map_err(|_| "gio reader failed".to_string())?;
    if !status.success() {
        return Err(format!("gio list exited {}", status));
    }
    Ok(bytes)
}

// The parse half the prefetch adoption shares: gio's text in, the listing with its store out.
// Sample input: "smb://h/share/a.txt\t3\t(regular)\ttime::modified=100\n"
pub(crate) fn build_listing(text: &str, hidden: bool, path: &str) -> Result<Listing, String> {
    let base_dev: u64 = std::fs::metadata(path).map(|m| m.dev()).unwrap_or(0);
    let mut l = Listing::new();
    l.base_dev = base_dev;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        match parse_line(line, hidden)? {
            None => {}
            Some(row) => {
                // A reported unix::mode is the backend's own stat; without it the SMB constants stay.
                let mode = row.unix_mode.unwrap_or_else(|| mode_for(row.is_dir, row.is_symlink));
                let index = l.len();
                l.push(&row.name, row.is_dir);
                l.spans[index].is_symlink = row.is_symlink;
                let name_off = l.spans[index].off;
                l.gio_meta.push(GioMeta { name_off, mode, size: row.size, mtime: row.mtime });
                if row.is_symlink && !row.target.is_empty() {
                    l.gio_targets.push(GioTarget { name_off, target: row.target });
                }
                // Nothing else from this row's parse outlives the iteration: the name is in the arena, the figures in the record.
            }
        }
    }
    l.gio_meta.shrink_to_fit();
    l.gio_targets.shrink_to_fit();
    Ok(l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    const NAS: &str = "smb://192.168.21.25/isos/flea-b036-nas-1790537810/fix-10k";

    #[test]
    fn a_regular_row_parses_with_size_and_mtime() {
        let row = parse_line(&format!("{}/file-00001.txt\t10\t(regular)\ttime::modified=1790537811", NAS), false)
            .unwrap().expect("regular row");
        assert_eq!(row.name, "file-00001.txt");
        assert_eq!((row.size, row.mtime), (10, 1790537811));
        assert!(!row.is_dir && !row.is_symlink);
        assert!(row.target.is_empty());
        assert_eq!(row.unix_mode, None, "no unix::mode key means the SMB fallback stays");
        assert_eq!(mode_for(row.is_dir, row.is_symlink), 0o100700);
    }

    #[test]
    fn a_directory_row_parses_with_its_mode() {
        let row = parse_line(&format!("{}/sub\t4096\t(directory)\ttime::modified=1790537811", NAS), false)
            .unwrap().expect("directory row");
        assert_eq!(row.name, "sub");
        assert!(row.is_dir);
        assert_eq!(mode_for(row.is_dir, row.is_symlink), 0o40700);
    }

    #[test]
    fn a_symlink_row_carries_its_target() {
        let row = parse_line(&format!("{}/link1\t5\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=a.txt time::modified=1790537811", NAS), false)
            .unwrap().expect("symlink row");
        assert_eq!((row.name.as_str(), row.target.as_str()), ("link1", "a.txt"));
        assert!(row.is_symlink && !row.is_dir);
        assert_eq!(mode_for(row.is_dir, row.is_symlink), 0o120777);
    }

    #[test]
    fn a_percent_encoded_space_decodes_and_a_tab_round_trips() {
        let row = parse_line(&format!("{}/sp%20ace.txt\t0\t(regular)\ttime::modified=1", NAS), false)
            .unwrap().expect("space row");
        assert_eq!(row.name, "sp ace.txt");
        let row = parse_line(&format!("{}/tab%09name.txt\t1\t(regular)\ttime::modified=1", NAS), false)
            .unwrap().expect("tab row");
        assert_eq!(row.name, "tab\tname.txt");
        let row = parse_line(&format!("{}/new%0Aline.txt\t1\t(regular)\ttime::modified=1", NAS), false)
            .unwrap().expect("newline row");
        assert_eq!(row.name, "new\nline.txt");
    }

    #[test]
    fn a_non_utf8_escape_is_lossy_rather_than_a_failure() {
        let row = parse_line(&format!("{}/bad-%FF-name\t0\t(regular)\ttime::modified=1", NAS), false)
            .unwrap().expect("non-utf8 row");
        assert!(row.name.contains('\u{FFFD}'), "expected a replacement char, got {:?}", row.name);
    }

    #[test]
    fn a_hidden_name_is_dropped_unless_hidden_is_true() {
        let line = format!("{}/.hidden\t0\t(regular)\ttime::modified=1", NAS);
        assert!(parse_line(&line, false).unwrap().is_none());
        assert_eq!(parse_line(&line, true).unwrap().expect("hidden kept").name, ".hidden");
    }

    #[test]
    fn garbage_is_a_parse_error_that_falls_back() {
        for line in ["", "garbage", "a\tb", "smb://h/f\t10\t(regular)", "smb://h/f\tnotnum\t(regular)\ttime::modified=1", "smb://h/f\t1\t(bogus)\ttime::modified=1", "smb://h/f\t1\t(regular)\tno-mtime-here"] {
            assert!(parse_line(line, false).is_err(), "expected an error for {:?}", line);
        }
    }

    fn fake_gio(dir: &TestDir, name: &str, body: &str) -> String {
        dir.script(name, body).to_string_lossy().into_owned()
    }

    #[test]
    fn hidden_false_still_asks_gio_for_hidden_names() {
        let d = TestDir::new("gvfs-always-h");
        let fake = fake_gio(&d, "gio", "#!/bin/sh\ncase \" $* \" in *' -h '*) printf '%s\\n' 'smb://h/share/.hidden\t0\t(regular)\ttime::modified=1' 'smb://h/share/seen.txt\t1\t(regular)\ttime::modified=2';; *) exit 3;; esac\n");
        let (l, _) = list_via_gio(d.path().to_str().unwrap(), false, &fake, Duration::from_secs(5)).unwrap();
        assert_eq!(l.len(), 1, "gio sent the dotfile and the dot filter dropped it");
        assert_eq!(l.name(0), "seen.txt");
    }

    #[test]
    fn a_symlink_target_cannot_spoof_mtime_or_mode() {
        let row = parse_line(&format!("{}/l\t5\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=x time::modified=5 unix::mode=16877 time::modified=7 unix::mode=41471", NAS), false).unwrap().expect("spoof row");
        assert_eq!(row.target, "x time::modified=5 unix::mode=16877");
        assert_eq!(row.mtime, 7);
        assert_eq!(row.unix_mode, Some(41471));
    }

    #[test]
    fn a_slow_stream_past_the_old_total_still_lists() {
        let d = TestDir::new("gvfs-slow");
        let fake = fake_gio(&d, "gio", "#!/bin/sh\nprintf '%s\\n' 'smb://h/share/a.txt\t3\t(regular)\ttime::modified=100'\nsleep 0.3\nprintf '%s\\n' 'smb://h/share/b.txt\t4\t(regular)\ttime::modified=200'\nsleep 0.3\nprintf '%s\\n' 'smb://h/share/c.txt\t5\t(regular)\ttime::modified=300'\n");
        let (l, _) = list_via_gio(d.path().to_str().unwrap(), false, &fake, Duration::from_millis(500)).unwrap();
        assert_eq!(l.len(), 3, "progress, not total time, bounds the listing");
    }

    #[test]
    fn a_gio_that_exits_1_falls_back() {
        let d = TestDir::new("gvfs-exit1");
        let fake = fake_gio(&d, "gio", "#!/bin/sh\nexit 1\n");
        let e = list_via_gio("/run/user/1000/gvfs/smb-share:server=x,share=y/dir", false, &fake, Duration::from_secs(5)).unwrap_err();
        assert!(e.contains("exited"), "expected an exit failure, got {}", e);
    }

    #[test]
    fn a_gio_that_hangs_past_the_deadline_falls_back() {
        // One keeps its output open and trips the idle deadline, with no exit wait; the other closes it and lingers, tripping the exit deadline.
        for (body, exit_waits) in [("#!/bin/sh\nsleep 30\n", false), ("#!/bin/sh\nexec >&-\nsleep 30\n", true)] {
            let d = TestDir::new("gvfs-hang");
            let fake = fake_gio(&d, "gio", body);
            crate::backend::child::POLL_WAITS.with(|v| v.set(0));
            let t = Instant::now();
            let e = list_via_gio("/run/user/1000/gvfs/smb-share:server=x,share=y/dir", false, &fake, Duration::from_millis(200)).unwrap_err();
            assert!(e.contains("timed out"), "expected a timeout, got {}", e);
            assert!(t.elapsed() < Duration::from_secs(10), "the deadline must fire, not the sleep");
            assert_eq!(crate::backend::child::POLL_WAITS.with(|v| v.get()) > 0, exit_waits, "which deadline fired: {}", body);
        }
    }

    #[test]
    fn a_gio_exit_is_noticed_when_it_exits_rather_than_at_the_next_poll_boundary() {
        // One recv per chunk plus the one that sees EOF, then one blocking poll on the exit, whatever the sleep before the rows or the linger after them.
        let mut counts = Vec::with_capacity(4);
        for (sleep, linger) in [(0.02, 0.0), (0.12, 0.0), (0.0, 0.02), (0.0, 0.12)] {
            let d = TestDir::new("gvfs-exact-wait");
            // cat sends the heredoc in one write whatever /bin/sh's printf flushes, and exec >&- closes the output before the linger.
            let body = format!("#!/bin/sh\nsleep {sleep:.4}\ncat <<'ROWS'\nsmb://h/share/a.txt\t3\t(regular)\ttime::modified=100\nsmb://h/share/b.txt\t4\t(regular)\ttime::modified=200\nROWS\nexec >&-\nsleep {linger:.4}\n");
            let fake = fake_gio(&d, "gio", &body);
            RECV_WAITS.with(|v| v.set(0));
            crate::backend::child::POLL_WAITS.with(|v| v.set(0));
            let bytes = raw_output("/run/user/1000/gvfs/smb-share:server=x,share=y/dir", &fake, Duration::from_secs(5)).expect("fake gio");
            assert_eq!(String::from_utf8(bytes).expect("fake gio is UTF-8").lines().count(), 2, "the drain answered both rows");
            counts.push((RECV_WAITS.with(|v| v.get()), crate::backend::child::POLL_WAITS.with(|v| v.get())));
        }
        assert_eq!(counts.as_slice(), &[(2, 1); 4], "one chunk plus EOF, then one exit wait, at every sleep and linger");
    }

    #[test]
    fn a_failed_exit_wait_names_its_os_error() {
        let d = TestDir::new("gvfs-waitfail");
        const EMFILE: i32 = 24;
        let fake = fake_gio(&d, "gio", "#!/bin/sh\nexec >&-\nsleep 30\n");
        crate::backend::child::test_fail_wait(Some(EMFILE));
        let e = raw_output("/run/user/1000/gvfs/smb-share:server=x,share=y/dir", &fake, Duration::from_secs(5)).unwrap_err();
        crate::backend::child::test_fail_wait(None);
        assert_eq!(e, format!("gio wait failed: {}", std::io::Error::from_raw_os_error(EMFILE)), "the failure names the OS error it got");
    }

    #[test]
    fn a_gio_that_prints_garbage_falls_back() {
        let d = TestDir::new("gvfs-garbage");
        let fake = fake_gio(&d, "gio", "#!/bin/sh\necho 'this is not a gio line'\n");
        let e = list_via_gio("/run/user/1000/gvfs/smb-share:server=x,share=y/dir", false, &fake, Duration::from_secs(5)).unwrap_err();
        assert!(!e.is_empty(), "garbage must be a parse error");
    }

    #[test]
    fn a_successful_gio_run_builds_the_listing_and_its_cache() {
        let d = TestDir::new("gvfs-success");
        let fake = fake_gio(&d, "gio", "#!/bin/sh\nprintf '%s\\n' 'smb://h/share/a.txt\t3\t(regular)\ttime::modified=100' 'smb://h/share/sub\t4096\t(directory)\ttime::modified=200' 'smb://h/share/l\t1\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=a.txt time::modified=300'\n");
        let (l, _) = list_via_gio(d.path().to_str().unwrap(), false, &fake, Duration::from_secs(5)).unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!((l.name(0), l.name(1), l.name(2)), ("a.txt", "sub", "l"));
        assert!(l.is_dir(1) && !l.is_dir(0) && !l.is_dir(2));
        assert!(l.spans[2].is_symlink, "the symlink bit rides in the span like the readdir path");
        let cached = l.gio_for(2).expect("every row is cached by offset");
        assert_eq!((cached.size, cached.mtime, cached.mode), (1, 300, 0o120777));
        assert_eq!(l.gio_target(cached.name_off), "a.txt");
        assert_eq!(l.gio_for(0).expect("file cached").size, 3);
    }

    #[test]
    fn a_gio_run_reporting_unix_mode_caches_real_modes() {
        let d = TestDir::new("gvfs-unixmode");
        let fake = fake_gio(&d, "gio", "#!/bin/sh\nprintf '%s\\n' 'smb://h/share/plain.txt\t6\t(regular)\ttime::modified=100 unix::mode=33188' 'smb://h/share/l\t6\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=plain.txt time::modified=100 unix::mode=41471' 'smb://h/share/odd.txt\t6\t(regular)\ttime::modified=100 unix::mode=notanumber'\n");
        let (l, _) = list_via_gio(d.path().to_str().unwrap(), false, &fake, Duration::from_secs(5)).unwrap();
        assert_eq!(l.gio_for(0).expect("file cached").mode, 33188, "0644, not the 0700 fallback");
        assert_eq!(l.gio_for(1).expect("link cached").mode, 41471, "the link's own 0o120777");
        assert!(l.spans[1].is_symlink, "the symlink bit still rides in the span");
        assert_eq!(l.gio_for(2).expect("odd row cached").mode, 0o100700, "a garbled mode keeps the fallback");
    }

    #[test]
    fn a_cached_symlink_to_a_folder_reports_target_is_dir() {
        use std::os::unix::fs::symlink;
        let d = TestDir::new("gvfs-linkdir");
        std::fs::create_dir(d.join("realdir")).unwrap();
        symlink("realdir", d.join("linkdir")).unwrap();
        symlink("nowhere", d.join("broken")).unwrap();
        let text = "smb://h/share/realdir\t4096\t(directory)\ttime::modified=100 unix::mode=16832\nsmb://h/share/linkdir\t6\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=realdir time::modified=100 unix::mode=41471\nsmb://h/share/broken\t6\t(symlink)\tstandard::is-symlink=TRUE standard::symlink-target=nowhere time::modified=100 unix::mode=41471\n";
        let l = build_listing(text, false, d.path().to_str().unwrap()).unwrap();
        let (metas, _) = crate::backend::meta::stat_range(d.path(), &l, 0, 3);
        assert!(!metas[0].target_is_dir, "a real directory carries d itself, not the symlink flag");
        assert!(metas[1].target_is_dir, "a cached symlink to a folder must draw the folder icon");
        assert!(!metas[2].target_is_dir, "a broken cached link resolves to nothing");
        assert_eq!(metas[1].mtime, 100, "the gio mtime, never the local link's own, so the cache branch answered");
    }

    #[test]
    fn ten_thousand_rows_build_a_compact_store_with_no_slack() {
        use std::fmt::Write as _;
        let d = TestDir::new("gvfs-compact-10k");
        let mut text = String::with_capacity(10000 * 72);
        for i in 0..10000 {
            let _ = writeln!(text, "smb://h/share/file-{i:05}.txt\t{i}\t(regular)\ttime::modified={}", 1790537811 + (i as i64 % 100));
        }
        let l = build_listing(&text, false, d.path().to_str().unwrap()).unwrap();
        assert_eq!(std::mem::size_of::<crate::backend::listing::GioMeta>(), 24, "one record per gio row is 24 bytes");
        assert_eq!(l.gio_meta.len(), 10000);
        assert_eq!(l.gio_meta.capacity(), l.gio_meta.len(), "the store is shrunk to its length");
        assert!(l.gio_meta.windows(2).all(|w| w[0].name_off < w[1].name_off), "build order keeps offsets increasing for the binary search");
    }

    #[test]
    fn only_a_gvfs_path_takes_the_gio_branch() {
        assert!(is_gvfs(Path::new("/run/user/1000/gvfs/smb-share:server=192.168.21.25,share=isos/dir")));
        assert!(!is_gvfs(Path::new("/home/gm")));
        assert!(!is_gvfs(Path::new("/run/user/1000/doc/x")));
    }
}
