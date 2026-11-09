// The compare request: two regular files answered with the unified diff of their lines, or the reason there is none.
use crate::backend::opsreq::OpMsg;
use crate::backend::regfile::open_regular;
use crate::backend::{child, jail, sandbox};
use crate::json::{escape, field_str, field_usize};
use std::io::Read;
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

// A file past this many bytes is too large to compare.
const OVER_BYTES: u64 = 1_048_576;
// A NUL among the first bytes of either file makes the pair binary.
const NUL_WINDOW: u64 = 8192;
// Two files at the limit differ by a patch of about twice that; more than this is a diff that went wrong.
const PATCH_CAP: u64 = 8 * 1_048_576;
// The archive reader's deadline also bounds one small text diff.
const DIFF_WAIT: Duration = Duration::from_millis(crate::backend::archivelist::ARCHIVE_READ_MS);
const DIFF: &str = "/usr/bin/diff";

pub struct Ask {
    token: usize,
    a: String,
    b: String,
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Identical,
    Patch(String),
    Binary,
    Over,
    Failed,
}

impl Ask {
    // Sample input: {"c":"compare","token":3,"a":"/home/gm/notes-a.md","b":"/home/gm/notes-b.md"}
    pub fn parse(line: &str) -> Ask {
        Ask {
            token: field_usize(line, "token").unwrap_or(0),
            a: field_str(line, "a").unwrap_or_default(),
            b: field_str(line, "b").unwrap_or_default(),
        }
    }
}

// Answered on a thread, because a read on a hung mount never returns and the loop waits on nothing.
pub fn spawn(ask: Ask, tx: Sender<OpMsg>) {
    std::thread::spawn(move || {
        let _ = tx.send(OpMsg::Meta { line: reply(ask.token, &compare(&ask.a, &ask.b)) });
    });
}

fn compare(a: &str, b: &str) -> Outcome {
    match screen(a, b) {
        Ok(()) => diff(a, b),
        Err(outcome) => outcome,
    }
}

// The answers that need no process: over is judged on both files before binary is, and diff runs only for text of a size it can answer.
fn screen(a: &str, b: &str) -> Result<(), Outcome> {
    if !flea::bytename::view(a).is_absolute() || !flea::bytename::view(b).is_absolute() {
        return Err(Outcome::Failed);
    }
    let (Some(first), Some(second)) = (
        open_regular(&flea::bytename::view(a)),
        open_regular(&flea::bytename::view(b)),
    ) else {
        return Err(Outcome::Failed);
    };
    let (Ok(first_len), Ok(second_len)) = (
        first.metadata().map(|m| m.len()),
        second.metadata().map(|m| m.len()),
    ) else {
        return Err(Outcome::Failed);
    };
    if first_len > OVER_BYTES || second_len > OVER_BYTES {
        return Err(Outcome::Over);
    }
    for file in [first, second] {
        let mut head = Vec::new();
        if file.take(OVER_BYTES + 1).read_to_end(&mut head).is_err() {
            return Err(Outcome::Failed);
        }
        if head.len() as u64 > OVER_BYTES { return Err(Outcome::Over); }
        if head[..head.len().min(NUL_WINDOW as usize)].contains(&0) {
            return Err(Outcome::Binary);
        }
        if std::str::from_utf8(&head).is_err() { return Err(Outcome::Failed); }
    }
    Ok(())
}

// The labels are the real paths, so the patch names the files it was made from.
fn diff_argv(a: &str, b: &str) -> Vec<String> {
    [DIFF, "-u", "--label", a, "--label", b, "--", a, b].iter().map(|arg| arg.to_string()).collect()
}

// One boxed process with the two files bound read-only and nothing writable; exit 0 is identical, 1 is a patch.
fn diff(a: &str, b: &str) -> Outcome {
    if !sandbox::available() {
        return Outcome::Failed;
    }
    let inner = diff_argv(a, b);
    let mut full = sandbox::wrap_readonly_extra(
        &inner,
        &[&*flea::bytename::view(a), &*flea::bytename::view(b)],
    );
    sandbox::add_status(&mut full, inner.len());
    let spawned = jail::spawn_jailed(&full, |cmd| {
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        cmd.env(
            "LANG",
            std::env::var_os("LANG").unwrap_or_else(|| "C.UTF-8".into()),
        );
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
    });
    let Ok(jailed) = spawned else {
        return Outcome::Failed;
    };
    read_diff(jailed)
}

// The pipe drains beside the existing exit waiter, so a reopen or read that hangs cannot outlive the deadline.
fn read_diff(mut jailed: jail::Jailed) -> Outcome {
    let Some(pipe) = jailed.child.stdout.take() else {
        jail::kill_tree(&mut jailed);
        return Outcome::Failed;
    };
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        pipe.take(PATCH_CAP + 1).read_to_end(&mut out).map(|_| out)
    });
    if !matches!(child::wait_for_exit(&jailed.child, Instant::now() + DIFF_WAIT), child::Woke::Exited) {
        jail::kill_tree(&mut jailed);
        return Outcome::Failed;
    }
    let code = jailed.child.wait().ok().and_then(|status| status.code());
    match reader.join() {
        Ok(Ok(out)) if out.len() as u64 <= PATCH_CAP => by_exit(code, out),
        _ => Outcome::Failed,
    }
}

// Exit 0 is identical and exit 1 a patch, which always names the two files; bwrap also exits 1 when it cannot start diff, and that says nothing.
fn by_exit(code: Option<i32>, out: Vec<u8>) -> Outcome {
    match code {
        Some(0) => Outcome::Identical,
        Some(1) if !out.is_empty() => String::from_utf8(out).map_or(Outcome::Failed, Outcome::Patch),
        _ => Outcome::Failed,
    }
}

// Sample output: {"t":"compare","token":3,"state":"diff","patch":"--- /a\n+++ /b\n@@ -1 +1 @@\n-x\n+y\n"}
fn reply(token: usize, outcome: &Outcome) -> String {
    let (state, patch) = match outcome {
        Outcome::Identical => ("identical", ""),
        Outcome::Patch(patch) => ("diff", patch.as_str()),
        Outcome::Binary => ("binary", ""),
        Outcome::Over => ("over", ""),
        Outcome::Failed => ("failed", ""),
    };
    format!(r#"{{"t":"compare","token":{token},"state":"{state}","patch":"{}"}}"#, escape(patch))
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
