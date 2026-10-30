// The shelf's own file actions, Actions rule 1: every one is a `flea shelf` call, and they run the
// same Rust transfer engine, conflict handling, undo and trash the pane runs. Rule 2: no new file
// operation exists here, because one the pane cannot do would be a feature in the wrong place.
use crate::backend::opsreq::{run_transfer_checked, transferdone_line, transferitem_line,
                             transferprogress_line, transferstarted_line, usable_dest, OpMsg};
use crate::backend::proto::error_line;
use crate::backend::undo::Step;
use crate::error::io_message;
use crate::shelf::{now_ms, Shelf};
use crate::shelfplaces;
use crate::shelfundo;
use crate::uistore;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const DIR: &str = "omarchy/flea-shelf";
const CANCEL: &str = "cancel";
// The card is another process and this binary carries no signal handling, so a cancel is a file the
// card writes and the transfer watches. Fast enough that esc feels immediate, slow enough to be free.
const CANCEL_POLL_MS: u64 = 100;

// flea shelf move|copy <dest> <path>...: the paths are what the card chose, because chosen-or-whole
// is the card's question and the answer crosses as an argument list rather than as a rule here.
pub fn transfer(moving: bool, rest: &[String]) -> i32 {
    let (dest, paths) = match rest.split_first() {
        Some((dest, paths)) if !paths.is_empty() => (dest, paths.to_vec()),
        _ => {
            eprintln!("flea: shelf move and copy take a destination, then the paths");
            return 2;
        }
    };
    let dest_name = dest.clone();
    let dest = match usable_dest(dest) {
        Ok(dest) => dest,
        Err(e) => {
            println!("{}", error_line(&e));
            return 2;
        }
    };
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let landing = dest.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let marker = match cancel_file() {
        Ok(marker) => marker,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let _ = std::fs::remove_file(&marker);
    watch_cancel(&marker, &cancel);
    println!("{}", transferstarted_line(0, paths.len(), moving));
    let (tx, rx) = channel::<OpMsg>();
    let watched = paths.clone();
    let engine = {
        let cancel = Arc::clone(&cancel);
        thread::spawn(move || run_transfer_checked(0, moving, paths, dest, cancel, tx, None, None, crate::backend::collide::Policy::default()))
    };
    let (moved, mut failed, steps) = report(rx, &watched, moving);
    // A panicking engine reports no item at all, so joining it is the only thing that can tell a
    // clean run from one that died before it started.
    if engine.join().is_err() {
        eprintln!("flea: the transfer stopped before it finished");
        failed += 1;
    }
    cancel.store(true, Ordering::Relaxed);
    let _ = std::fs::remove_file(&marker);
    // Main rule 10: a pinned row survives its own move, so its entry follows the file to the new
    // path while every other moved reference leaves the pile.
    let pinned = shelf.pinned_among(&moved);
    let followed: Vec<(String, String)> = pinned
        .iter()
        .filter_map(|from| landed(&landing, from).map(|to| (from.clone(), to)))
        .collect();
    // Rule 4: a move empties what it moved and nothing else; a copy leaves the pile exactly as it was.
    let mut kept = true;
    if let Err(e) = shelf.settle(&moved) {
        eprintln!("flea: the shelf kept its references ({})", e);
        kept = false;
    }
    if let Err(e) = shelf.repoint(&followed) {
        eprintln!("flea: a pin stayed on the old path ({})", e);
        kept = false;
    }
    if let Err(e) = shelfplaces::remember(&dest_name) {
        eprintln!("flea: the destination was not remembered ({})", e);
    }
    // Keys board: z undoes, which the landed sentence promises, so a move leaves the one step back
    // on disk. This process is gone by the time the card presses it.
    if moving {
        if let Err(e) = record_undo(&steps) {
            eprintln!("flea: this move cannot be undone ({})", e);
        }
    }
    i32::from(failed > 0 || !kept)
}

// Where a moved file landed, and None when nothing of that name is there: the engine renames on a
// collision and says so to nobody, so a pin follows only a path this can see for itself.
fn landed(dest: &Path, from: &str) -> Option<String> {
    let name = Path::new(from).file_name()?;
    let to = dest.join(name);
    if to.symlink_metadata().is_err() {
        return None;
    }
    Some(to.to_string_lossy().to_string())
}

// A move that changed nothing records nothing, the same rule the pane's journal keeps.
fn record_undo(steps: &[Step]) -> Result<(), String> {
    let moves = shelfundo::moves_from(steps);
    if moves.is_empty() {
        return Ok(());
    }
    shelfundo::Moves::user()?.record(&moves, now_ms())
}

// Every line the pane's own transfer prints, in the same vocabulary, so the card reads one protocol.
// The engine's own steps come back with them, because they carry where each item actually landed.
fn report(rx: std::sync::mpsc::Receiver<OpMsg>, paths: &[String], moving: bool) -> (Vec<String>, usize, Vec<Step>) {
    let mut moved = Vec::new();
    let mut steps = Vec::new();
    let mut failed = 0;
    for msg in rx {
        match msg {
            OpMsg::Meta { line } => println!("{}", line),
            OpMsg::Progress { id, index, name, bytes, total, scanned } => {
                println!("{}", transferprogress_line(id, index, &name, bytes, total, scanned));
            }
            OpMsg::Item { id, index, name, ok, err } => {
                println!("{}", transferitem_line(id, index, &name, ok, &err));
                if !ok {
                    failed += 1;
                } else if moving {
                    if let Some(path) = paths.get(index) {
                        moved.push(path.clone());
                    }
                }
            }
            OpMsg::TransferDone { id, ok, failed: bad, skipped, cancelled, retry, entry, durable, note, refused } => {
                println!("{}", transferdone_line(id, ok, bad, skipped, cancelled, &retry, durable, &note, &refused));
                steps = entry.steps;
            }
            _ => {}
        }
    }
    (moved, failed, steps)
}

pub fn paths(rest: &[String]) -> i32 {
    for path in rest {
        println!("{}", path);
    }
    0
}

// flea shelf clip <path>...: the files themselves on the clipboard, the way Flea's own copy puts
// them, so a file manager pastes copies; one regular file also offers its own bytes under its type,
// so a chat app or a browser pastes the file rather than its path.
pub fn clip(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("flea: shelf clip takes the paths");
        return 2;
    }
    match crate::clip::own::spawn_owner_content(rest) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

// flea shelf send <peer> <path>...: Taildrop already ships in Flea, and a pile gathered from five
// folders is the best payload it will ever get. Rule 4: a send leaves the pile exactly as it was,
// because a send reports only by notification and nobody here knows whether it landed.
pub fn send(rest: &[String]) -> i32 {
    let (peer, paths) = match rest.split_first() {
        Some((peer, paths)) if !paths.is_empty() => (peer, paths.to_vec()),
        _ => {
            eprintln!("flea: shelf send takes a peer, then the paths");
            return 2;
        }
    };
    let finished = std::process::Command::new("omarchy-tailscale-send")
        .arg(peer)
        .args(&paths)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match finished {
        Ok(status) if status.success() => 0,
        Ok(_) => {
            eprintln!("flea: {} did not take that send", peer);
            2
        }
        Err(_) => {
            eprintln!("flea: omarchy-tailscale-send is not installed, so nothing was sent");
            2
        }
    }
}

// flea shelf peers: the send flyout's rows. Sample input, the fields Taildrop.js reads of
// `tailscale status --json`: {"Peer":{"nodekey:aa":{"DNSName":"macbookair.tail1234.ts.net.","HostName":"macbookair","Online":true}}}
pub fn peers() -> i32 {
    let out = std::process::Command::new("tailscale")
        .arg("status")
        .arg("--json")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    let Ok(out) = out else {
        eprintln!("flea: Tailscale is not installed, so there are no peers to show");
        return 2;
    };
    if !out.status.success() {
        eprintln!("flea: tailscale would not answer with its status");
        return 2;
    }
    for peer in peer_names(&String::from_utf8_lossy(&out.stdout)) {
        println!("{}", peer);
    }
    0
}

pub fn peer_names(status: &str) -> Vec<String> {
    let Ok(doc) = crate::jsondoc::parse(status) else { return Vec::new() };
    let Some(crate::jsondoc::Json::Obj(peers)) = doc.get("Peer") else { return Vec::new() };
    let mut out = Vec::new();
    for (_, peer) in peers {
        let dns = peer.get("DNSName").and_then(crate::jsondoc::Json::as_str).unwrap_or_default();
        let host = peer.get("HostName").and_then(crate::jsondoc::Json::as_str).unwrap_or_default();
        let name = if dns.is_empty() { host.to_string() } else { dns.trim_end_matches('.').to_string() };
        // An exit-node relay is never a send target, the same rule the OEM's own isMullvadPeer applies.
        if name.is_empty() || name.contains(".mullvad.ts.net") {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out
}

// flea shelf cancel: esc in the card while an action runs, which the engine reads as its own cancel.
pub fn cancel() -> i32 {
    let marker = match cancel_file() {
        Ok(marker) => marker,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let dir = match marker.parent() {
        Some(dir) => dir,
        None => return 2,
    };
    if let Err(e) = uistore::make_dir(dir) {
        eprintln!("flea: {}", e);
        return 2;
    }
    match std::fs::write(&marker, "cancel\n") {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flea: the cancel could not be written: {}", io_message(&e));
            2
        }
    }
}

fn watch_cancel(marker: &Path, cancel: &Arc<AtomicBool>) {
    let marker = marker.to_path_buf();
    let flag = Arc::clone(cancel);
    thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            if marker.symlink_metadata().is_ok() {
                flag.store(true, Ordering::Relaxed);
                return;
            }
            thread::sleep(Duration::from_millis(CANCEL_POLL_MS));
        }
    });
}

fn cancel_file() -> Result<PathBuf, String> {
    Ok(uistore::state_home()?.join(DIR).join(CANCEL))
}

#[cfg(test)]
#[path = "shelfops_tests.rs"]
mod tests;
