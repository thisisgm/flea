use crate::thp;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// The exit status ui/Opener.qml reads. 0 is a successful handoff and needs no name.
pub const FAILED: i32 = 2;

// Canonical, so a relative path and a symlink both name the one real directory the terminal sits in.
fn resolved(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

// xdg-terminal-exec is the OEM route: `omarchy default terminal` configures what it reads,
// and --dir= names the working directory without taking a command.
pub fn open_terminal(path: &str) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        // The reason is elided, never shown raw, and the path is the user's own input.
        None => {
            eprintln!("flea: that directory could not be opened in a terminal, check that it still exists");
            return FAILED;
        }
    };
    if !target.is_dir() {
        eprintln!("flea: that directory could not be opened in a terminal, check that it still exists");
        return FAILED;
    }
    // The setting is inherited across exec, so this is the last point that can hand it back.
    thp::enable();
    // An OsString and not a format!, because Path::display would substitute U+FFFD for a byte that is not UTF-8.
    let mut dir = OsString::from("--dir=");
    dir.push(&target);
    // corner: spawn and not exec, because the terminal outlives us; see AGENTS.md "Opening a file".
    let started = Command::new("xdg-terminal-exec")
        .arg(&dir)
        // The terminal outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the terminal.
        .process_group(0)
        .spawn();
    match started {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open a terminal there");
            FAILED
        }
    }
}

// The agent name omarchy-default-agent returns, mapped to the actual command.
fn agent_command(name: &str) -> Option<&'static str> {
    match name {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        "gemini" => Some("gemini"),
        "grok" => Some("grok"),
        "copilot" => Some("copilot"),
        "omp" => Some("omp"),
        "pi" => Some("pi"),
        "crush" => Some("crush"),
        _ => None,
    }
}

// Open the default code agent in the given directory. Reads omarchy-default-agent,
// maps the name to a command, and launches it through xdg-terminal-exec -e.
// If agent_name is provided, uses that instead of omarchy-default-agent.
pub fn open_agent(path: &str, agent_name: Option<&str>) -> i32 {
    let target = match resolved(path) {
        Some(p) => p,
        None => {
            eprintln!("flea: that directory could not be opened, check that it still exists");
            return FAILED;
        }
    };
    if !target.is_dir() {
        eprintln!("flea: that path is not a directory");
        return FAILED;
    }
    // Use provided agent name or detect the default.
    let agent_name = match agent_name {
        Some(n) => n.to_string(),
        None => {
            let output = std::process::Command::new("omarchy-default-agent")
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                });
            match output {
                Some(n) => n,
                None => {
                    eprintln!("flea: no default agent set (omarchy-default-agent)");
                    return FAILED;
                }
            }
        }
    };
    let cmd = match agent_command(&agent_name) {
        Some(c) => c,
        None => {
            eprintln!("flea: unknown agent '{}', cannot launch", agent_name);
            return FAILED;
        }
    };
    thp::enable();
    let mut dir = OsString::from("--dir=");
    dir.push(&target);
    let started = Command::new("xdg-terminal-exec")
        .arg(&dir)
        .arg("-e")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    match started {
        Ok(_) => 0,
        Err(_) => {
            eprintln!("flea: nothing on this system could be asked to open an agent there");
            FAILED
        }
    }
}
