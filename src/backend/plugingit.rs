// Git for plugins added from a URL: a clone, and an Update that pulls only into a plain repository.
use flea::bytename::ExactText as _;
use super::pluginfs::finish;
use super::regfile;
use crate::oflags::O_NOFOLLOW;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

// Git's own network work, not a plugin's start, so it gets a longer deadline.
pub const GIT_DEADLINE: Duration = Duration::from_secs(120);

// Git with no system or user config, no hooks and no fsmonitor, so no repository setting runs code of its own.
pub(super) fn git(args: &[&str], deadline: Duration) -> Result<(), String> {
    let child = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.ext.allow=never",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    finish(child, deadline)
}

pub fn clone(url: &str, to: &Path, deadline: Duration) -> Result<(), String> {
    git(
        &[
            "clone",
            "--depth",
            "1",
            "--quiet",
            "--",
            url,
            &to.exact_text(),
        ],
        deadline,
    )
}

// The settings a clone writes; anything else in a plugin's .git/config could run code or move the work tree.
fn allowed(key: &str) -> bool {
    const PLAIN: [&str; 9] = [
        "core.repositoryformatversion",
        "core.filemode",
        "core.bare",
        "core.logallrefupdates",
        "core.ignorecase",
        "core.precomposeunicode",
        "core.symlinks",
        "remote.origin.url",
        "remote.origin.fetch",
    ];
    PLAIN.contains(&key)
        || (key.starts_with("branch.") && (key.ends_with(".remote") || key.ends_with(".merge")))
}

// A real .git folder inside the plugin, sharing no other repository's config, holding only what a clone writes.
fn plain_repo(folder: &Path) -> Result<PathBuf, String> {
    let dir = folder.join(".git");
    let real = fs::symlink_metadata(&dir)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if !real || fs::symlink_metadata(dir.join("commondir")).is_ok() {
        return Err(format!("{} is not a plain repository", folder.display()));
    }
    let config = dir.join("config");
    read_config(folder).ok_or_else(|| format!("{} cannot be read", config.display()))?;
    let mut child = Command::new("git")
        .args(["config", "--no-includes", "--null", "--list", "--file"])
        .arg(&config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("no output")?;
    // Read on its own thread so the deadline below holds even if the config was swapped for a fifo.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).map(|_| out)
    });
    let ran = finish(child, CONFIG_DEADLINE);
    let output = reader
        .join()
        .map_err(|_| "no output".to_string())?
        .map_err(|e| e.to_string())?;
    ran.map_err(|e| format!("{}: {e}", config.display()))?;
    // Sample output: "core.bare\nfalse\0remote.origin.url\nfile:///src\0".
    for pair in output.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let key = String::from_utf8_lossy(pair.split(|b| *b == b'\n').next().unwrap_or_default())
            .to_lowercase();
        if !allowed(&key) {
            return Err(format!("{} sets {key}", config.display()));
        }
    }
    Ok(dir)
}

pub fn pull(folder: &Path, deadline: Duration) -> Result<(), String> {
    let dir = plain_repo(folder)?;
    let (dir, tree) = (
        format!("--git-dir={}", dir.display()),
        format!("--work-tree={}", folder.display()),
    );
    git(
        &[
            &dir,
            &tree,
            "-C",
            &folder.exact_text(),
            "pull",
            "--ff-only",
            "--quiet",
        ],
        deadline,
    )
}

// A clone's config is a few hundred bytes; a link, a fifo or anything past this is not read.
const CONFIG_LIMIT: u64 = 64 << 10;
const CONFIG_DEADLINE: Duration = Duration::from_secs(10);

// The plugin's own .git/config, only from a real .git folder, as a regular file read within its bound.
fn read_config(folder: &Path) -> Option<String> {
    if !fs::symlink_metadata(folder.join(".git")).ok()?.is_dir() {
        return None;
    }
    let path = folder.join(".git/config");
    if !fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let file = regfile::open_if_regular(&path, O_NOFOLLOW).ok()?;
    super::pluginfs::read_bounded(file, CONFIG_LIMIT)
}

// Sample input: [remote "origin"]\n\turl = https://github.com/mtorvik/flea-checksums
pub fn git_origin(folder: &Path) -> Option<String> {
    let text = read_config(folder)?;
    let mut in_origin = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "url" {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    None
}
