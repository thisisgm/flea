//! `flea --action <id> [--window <pid>]` and `flea --actions [--window <pid>]`: one request on the window's IPC socket.
use super::{
    client, endpoint,
    wire::{Reply, Request},
};
use crate::jsondoc::{self, Json};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub use crate::keys::actions::{NO_WINDOW, RAN, UNAVAILABLE, UNKNOWN, USAGE};

const COMPOSITOR_WAIT: Duration = Duration::from_millis(500);
const COMPOSITOR_POLL: Duration = Duration::from_millis(5);
const CLIENTS_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub struct Options {
    pub window: Option<u32>,
    pub request: Request,
}

fn usage() -> Reply {
    Reply::error(
        USAGE,
        "usage: flea --action <id> [--window <pid>] | flea --actions [--window <pid>]",
    )
}

/// `arguments` starts at the verb, `--action` or `--actions`.
// Sample input: ["--action", "example.checksums:sha256", "--window", "4242"]
pub fn parse(arguments: &[String]) -> Result<Options, Reply> {
    let (id, rest) = match arguments.first().map(String::as_str) {
        Some("--action") => match arguments.get(1) {
            Some(id) if !id.starts_with("--") => (Some(id.clone()), &arguments[2..]),
            _ => return Err(usage()),
        },
        Some("--actions") => (None, &arguments[1..]),
        _ => return Err(usage()),
    };
    let window = match rest {
        [] => None,
        [flag, pid] if flag == "--window" => Some(
            pid.parse::<u32>()
                .ok()
                .filter(|pid| *pid > 0 && *pid <= i32::MAX as u32)
                .ok_or_else(|| Reply::error(USAGE, "flea: --window needs a window's process id"))?,
        ),
        _ => return Err(usage()),
    };
    let (function, arguments) = match id {
        Some(id) => ("runAction", vec![id]),
        None => ("actionIds", Vec::new()),
    };
    Ok(Options {
        window,
        request: Request::Call {
            target: "flea".into(),
            function: function.into(),
            arguments,
        },
    })
}

/// The Hyprland client pids from most to least recently focused, read off `hyprctl -j clients`.
// Sample input: [{"address":"0x55d1","pid":4242,"class":"com.thisisgm.flea","focusHistoryID":0}, ...]
pub fn focus_order(clients: &str) -> Vec<u32> {
    let Ok(Json::Arr(clients)) = jsondoc::parse(clients) else {
        return Vec::new();
    };
    let number = |client: &Json, key: &str| match client.get(key) {
        Some(Json::Num(n)) => n.parse::<i64>().ok(),
        _ => None,
    };
    let mut order: Vec<(i64, u32)> = clients
        .iter()
        .filter_map(|client| {
            let history = number(client, "focusHistoryID").filter(|h| *h >= 0)?;
            let pid = u32::try_from(number(client, "pid")?)
                .ok()
                .filter(|pid| *pid > 0)?;
            Some((history, pid))
        })
        .collect();
    order.sort_unstable();
    order.into_iter().map(|(_, pid)| pid).collect()
}

// Bounded so a stalled compositor costs half a second and falls back, never a hung script.
fn compositor_clients() -> Option<String> {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|s| !s.is_empty())?;
    let mut child = Command::new("hyprctl")
        .args(["-j", "clients"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut pipe = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut pipe)
            .take(CLIENTS_LIMIT as u64 + 1)
            .read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + COMPOSITOR_WAIT;
    while child.try_wait().ok()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(COMPOSITOR_POLL);
    }
    let bytes = reader.join().ok()?;
    (bytes.len() <= CLIENTS_LIMIT)
        .then(|| String::from_utf8(bytes).ok())
        .flatten()
}

/// The named window, else the most recently focused Flea window, else the newest one when the compositor cannot say.
pub fn choose(
    windows: &[(u32, PathBuf)],
    named: Option<u32>,
    focus: &[u32],
) -> Result<PathBuf, Reply> {
    if let Some(pid) = named {
        return windows
            .iter()
            .find(|(window, _)| *window == pid)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| {
                Reply::error(
                    NO_WINDOW,
                    format!("flea: no running Flea window has process id {pid}"),
                )
            });
    }
    focus
        .iter()
        .find_map(|pid| windows.iter().find(|(window, _)| window == pid))
        .or_else(|| windows.last())
        .map(|(_, path)| path.clone())
        .ok_or_else(|| {
            Reply::error(
                NO_WINDOW,
                "flea: no Flea window is running, so there is nothing to run the action in",
            )
        })
}

fn first_line(text: &str) -> &str {
    text.trim().lines().next().unwrap_or("")
}

// A run prints nothing and a list prints one id per line; IPC answers such as "Target not found." are code 0 too.
fn checked(listing: bool, reply: Reply) -> Reply {
    let answer = reply.stdout.trim();
    let ran = if listing {
        !answer.is_empty()
            && answer
                .lines()
                .all(|id| !id.is_empty() && !id.contains(char::is_whitespace))
    } else {
        answer.is_empty()
    };
    if reply.code == RAN && ran {
        return reply;
    }
    if matches!(reply.code, UNKNOWN | UNAVAILABLE) && !reply.stderr.trim().is_empty() {
        return Reply::error(reply.code, first_line(&reply.stderr));
    }
    if answer == "Function not found." {
        return Reply::error(
            UNAVAILABLE,
            "flea: that window is an older Flea that cannot run actions",
        );
    }
    let said = if reply.stderr.trim().is_empty() {
        first_line(&reply.stdout)
    } else {
        first_line(&reply.stderr)
    };
    let said = if said.is_empty() {
        "it answered nothing"
    } else {
        said
    };
    let what = if listing {
        "list its actions"
    } else {
        "run the action"
    };
    Reply::error(
        UNAVAILABLE,
        format!("flea: the window could not {what}: {said}"),
    )
}

pub fn execute(root: &Path, options: &Options, focus: impl FnOnce() -> Vec<u32>) -> Reply {
    let windows = match endpoint::instances(root) {
        Ok(windows) => windows,
        Err(error) => {
            return Reply::error(
                NO_WINDOW,
                format!("flea: the window sockets could not be read: {error}"),
            )
        }
    };
    let order = if options.window.is_none() && windows.len() > 1 {
        focus()
    } else {
        Vec::new()
    };
    let path = match choose(&windows, options.window, &order) {
        Ok(path) => path,
        Err(reply) => return reply,
    };
    let listing =
        matches!(&options.request, Request::Call { function, .. } if function == "actionIds");
    match client::exchange(&path, &options.request) {
        Ok(reply) => checked(listing, reply),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            Reply::error(
                NO_WINDOW,
                "flea: no Flea window is running, so there is nothing to run the action in",
            )
        }
        Err(error) => Reply::error(
            UNAVAILABLE,
            format!("flea: the window did not answer: {error}"),
        ),
    }
}

pub fn run(arguments: &[String]) -> i32 {
    let reply = match parse(arguments) {
        Err(reply) => reply,
        Ok(options) => match endpoint::runtime() {
            Ok(root) => execute(&root, &options, || {
                compositor_clients().map_or_else(Vec::new, |c| focus_order(&c))
            }),
            Err(error) => Reply::error(
                NO_WINDOW,
                format!("flea: the window sockets could not be read: {error}"),
            ),
        },
    };
    print!("{}", reply.stdout);
    eprint!("{}", reply.stderr);
    reply.code
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
