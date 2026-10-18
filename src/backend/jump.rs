// The path bar's folder jump, answered once per open of the bar; see docs/protocol.md "jump".
use crate::backend::opsreq::OpMsg;
use crate::json::escape;
use crate::backend::mountinfo::{enclosing, mounts_in};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// zoxide answers from one local file in milliseconds, so past this it is wedged and draws nothing.
const ZOXIDE_LIMIT: Duration = Duration::from_secs(2);
// A zoxide whose pipe closed is exiting; this is how long the answer waits to reap it, polled in steps of REAP_POLL.
const REAP_GRACE: Duration = Duration::from_millis(100);
const REAP_POLL: Duration = Duration::from_millis(5);
// About ten thousand paths, far past any ranking a person reads; a larger database is cut at its tail.
const ZOXIDE_BYTES: u64 = 1 << 20;
// The ranked head kept for the existence checks, already more than a dropdown ever draws.
const ZOXIDE_ROWS: usize = 1000;
// One budget for every existence check: a wedged stat costs its own source's later rows, never the answer.
const CHECK_LIMIT: Duration = Duration::from_secs(1);
// A zoxide ranking: each path with its score.
type Ranking = Vec<(String, f64)>;
// One zoxide at a time; checks in flight carry their open's deadline, and the next open skips a wedged key.
static ZOXIDE_RUNNING: AtomicBool = AtomicBool::new(false);
static CHECKING: Mutex<BTreeMap<String, Vec<(u64, Instant)>>> = Mutex::new(BTreeMap::new());
static TICKETS: AtomicU64 = AtomicU64::new(0);
// The last ranking a run answered before its limit, drawn while a later run is still in flight.
static LAST_ZOXIDE: Mutex<Ranking> = Mutex::new(Vec::new());
// The ranking the newest run that answered fresh computed, with the ask it answered, for the whole ask that follows it.
static KEPT_RANKING: Mutex<Option<(usize, Ranking)>> = Mutex::new(None);
// Where the mount table is read, once per answer, and never through the filesystems it lists.
const MOUNTINFO: &str = "/proc/self/mountinfo";

// Where a candidate came from, in the order the dropdown draws the sources.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    Favourite,
    Zoxide,
    Recent,
}

const SOURCES: [Source; 3] = [Source::Favourite, Source::Zoxide, Source::Recent];

#[derive(Clone)]
struct Candidate {
    source: Source,
    path: String,
}

// Answered on its own thread: zoxide is a subprocess and a stat can block, so the loop never waits.
pub fn request(id: usize, ranking: usize, favourites: Vec<String>, recent: Vec<String>, replies: Sender<OpMsg>) {
    // Meta's variant carries any finished line; it exists for the same reason, a subprocess the loop must not wait on.
    std::thread::spawn(move || {
        let _ = replies.send(OpMsg::Meta { line: answer("zoxide", id, ranking, &favourites, &recent) });
    });
}

// Production entry: the recent files go through recent_folder on the same budget as every other check.
fn answer(program: &str, id: usize, ranking: usize, favourites: &[String], recent: &[String]) -> String {
    answer_checked(program, id, ranking, favourites, recent, CHECK_LIMIT, recent_folder)
}

// recent_check is a parameter so tests can stand a wedged stat in for the real one.
fn answer_checked(
    program: &str, id: usize, ranking: usize, favourites: &[String], recent: &[String],
    limit: Duration, recent_check: fn(&Candidate) -> Option<String>,
) -> String {
    let started = Instant::now();
    let ranked = kept_ranking(ranking).unwrap_or_else(|| {
        let fresh = zoxide(program, ZOXIDE_LIMIT);
        if let Some(rows) = &fresh {
            *KEPT_RANKING.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((id, rows.clone()));
        }
        fresh.unwrap_or_else(last_ranking)
    });
    let paths: Vec<String> = ranked.iter().map(|(path, _)| path.clone()).collect();
    let mounts = mounts_in(&std::fs::read_to_string(MOUNTINFO).unwrap_or_default());
    let mut found = existing(candidates(favourites, &paths), limit, is_dir_path, &mounts);
    let (parents, files) = recent_parents(recent);
    let resolved = resolve_parents(parents, limit, &mounts);
    let recent_candidates: Vec<Candidate> = files.into_iter()
        .filter(|(_, parent)| resolved.contains(parent))
        .map(|(file, _)| Candidate { source: Source::Recent, path: file })
        .collect();
    found.extend(existing(recent_candidates, limit, recent_check, &mounts));
    jumped_line(id, &found, &ranked, started.elapsed().as_secs_f64() * 1000.0)
}

// The whole ask of an open names the provisional ask before it (0 names none), and that ask's ranking answers it.
fn kept_ranking(ranking: usize) -> Option<Vec<(String, f64)>> {
    let kept = KEPT_RANKING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kept.as_ref().filter(|(id, _)| ranking != 0 && *id == ranking).map(|(_, rows)| rows.clone())
}

// --all keeps zoxide from pruning its database on a query Flea made, --score is the frecency the client ranks by, and None means no run of its own answered (one in flight, or past its limit).
fn zoxide(program: &str, limit: Duration) -> Option<Vec<(String, f64)>> {
    if ZOXIDE_RUNNING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let spawned = crate::backend::locale::command(program)
        .args(["query", "--list", "--all", "--score"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(_) => {
            ZOXIDE_RUNNING.store(false, Ordering::SeqCst);
            return Some(Vec::new());
        }
    };
    let pipe = child.stdout.take();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(ZOXIDE_BYTES).read_to_end(&mut text);
        }
        let _ = tx.send(text);
    });
    let text = rx.recv_timeout(limit).map(|text| (true, text)).unwrap_or((false, Vec::new()));
    // Short of the cap means the pipe closed; at the cap, or past the limit with nothing, zoxide may still be running.
    let whole = !text.1.is_empty() && (text.1.len() as u64) < ZOXIDE_BYTES;
    if !whole {
        let _ = child.kill();
    }
    // A closed pipe means zoxide is exiting, so the slot is free before the answer leaves and the next ask runs its own.
    if whole && reaped_within(&mut child, REAP_GRACE) {
        ZOXIDE_RUNNING.store(false, Ordering::SeqCst);
    } else {
        // A zoxide blocked in the kernel outlives even SIGKILL until its read returns, so the reap has a thread of its own.
        std::thread::spawn(move || {
            let _ = child.wait();
            ZOXIDE_RUNNING.store(false, Ordering::SeqCst);
        });
    }
    // A run past its limit draws the ranking that answered in time, as an open behind a run in flight does.
    if !text.0 {
        return None;
    }
    let ranked = ranked_paths(&String::from_utf8_lossy(&text.1), whole);
    *LAST_ZOXIDE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = ranked.clone();
    Some(ranked)
}

// True once the child has exited and been reaped within the grace; a child still running after it, or one try_wait cannot read, is the reaper thread's.
fn reaped_within(child: &mut std::process::Child, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(REAP_POLL);
    }
}

// The ranking kept from the last run that answered before its limit, empty on a first-ever open.
fn last_ranking() -> Vec<(String, f64)> {
    LAST_ZOXIDE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

// Sample input: "  80.0 /home/gm/My Documents", a right-aligned score, one space, then the path, which may hold spaces.
// A cut read drops its last line; a row needs a finite score and an absolute path.
fn ranked_paths(text: &str, whole: bool) -> Vec<(String, f64)> {
    let mut lines: Vec<&str> = text.lines().collect();
    if !whole {
        lines.pop();
    }
    let mut out = Vec::new();
    for line in lines {
        let (score, path) = match line.trim_start().split_once(' ') {
            Some(pair) => pair,
            None => continue,
        };
        match score.parse::<f64>() {
            Ok(score) if score.is_finite() && path.starts_with('/') => out.push((path.to_string(), score)),
            _ => continue,
        }
        if out.len() == ZOXIDE_ROWS {
            break;
        }
    }
    out
}

// Favourites, then zoxide's ranking, each in its own order; a path named twice is kept in its first source only.
fn candidates(favourites: &[String], ranked: &[String]) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (source, paths) in [Source::Favourite, Source::Zoxide].into_iter().zip([favourites, ranked]) {
        for path in paths.iter().filter(|path| path.starts_with('/')) {
            if seen.insert(path.as_str()) {
                out.push(Candidate { source, path: path.clone() });
            }
        }
    }
    out
}

// Sample input: ["/a/f.txt", "/a/g.txt"] names "/a" once and pairs every file with its folder.
fn recent_parents(recent: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let mut seen = HashSet::new();
    let mut parents = Vec::new();
    let mut files = Vec::new();
    for file in recent.iter().filter(|path| path.starts_with('/')) {
        let parent = Path::new(file).parent().map(|parent| parent.to_string_lossy().into_owned()).unwrap_or_default();
        if !parent.is_empty() && seen.insert(parent.clone()) {
            parents.push(parent.clone());
        }
        files.push((file.clone(), parent));
    }
    (parents, files)
}

// What one recent file stands for: itself when it is a folder, else the parent the caller already resolved.
fn recent_folder(candidate: &Candidate) -> Option<String> {
    let path = Path::new(&candidate.path);
    if path.is_dir() {
        return Some(candidate.path.clone());
    }
    path.parent().filter(|parent| !parent.as_os_str().is_empty()).map(|parent| parent.to_string_lossy().into_owned())
}

// Every folder recent files sit in, checked once each; an unresolved parent drops every file under it.
fn resolve_parents(parents: Vec<String>, limit: Duration, mounts: &[(PathBuf, String)]) -> HashSet<String> {
    let own: Vec<Candidate> = parents.into_iter().map(|path| Candidate { source: Source::Recent, path }).collect();
    existing(own, limit, is_dir_path, mounts).into_iter().map(|(_, path)| path).collect()
}

// A folder stands for itself when it is one.
fn is_dir_path(candidate: &Candidate) -> Option<String> {
    Path::new(&candidate.path).is_dir().then(|| candidate.path.clone())
}

// A filesystem whose stat can wedge: the shared network list plus any FUSE mount.
fn remote(kind: &str) -> bool {
    super::netfs::can_wedge(kind)
}

// What a check is known by: the mount on a remote filesystem, else the path itself; lexical, never a stat.
fn key_for(path: &str, mounts: &[(PathBuf, String)]) -> String {
    match enclosing(Path::new(path), mounts) {
        Some((point, kind)) if remote(kind) => format!("mount {}", point.display()),
        _ => path.to_string(),
    }
}

fn checking() -> std::sync::MutexGuard<'static, BTreeMap<String, Vec<(u64, Instant)>>> {
    CHECKING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// One check, unless a check on the same key already ran past its open's deadline: that one is wedged.
fn checked(candidate: &Candidate, key: &str, deadline: Instant, check: fn(&Candidate) -> Option<String>) -> Option<String> {
    let ticket = TICKETS.fetch_add(1, Ordering::SeqCst);
    {
        let mut table = checking();
        let running = table.entry(key.to_string()).or_default();
        if running.iter().any(|(_, given_up)| Instant::now() >= *given_up) {
            return None;
        }
        running.push((ticket, deadline));
    }
    let answer = check(candidate);
    let mut table = checking();
    if let Some(running) = table.get_mut(key) {
        running.retain(|(own, _)| *own != ticket);
        if running.is_empty() {
            table.remove(key);
        }
    }
    answer
}

// Each source is checked on a thread of its own; whatever has not answered by the limit is dropped.
fn existing(candidates: Vec<Candidate>, limit: Duration, check: fn(&Candidate) -> Option<String>, mounts: &[(PathBuf, String)]) -> Vec<(Source, String)> {
    let total = candidates.len();
    let deadline = Instant::now() + limit;
    let (tx, rx) = channel();
    for source in SOURCES {
        let own: Vec<(usize, Candidate, String)> = candidates.iter().enumerate()
            .filter(|(_, candidate)| candidate.source == source)
            .map(|(index, candidate)| (index, candidate.clone(), key_for(&candidate.path, mounts)))
            .collect();
        let tx = tx.clone();
        std::thread::spawn(move || {
            for (index, candidate, key) in own {
                if tx.send((index, candidate.source, checked(&candidate, &key, deadline, check))).is_err() {
                    return;
                }
            }
        });
    }
    drop(tx);
    let mut found: Vec<Option<(Source, String)>> = vec![None; total];
    // Ends at the limit, or as soon as every source's thread has finished and dropped its sender.
    while let Ok((index, source, resolved)) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        found[index] = resolved.map(|path| (source, path));
    }
    found.into_iter().flatten().collect()
}

// A folder appears once, in the first source that names it; frecency rides along for any ranked folder.
fn jumped_line(id: usize, found: &[(Source, String)], scores: &[(String, f64)], ms: f64) -> String {
    let ranked: HashMap<&str, f64> = scores.iter().map(|(path, score)| (path.as_str(), *score)).collect();
    let mut seen = HashSet::new();
    let mut lists: [Vec<String>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut frecency = Vec::new();
    for (source, folder) in found {
        if seen.insert(folder.as_str()) {
            lists[*source as usize].push(format!("\"{}\"", escape(folder)));
            if let Some(score) = ranked.get(folder.as_str()) {
                frecency.push(format!("\"{}\":{}", escape(folder), score));
            }
        }
    }
    format!(
        r#"{{"t":"jumped","id":{},"favourites":[{}],"zoxide":[{}],"recent":[{}],"frecency":{{{}}},"ms":{:.3}}}"#,
        id, lists[0].join(","), lists[1].join(","), lists[2].join(","), frecency.join(","), ms
    )
}

#[cfg(test)]
#[path = "jump_tests.rs"]
mod tests;
