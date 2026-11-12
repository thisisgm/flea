// A cancelled `omdrop send` and everything it started: Flea runs it as the leader of its own session, so the session id
// names the whole send, including a sender omdrop moved into a process group of its own.
use std::process::Child;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(50);
// Linux signal numbers, which tests check against libc.
const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn getsid(pid: i32) -> i32;
}
// A sweep that still finds live members after this long gives up rather than hold the job forever.
const SWEEP_LIMIT: Duration = Duration::from_secs(2);

// SIGTERM to omdrop so it can stop its sender itself, then SIGKILL to every process left in its session, whether or not
// omdrop exited first. omdrop stays unreaped until the sweep is done, so its pid, and so the session id, can't be reused.
pub(crate) fn cancel(child: &mut Child, grace: Duration, mut during: impl FnMut()) {
    let Ok(leader) = i32::try_from(child.id()) else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    // SAFETY: kill(2) on our own child, which is not yet reaped, so the pid is still its own.
    unsafe { kill(leader, SIGTERM) };
    let deadline = Instant::now() + grace;
    while !exited(leader) && Instant::now() < deadline {
        during();
        std::thread::sleep(POLL);
    }
    // Only a session the child leads is swept, never one it merely belongs to, such as Flea's own.
    // SAFETY: getsid(2) reads the session of our own unreaped child.
    if unsafe { getsid(leader) } == leader {
        sweep(leader);
    } else {
        let _ = child.kill();
    }
    let _ = child.wait();
}

// True once the child has exited: unreaped, it is a zombie, and its pid stays its own.
fn exited(pid: i32) -> bool {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    matches!(
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next()),
        None | Some("Z")
    )
}

// SIGKILL to every live member of the session, again and again: one that forked between a scan and its signal leaves a
// child the next scan finds. Done when a scan finds only zombies.
fn sweep(sid: i32) {
    let deadline = Instant::now() + SWEEP_LIMIT;
    loop {
        let live = members(sid);
        if live.is_empty() || Instant::now() >= deadline {
            return;
        }
        for pid in live {
            // SAFETY: kill(2) on a process in the session our unreaped child leads.
            unsafe { kill(pid, SIGKILL) };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// Every live process whose session is `sid`; zombies are already gone and only wait to be reaped.
pub(super) fn members(sid: i32) -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            live_in(&stat, sid)
        })
        .collect()
}

// Sample /proc/<pid>/stat: "123 (sleep) S 1 120 120 ..." so the state is the first field after the last ")" and the
// session the fourth.
fn live_in(stat: &str, sid: i32) -> bool {
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return false;
    };
    let fields: Vec<&str> = rest.split_whitespace().take(4).collect();
    matches!(fields[..], [state, _, _, session] if state != "Z" && session.parse() == Ok(sid))
}

#[cfg(test)]
#[path = "airdropsession_tests.rs"]
mod tests;
