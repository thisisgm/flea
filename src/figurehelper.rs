// `flea --figure-helper`: maths and diagrams through quickjs-ng, in a jail (AGENTS.md "Markdown figures").
use crate::backend::sandbox;
use crate::figurebuild;
use crate::figurecache;
use crate::paths;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

// Arch's extra/quickjs-ng, the only engine the vendored bundles run under.
const SYSTEM_QJS: &str = "/usr/bin/qjs";
// A test hook, and only a test hook: an absolute path to a qjs binary.
const QJS_ENV: &str = "FLEA_QJS";
// The helper lazy-loads vendor bundles and imports the shared post-processing module from the UI js directory.
const HELPER_NAME: &str = "figure-helper.mjs";
const WORKER_NAME: &str = "FigureWorker.mjs";
// Missing sandbox or engine refuses with this, never by running unsandboxed.
const REFUSED: i32 = 127;

// An absolute hook wins; an empty path is never absolute, so it takes the system binary.
fn qjs_from(value: Option<OsString>) -> PathBuf {
    match value {
        Some(raw) => {
            let candidate = PathBuf::from(raw);
            if candidate.is_absolute() {
                return candidate;
            }
            PathBuf::from(SYSTEM_QJS)
        }
        _ => PathBuf::from(SYSTEM_QJS),
    }
}

// The thin env-reading wrapper run() calls through resolve().
pub(crate) fn qjs_path() -> PathBuf {
    qjs_from(std::env::var_os(QJS_ENV))
}

// True when the jail's read-only /usr already covers the binary.
pub(crate) fn under_usr(path: &Path) -> bool {
    path.starts_with("/usr/")
}

// The full argv: prlimit caps, the jail flags, the read-only binds, then qjs, the helper and its flags.
pub(crate) fn figure_argv(qjs: &Path, vendor: &Path, bytecode: Option<&Path>, warm: &[&str]) -> Vec<String> {
    let mut binds: Vec<PathBuf> = vec![vendor.to_path_buf()];
    binds.extend(bytecode.map(Path::to_path_buf));
    // Sample layout: vendor is <ui>/vendor, so its parent holds js/FigureWorker.mjs.
    if let Some(ui) = vendor.parent() {
        binds.push(ui.join("js").join(WORKER_NAME));
    }
    if !under_usr(qjs) {
        binds.push(qjs.to_path_buf());
    }
    let mut inner = vec![
        qjs.to_string_lossy().into_owned(),
        vendor.join(HELPER_NAME).to_string_lossy().into_owned(),
    ];
    inner.extend(bytecode.map(|dir| format!("--bytecode={}", dir.display())));
    if !warm.is_empty() {
        inner.push(format!("--warm={}", warm.join(",")));
    }
    let refs: Vec<&Path> = binds.iter().map(PathBuf::as_path).collect();
    let mut argv = sandbox::wrap_readonly_extra(&inner, &refs);
    pin_utc(&mut argv);
    argv
}

// The helper formats with local getters, so its zone is pinned to UTC: after --clearenv, else after bwrap; a non-jail argv is left alone.
pub(crate) fn pin_utc(argv: &mut Vec<String>) {
    let at = argv.iter().position(|a| a == "--clearenv").or_else(|| argv.iter().position(|a| a == sandbox::BWRAP));
    if let Some(at) = at.map(|i| i + 1) {
        argv.splice(at..at, ["--setenv".to_string(), "TZ".to_string(), "UTC".to_string()]);
    }
}

// The helper's own flag that prints one line per bundle it loads, saying whether it came from bytecode or from source.
const REPORT_FLAG: &str = "--report";
// A test hook, and only a test hook: "1" makes every start a reporting one.
const REPORT_ENV: &str = "FLEA_FIGURE_REPORT";

// The same argv, asking the helper to say where each bundle came from.
pub(crate) fn reporting(mut argv: Vec<String>) -> Vec<String> {
    argv.push(REPORT_FLAG.to_string());
    argv
}

// Checked in order, so a missing sandbox refuses before anything is probed.
fn resolve_with(sandbox_ok: bool, qjs: &Path, ui: Option<&Path>, bytecode: Option<&Path>, warm: &[&str]) -> Result<Vec<String>, String> {
    if !sandbox_ok {
        return Err(String::from("the figure helper needs bwrap and prlimit, and one of them is missing"));
    }
    if !qjs.is_file() {
        return Err(format!("the figure helper needs quickjs-ng at {}, and it is missing", qjs.display()));
    }
    let Some(root) = ui else {
        return Err(String::from("the figure helper is missing from the UI tree"));
    };
    let vendor = root.join("vendor");
    let helper = vendor.join(HELPER_NAME);
    if !helper.is_file() {
        return Err(format!("the figure helper is missing at {}", helper.display()));
    }
    let worker = root.join("js").join(WORKER_NAME);
    if !worker.is_file() {
        return Err(format!("the figure worker module is missing at {}", worker.display()));
    }
    Ok(figure_argv(qjs, &vendor, bytecode, warm))
}

// The verified bytecode directory for this engine and tree, or None after starting its background build; any doubt is the source path.
fn bytecode_dir(qjs: &Path, ui: &Path) -> Option<PathBuf> {
    let root = figurecache::root().filter(|dir| dir.is_absolute())?;
    let key = figurecache::key(qjs, ui)?;
    if let Some(dir) = figurecache::verified(&root.join(&key), &key) {
        return Some(dir);
    }
    if figurebuild::wanted(&root, &key) && figurebuild::make_root(&root).is_some() {
        figurebuild::spawn_background();
    }
    None
}

// The figure kinds a warm start may name; anything else on the command line is dropped before it reaches the helper.
const KINDS: [&str; 2] = ["math", "mermaid"];

// Sample input: "--warm=mermaid,../x,math,mermaid" gives ["mermaid", "math"]; no argument gives none.
fn warm_kinds(arg: Option<&str>) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = Vec::new();
    for name in arg.and_then(|a| a.strip_prefix("--warm=")).unwrap_or("").split(',') {
        if let Some(kind) = KINDS.iter().find(|k| **k == name) {
            if !kinds.contains(kind) {
                kinds.push(kind);
            }
        }
    }
    kinds
}

// The thin env-reading wrapper run() calls; the refusals come first, so a missing engine never reads the cache.
fn resolve(warm: &[&str]) -> Result<Vec<String>, String> {
    let (qjs, ui) = (qjs_path(), paths::ui_dir());
    let source = resolve_with(sandbox::available(), &qjs, ui.as_deref(), None, warm)?;
    // The checks above already ran on this engine and tree, so a verified bytecode directory only rebuilds the argv.
    let argv = match ui.as_deref() {
        Some(root) => match bytecode_dir(&qjs, root) {
            Some(dir) => figure_argv(&qjs, &root.join("vendor"), Some(&dir), warm),
            None => source,
        },
        None => source,
    };
    Ok(if std::env::var_os(REPORT_ENV).is_some_and(|v| v == "1") { reporting(argv) } else { argv })
}

// `warm` is the optional `--warm=KINDS` argument: those bundles load before the first request.
// Sample input: ["flea", "--figure-helper", "--warm=math"], "--figure-compile" or "--figure-store"; anything else is not ours.
pub fn dispatch(args: &[String]) -> Option<i32> {
    let mode = args.get(1)?.as_str();
    match (mode, args.len()) {
        ("--figure-helper", 2) => Some(run(None)),
        ("--figure-helper", 3) if args[2].starts_with("--warm=") => Some(run(Some(args[2].as_str()))),
        ("--figure-compile", 2) => Some(figurebuild::run()),
        ("--figure-store", 2) => Some(crate::figurestore::run()),
        _ => None,
    }
}

fn run(warm: Option<&str>) -> i32 {
    match resolve(&warm_kinds(warm)) {
        Ok(argv) => {
            let mut cmd = crate::backend::locale::command(&argv[0]);
            cmd.args(&argv[1..]);
            // Exec replaces us, so the caller's stdin and stdout pipes reach qjs direct.
            let error = cmd.exec();
            eprintln!("flea: the figure helper could not start {}: {}", argv[0], error);
            REFUSED
        }
        Err(message) => {
            eprintln!("flea: {}", message);
            REFUSED
        }
    }
}

#[cfg(test)]
#[path = "figurehelper_tests.rs"]
mod tests;
