// `flea --figure-compile`: builds the figure bytecode cache in a jail that can write only its own scratch directory (AGENTS.md "Markdown figures").
use crate::backend::sandbox;
use crate::figurecache;
use crate::figurehelper;
use crate::figurestore::{DIR_MODE, FILE_MODE, SCRATCH_STALE};
use crate::json;
use crate::oflags::O_NOFOLLOW;
use crate::paths;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

// A build is a second of CPU, so this wall bound only ends a hung one.
const BUILD_DEADLINE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(10);
// One build is the compile and the two smoke runs, each its own bounded jail run.
const BUILD_RUNS: u32 = 3;
const LOCK_MARGIN: Duration = Duration::from_secs(60);
// A lock older than the longest build was left by a dead builder; a failure is retried after this long.
const LOCK_STALE: Duration = Duration::from_secs(BUILD_DEADLINE.as_secs() * BUILD_RUNS as u64 + LOCK_MARGIN.as_secs());
const FAILED_RETRY: Duration = Duration::from_secs(3600);
// The compile script, and the two requests that prove the bytecode renders what the source renders before it is trusted.
const COMPILE_NAME: &str = "figure-compile.mjs";
const SMOKE_BUNDLES: [&str; 2] = ["math", "mermaid"];
const SMOKE_INPUT: &str = "{\"id\":1,\"kind\":\"math\",\"source\":\"x^2\",\"display\":true,\"theme\":{\"bg\":\"#101315\",\"fg\":\"#c0caf5\"}}\n{\"id\":2,\"kind\":\"mermaid\",\"source\":\"flowchart TD\\n    A --> B\",\"display\":true,\"theme\":{\"bg\":\"#101315\",\"fg\":\"#c0caf5\"}}\n";

// Runs one argv with the text on stdin and answers its stdout; the seam a test replaces so no jail is needed.
pub type Runner<'a> = &'a dyn Fn(&[String], &str) -> Result<String, String>;

fn age(path: &Path) -> Option<Duration> {
    let modified = fs::symlink_metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn lock_path(root: &Path, key: &str) -> PathBuf {
    root.join(format!("{key}.lock"))
}

fn failed_path(root: &Path, key: &str) -> PathBuf {
    root.join(format!("{key}.failed"))
}

// True when starting a build would not duplicate a live one or repeat a recent failure.
pub fn wanted(root: &Path, key: &str) -> bool {
    let fresh = |path: PathBuf, limit: Duration| age(&path).is_some_and(|a| a < limit);
    !fresh(lock_path(root, key), LOCK_STALE) && !fresh(failed_path(root, key), FAILED_RETRY)
}

// The cache root, made private to the user; None when it cannot be made, and then no build is started.
pub fn make_root(root: &Path) -> Option<()> {
    fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(root).ok()?;
    root.is_dir().then_some(())
}

// A detached `flea --figure-compile` through a shell that exits at once, so the launcher keeps no zombie child and the build outlives it.
pub fn spawn_background() {
    let Ok(exe) = std::env::current_exe() else { return };
    let started = Command::new("/bin/sh")
        .args(["-c", "\"$0\" --figure-compile < /dev/null > /dev/null 2>&1 &"])
        .arg(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    drop(started);
}

fn create_new(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new().write(true).create_new(true).mode(FILE_MODE).open(path)
}

// Whatever sits at an expected-ours path is removed only when it is a real directory or file there, never through a link.
fn remove_ours(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => drop(fs::remove_dir_all(path)),
        Ok(_) => drop(fs::remove_file(path)),
        Err(_) => {}
    }
}

#[derive(PartialEq)]
enum Kind {
    Live,
    Scratch,
    Lock,
    Failed,
}

// Sample input: "00112233445566778899aabbccddeeff.tmp-4242" is scratch of key 00112233445566778899aabbccddeeff; anything else under the root is not ours.
fn classify(name: &str) -> Option<(&str, Kind)> {
    let (stem, rest) = match name.split_once('.') {
        Some(parts) => parts,
        None => return figurecache::is_key(name).then_some((name, Kind::Live)),
    };
    if !figurecache::is_key(stem) {
        return None;
    }
    let digits = |tail: &str| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit());
    match rest {
        "lock" => Some((stem, Kind::Lock)),
        "failed" => Some((stem, Kind::Failed)),
        _ => {
            let tail = rest.strip_prefix("tmp-").or_else(|| rest.strip_prefix("old-"))?;
            digits(tail).then_some((stem, Kind::Scratch))
        }
    }
}

// Everything under the root that is ours and no longer current: other keys' directories, stale scratch and locks, failure marks.
fn sweep(root: &Path, current: &str) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((stem, kind)) = classify(&name) else { continue };
        let older = |limit: Duration| age(&entry.path()).is_some_and(|a| a > limit);
        let doomed = match kind {
            Kind::Live => stem != current && figurecache::replaceable(&entry.path(), stem),
            Kind::Failed => true,
            Kind::Lock => stem != current && older(LOCK_STALE),
            Kind::Scratch => older(SCRATCH_STALE),
        };
        if doomed {
            remove_ours(&entry.path());
        }
    }
}

// The jail argv for compiling vendor sources into `scratch`: read-only vendor tree and engine, one writable directory.
pub fn compile_argv(qjs: &Path, vendor: &Path, scratch: &Path) -> Vec<String> {
    let mut binds: Vec<&Path> = vec![vendor];
    if !figurehelper::under_usr(qjs) {
        binds.push(qjs);
    }
    let inner = vec![
        qjs.to_string_lossy().into_owned(),
        vendor.join(COMPILE_NAME).to_string_lossy().into_owned(),
        vendor.to_string_lossy().into_owned(),
        scratch.to_string_lossy().into_owned(),
    ];
    sandbox::wrap_compile(&inner, &binds, scratch)
}

// The manifest of what the jail wrote, or None when a blob is missing, empty or not a plain file.
fn write_manifest(scratch: &Path, key: &str) -> Option<()> {
    let mut blobs = Vec::new();
    for name in figurecache::BLOBS {
        let (size, sum) = figurecache::file_digest(&scratch.join(name))?;
        if size == 0 {
            return None;
        }
        blobs.push((name.to_string(), size, sum));
    }
    let mut file = create_new(&scratch.join(figurecache::MANIFEST)).ok()?;
    file.write_all(figurecache::manifest_text(key, &blobs).as_bytes()).ok()
}

// Sample input: "{\"id\":0,\"bundle\":\"math\",\"from\":\"bytecode\"}\n{\"id\":1,\"svg\":\"<svg/>\"}\n" is one load report and one answer.
fn split_reports(text: &str) -> (Vec<(String, String)>, Vec<&str>) {
    let mut reports = Vec::new();
    let mut answers = Vec::new();
    for line in text.lines() {
        match (json::field_usize(line, "id"), json::field_str(line, "bundle"), json::field_str(line, "from")) {
            (Some(0), Some(bundle), Some(from)) => reports.push((bundle, from)),
            _ => answers.push(line),
        }
    }
    (reports, answers)
}

// The same two requests through both paths; the bytecode run must say it loaded bytecode, because a load that fails falls back to source and answers the same.
fn smoke(qjs: &Path, vendor: &Path, scratch: &Path, run: Runner) -> Result<(), String> {
    let (with, without) = (
        run(&figurehelper::reporting(figurehelper::figure_argv(qjs, vendor, Some(scratch), &[])), SMOKE_INPUT)?,
        run(&figurehelper::reporting(figurehelper::figure_argv(qjs, vendor, None, &[])), SMOKE_INPUT)?,
    );
    let (with_reports, with_answers) = split_reports(&with);
    let (without_reports, without_answers) = split_reports(&without);
    let reported = |from: &str| SMOKE_BUNDLES.iter().map(|bundle| (bundle.to_string(), from.to_string())).collect::<Vec<_>>();
    if with_reports != reported("bytecode") {
        return Err(String::from("the helper did not load every bundle from the bytecode"));
    }
    if without_reports != reported("source") {
        return Err(String::from("the source run did not load every bundle from source"));
    }
    if with_answers != without_answers || with_answers.len() != SMOKE_BUNDLES.len() || !with.contains("\"svg\"") {
        return Err(String::from("the bytecode answered differently from the source"));
    }
    Ok(())
}

// One build: compile in the jail, check the bytecode renders what the source renders, then swap the directory in whole.
fn compile_into(root: &Path, key: &str, qjs: &Path, ui: &Path, run: Runner) -> Result<(), String> {
    if !figurecache::replaceable(&root.join(key), key) {
        return Err("foreign or newer-schema figure manifest; cache was left unchanged".to_string());
    }
    let vendor = ui.join("vendor");
    let scratch = root.join(format!("{key}.tmp-{}", std::process::id()));
    remove_ours(&scratch);
    fs::DirBuilder::new().mode(DIR_MODE).create(&scratch).map_err(|e| format!("scratch: {e}"))?;
    let built = (|| {
        run(&compile_argv(qjs, &vendor, &scratch), "")?;
        write_manifest(&scratch, key).ok_or("the compile left no complete bytecode")?;
        smoke(qjs, &vendor, &scratch, run)?;
        Ok(())
    })();
    if let Err(message) = built {
        remove_ours(&scratch);
        return Err(message);
    }
    let live = root.join(key);
    let old = root.join(format!("{key}.old-{}", std::process::id()));
    // A directory in the way is a corrupt or foreign one: it moves aside first, so the swap is one rename.
    if fs::symlink_metadata(&live).is_ok() {
        remove_ours(&old);
        fs::rename(&live, &old).map_err(|e| format!("moving the old cache aside: {e}"))?;
    }
    let swapped = fs::rename(&scratch, &live);
    remove_ours(&old);
    swapped.map_err(|e| {
        remove_ours(&scratch);
        format!("installing the cache: {e}")
    })
}

// The whole background job; Ok even when a rival holds the lock, because the cache is then somebody's to finish.
pub fn build(root: &Path, qjs: &Path, ui: &Path, run: Runner) -> Result<(), String> {
    let key = figurecache::key(qjs, ui).ok_or("the engine or a keyed source is unreadable")?;
    if figurecache::verified(&root.join(&key), &key).is_some() || !wanted(root, &key) {
        return Ok(());
    }
    // The launcher made the root; a root that is gone (a test's sandbox removed under a late build) is not recreated here.
    if !root.is_dir() {
        return Err(String::from("the cache directory is gone"));
    }
    let lock = lock_path(root, &key);
    remove_stale_lock(&lock);
    let Some(token) = take_lock(&lock) else { return Ok(()) };
    let result = compile_into(root, &key, qjs, ui, run);
    match &result {
        Ok(()) => sweep(root, &key),
        Err(_) => {
            let failed = failed_path(root, &key);
            remove_ours(&failed);
            drop(create_new(&failed));
        }
    }
    release_lock(&lock, &token);
    result
}

// Creates the lock exclusively and writes this builder's token into it, so the final removal can tell its own lock from a successor's.
fn take_lock(lock: &Path) -> Option<String> {
    let token = format!("{} {}\n", std::process::id(), SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    let mut file = create_new(lock).ok()?;
    if file.write_all(token.as_bytes()).is_err() {
        remove_ours(lock);
        return None;
    }
    Some(token)
}

// Removes the lock only while it holds this builder's token, so a builder that took over a stale lock keeps its own.
fn release_lock(lock: &Path, token: &str) {
    let mut text = String::new();
    let read = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW).open(lock).and_then(|mut file| file.read_to_string(&mut text));
    if read.is_ok() && text == token {
        remove_ours(lock);
    }
}

fn remove_stale_lock(lock: &Path) {
    if age(lock).is_some_and(|a| a >= LOCK_STALE) {
        remove_ours(lock);
    }
}

// Runs the argv with a wall bound and answers its stdout; the real Runner.
pub fn run_jailed(argv: &[String], input: &str) -> Result<String, String> {
    let mut child = crate::backend::locale::command(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", argv[0]))?;
    if let Some(mut stdin) = child.stdin.take() {
        drop(stdin.write_all(input.as_bytes()));
    }
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        drop(stdout.read_to_string(&mut text));
        text
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if started.elapsed() > BUILD_DEADLINE => {
                drop(child.kill());
                drop(child.wait());
                return Err(String::from("the build did not finish in time"));
            }
            None => std::thread::sleep(POLL),
        }
    };
    let text = reader.join().map_err(|_| String::from("reader panicked"))?;
    if status.success() { Ok(text) } else { Err(format!("{} exited {status}", argv[0])) }
}

pub fn run() -> i32 {
    let (Some(root), Some(ui)) = (figurecache::root(), paths::ui_dir()) else { return 0 };
    match build(&root, &figurehelper::qjs_path(), &ui, &run_jailed) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("flea: the figure bytecode cache was not built: {message}");
            1
        }
    }
}

#[cfg(test)]
#[path = "figurebuild_tests.rs"]
mod tests;
