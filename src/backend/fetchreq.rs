// Turning a fetch REQUEST into wire lines and a background job: the picker's typed URL is downloaded
// by gio copy into the picker cache, with progress, a cancel that kills the tool, and no partial
// file left behind on either failure. Where the file lands is pickercache.rs.
use crate::backend::opsdispatch::Ops;
use crate::backend::fetchprogress::read_progress;
use crate::backend::opsreq::OpMsg;
use crate::json::escape;
use crate::pickercache::fetch_dest;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// std offers no way to kill a child from another thread without owning it, so the signal is declared
// here rather than taking a crate, the same call metareq.rs makes for its watchdog.
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

const SIGKILL: i32 = 9;
// gio already carries the box's http, ftp and gvfs legs and is a hard dependency for trash, so the
// crate takes no HTTP code of its own.
const GIO: &str = "gio";
// The schemes gio copy reads without a mount. A share is not here: the picker mounts it through the
// Network rail's own flow and browses the fuse path, so nothing is copied.
const SCHEMES: [&str; 5] = ["http", "https", "ftp", "ftps", "file"];
const REFUSED: &str = "scheme not accepted: only http, https, ftp, ftps and file";
const CANCELLED: &str = "cancelled";
// How often the watcher looks at the cancel flag while the tool runs.
const CANCEL_POLL: Duration = Duration::from_millis(50);

pub fn fetchstarted_line(id: usize, uri: &str) -> String {
    format!(r#"{{"t":"fetchstarted","id":{},"uri":"{}"}}"#, id, escape(uri))
}

// total is 0 when the server did not say, the same rule transferprogress uses for a directory.
pub fn fetchprogress_line(id: usize, bytes: u64, total: u64) -> String {
    format!(r#"{{"t":"fetchprogress","id":{},"bytes":{},"total":{}}}"#, id, bytes, total)
}

// path rides only on a success and err only on a failure, so neither line carries an empty field.
pub fn fetchdone_line(id: usize, ok: bool, path: &str, err: &str) -> String {
    if ok {
        return format!(r#"{{"t":"fetchdone","id":{},"ok":true,"path":"{}"}}"#, id, escape(path));
    }
    format!(r#"{{"t":"fetchdone","id":{},"ok":false,"err":"{}"}}"#, id, escape(err))
}

pub fn scheme_accepted(uri: &str) -> bool {
    match uri.split_once("://") {
        Some((scheme, _)) => SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()),
        None => false,
    }
}

// The last path segment, without query or fragment; fetch_dest decodes it and refuses what could
// leave its dir. A URL with no path at all names no file, and the empty leaf becomes "download".
pub fn uri_leaf(uri: &str) -> &str {
    let no_query = uri.split(['?', '#']).next().unwrap_or("");
    let after_scheme = no_query.split_once("://").map_or(no_query, |(_, rest)| rest);
    match after_scheme.split_once('/') {
        Some((_, path)) => path.trim_end_matches('/').rsplit('/').next().unwrap_or(""),
        None => "",
    }
}

fn last_line(err: Option<impl Read>) -> String {
    let mut text = String::new();
    if let Some(mut err) = err {
        let _ = err.read_to_string(&mut text);
    }
    text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string()
}

// The file and its own dir, which fetch_dest made for this download alone; remove_dir is not
// recursive, so a dir holding anything else stays.
fn remove_partial(dest: &Path) {
    let _ = std::fs::remove_file(dest);
    if let Some(dir) = dest.parent() {
        let _ = std::fs::remove_dir(dir);
    }
}

// Runs the tool to its end or to the cancel and answers the terminal line. program is a parameter so
// a test can stand a script in for gio without touching PATH, which every thread in the test binary shares.
pub fn fetch_with(program: &str, id: usize, uri: &str, dest: &Path, cancel: &Arc<AtomicBool>, tx: &Sender<OpMsg>) -> String {
    let spawned = Command::new(program)
        .args(["copy", "-p", uri])
        .arg(dest)
        // The progress text is parsed, so it has to be the English one.
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own group, so the cancel ends the whole tree in one signal.
        .process_group(0)
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            remove_partial(dest);
            return fetchdone_line(id, false, "", &format!("cannot run {}: {}", program, e));
        }
    };
    // The reader below blocks inside a read, so the cancel is watched from a thread of its own,
    // stood down the moment the read finishes. Safe: the child is still ours until wait() reaps it.
    let done = Arc::new(AtomicBool::new(false));
    let watcher = {
        let done = Arc::clone(&done);
        let cancel = Arc::clone(cancel);
        let pid = child.id() as i32;
        thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    unsafe { kill(-pid, SIGKILL) };
                    return;
                }
                thread::sleep(CANCEL_POLL);
            }
        })
    };
    let stderr = child.stderr.take();
    let errors = thread::spawn(move || last_line(stderr));
    if let Some(stdout) = child.stdout.take() {
        read_progress(stdout, |bytes, total| {
            let _ = tx.send(OpMsg::Meta { line: fetchprogress_line(id, bytes, total) });
        });
    }
    // Stood down before the wait: the pipes closed, so the child is a zombie no new process can be
    // numbered as, and a late kill would hit nothing.
    done.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    let last_err = errors.join().unwrap_or_default();
    let status = child.wait();
    if cancel.load(Ordering::Relaxed) {
        remove_partial(dest);
        return fetchdone_line(id, false, "", CANCELLED);
    }
    match status {
        Ok(s) if s.success() => fetchdone_line(id, true, &dest.to_string_lossy(), ""),
        _ => {
            remove_partial(dest);
            let err = if last_err.is_empty() { format!("{} copy failed", program) } else { last_err };
            fetchdone_line(id, false, "", &err)
        }
    }
}

// The dispatch half. Every fetch answers exactly one fetchstarted and one fetchdone, a refused one
// too, so a client keys the pair on the id and never waits on a request that was turned down.
pub fn start_fetch(out: &mut impl Write, ops: &mut Ops, uri: &str) {
    let id = ops.claim_id();
    writeln!(out, "{}", fetchstarted_line(id, uri)).ok();
    if !scheme_accepted(uri) {
        writeln!(out, "{}", fetchdone_line(id, false, "", REFUSED)).ok();
        out.flush().ok();
        return;
    }
    let dest = match fetch_dest(uri_leaf(uri)) {
        Ok(d) => d,
        Err(e) => {
            writeln!(out, "{}", fetchdone_line(id, false, "", &format!("cannot make the picker cache: {}", e))).ok();
            out.flush().ok();
            return;
        }
    };
    out.flush().ok();
    let cancel = Arc::new(AtomicBool::new(false));
    ops.fetches.insert(id, Arc::clone(&cancel));
    let tx = ops.tx.clone();
    let uri = uri.to_string();
    thread::spawn(move || {
        let line = fetch_with(GIO, id, &uri, &dest, &cancel, &tx);
        let _ = tx.send(OpMsg::FetchDone { id, line });
    });
}

// No response line of its own: the fetch answers with its own fetchdone carrying err "cancelled".
pub fn cancel_fetch(ops: &Ops, id: usize) {
    if let Some(flag) = ops.fetches.get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::proto::{parse_request, Request};
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc::channel;

    // A stand-in for gio whose argv is gio's: copy -p <uri> <dest>.
    fn stub(d: &TestDir, body: &str) -> String {
        let p = d.file("gio", &format!("#!/bin/sh\n{}\n", body));
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        p.to_string_lossy().to_string()
    }

    fn dest(d: &TestDir) -> std::path::PathBuf {
        d.dir("cache/0badf00d").join("leaf.bin")
    }

    #[test]
    fn every_line_matches_the_shape_of_the_transfer_lines() {
        assert_eq!(fetchstarted_line(7, "https://x.org/a.pdf"), r#"{"t":"fetchstarted","id":7,"uri":"https://x.org/a.pdf"}"#);
        assert_eq!(fetchprogress_line(7, 1200000, 5000000), r#"{"t":"fetchprogress","id":7,"bytes":1200000,"total":5000000}"#);
        assert_eq!(fetchdone_line(7, true, "/home/gm/.cache/flea/picker/ab/a.pdf", ""),
                   r#"{"t":"fetchdone","id":7,"ok":true,"path":"/home/gm/.cache/flea/picker/ab/a.pdf"}"#);
        assert_eq!(fetchdone_line(7, false, "", "say \"no\""), r#"{"t":"fetchdone","id":7,"ok":false,"err":"say \"no\""}"#);
    }

    #[test]
    fn the_two_requests_parse_and_a_missing_field_is_empty_or_zero() {
        match parse_request(r#"{"c":"fetch","uri":"https://x.org/a%20b.pdf?x=1"}"#) {
            Request::Fetch { uri } => assert_eq!(uri, "https://x.org/a%20b.pdf?x=1"),
            _ => panic!("expected Fetch"),
        }
        assert!(matches!(parse_request(r#"{"c":"fetch"}"#), Request::Fetch { uri } if uri.is_empty()));
        match parse_request(r#"{"c":"fetchcancel","id":9}"#) {
            Request::FetchCancel { id } => assert_eq!(id, 9),
            _ => panic!("expected FetchCancel"),
        }
        assert!(matches!(parse_request(r#"{"c":"fetchcancel"}"#), Request::FetchCancel { id: 0 }));
    }

    #[test]
    fn only_the_five_schemes_pass_and_a_share_does_not() {
        for uri in ["http://x/a", "https://x/a", "ftp://x/a", "ftps://x/a", "file:///tmp/a", "HTTPS://x/a"] {
            assert!(scheme_accepted(uri), "{}", uri);
        }
        for uri in ["smb://host/share/a", "sftp://host/a", "ssh://host/a", "", "/tmp/a", "http:/x", "javascript://x"] {
            assert!(!scheme_accepted(uri), "{}", uri);
        }
    }

    #[test]
    fn the_leaf_is_the_last_segment_without_query_or_fragment() {
        assert_eq!(uri_leaf("https://x.org/dir/a%20b.pdf?dl=1#page"), "a%20b.pdf");
        assert_eq!(uri_leaf("file:///tmp/a.txt"), "a.txt");
        assert_eq!(uri_leaf("http://x.org/dir/"), "dir");
        assert_eq!(uri_leaf("http://x.org"), "");
        assert_eq!(uri_leaf("http://x.org/"), "");
        assert_eq!(uri_leaf(""), "");
    }

    #[test]
    fn a_failing_tool_answers_its_last_stderr_line_and_leaves_no_dir() {
        let d = TestDir::new("fetchfail");
        let gio = stub(&d, r#"echo "gio: $3: first" >&2; echo "gio: $3: Not Found" >&2; exit 1"#);
        let dest = dest(&d);
        let (tx, _rx) = channel();
        let line = fetch_with(&gio, 3, "http://x/leaf.bin", &dest, &Arc::new(AtomicBool::new(false)), &tx);
        assert_eq!(line, r#"{"t":"fetchdone","id":3,"ok":false,"err":"gio: http://x/leaf.bin: Not Found"}"#);
        assert!(!dest.parent().unwrap().exists(), "the fetch's own dir goes with the failure");
    }

    #[test]
    fn a_tool_that_writes_and_exits_well_answers_the_path_and_its_progress() {
        let d = TestDir::new("fetchok");
        let gio = stub(&d, r#"printf 'body' > "$4"; printf '\r\033[KCopied 2 bytes out of 4 bytes (x)\r\033[KCopied 4 bytes (average: x)\n'"#);
        let dest = dest(&d);
        let (tx, rx) = channel();
        let line = fetch_with(&gio, 4, "file:///src/leaf.bin", &dest, &Arc::new(AtomicBool::new(false)), &tx);
        assert_eq!(line, format!(r#"{{"t":"fetchdone","id":4,"ok":true,"path":"{}"}}"#, dest.display()));
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "body");
        match rx.try_recv() {
            Ok(OpMsg::Meta { line }) => assert_eq!(line, r#"{"t":"fetchprogress","id":4,"bytes":2,"total":4}"#),
            _ => panic!("expected one progress line"),
        }
        assert!(rx.try_recv().is_err(), "the Copied summary is not progress");
    }

    #[test]
    fn a_cancel_kills_the_tool_and_removes_the_partial_file() {
        let d = TestDir::new("fetchcancel");
        // sleep is a child of the script, so only a kill of the whole group ends it.
        let gio = stub(&d, r#"printf 'half' > "$4"; sleep 30"#);
        let dest = dest(&d);
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = channel();
        let flag = Arc::clone(&cancel);
        let dest2 = dest.clone();
        let started = std::time::Instant::now();
        let job = thread::spawn(move || fetch_with(&gio, 5, "http://x/leaf.bin", &dest2, &flag, &tx));
        thread::sleep(Duration::from_millis(200));
        cancel.store(true, Ordering::Relaxed);
        let line = job.join().expect("fetch thread");
        assert_eq!(line, r#"{"t":"fetchdone","id":5,"ok":false,"err":"cancelled"}"#);
        assert!(started.elapsed() < Duration::from_secs(10), "a cancel does not wait the tool out");
        assert!(!dest.exists() && !dest.parent().unwrap().exists());
    }

    #[test]
    fn a_missing_tool_is_a_failure_line_rather_than_a_panic() {
        let d = TestDir::new("fetchnotool");
        let dest = dest(&d);
        let (tx, _rx) = channel();
        let line = fetch_with(&d.join("absent").to_string_lossy(), 6, "http://x/a", &dest, &Arc::new(AtomicBool::new(false)), &tx);
        assert!(line.contains(r#""ok":false,"err":"cannot run "#), "{}", line);
        assert!(!dest.parent().unwrap().exists());
    }

    #[test]
    fn a_refused_scheme_answers_started_and_done_and_spawns_nothing() {
        let (tx, _rx) = channel();
        let mut ops = Ops::new(tx);
        let mut buf: Vec<u8> = Vec::new();
        start_fetch(&mut buf, &mut ops, "smb://host/share/a.txt");
        let text = String::from_utf8_lossy(&buf);
        assert_eq!(
            text,
            format!("{}\n{}\n", fetchstarted_line(1, "smb://host/share/a.txt"), fetchdone_line(1, false, "", REFUSED))
        );
        assert!(ops.fetches.is_empty(), "nothing to cancel was started");
        // A cancel for an id no fetch owns is a no-op, not a panic.
        cancel_fetch(&ops, 1);
    }
}
