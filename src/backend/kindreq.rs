// The file kind of each path the client names, symlinks followed; see docs/protocol.md "kind".
// The pin drop of the rail's FAVORITES heading is the caller: it arms only for one directory.
use super::iomount;
use super::opsreq::OpMsg;
use crate::json::escape;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

// Answered on its own thread like devs: a stat through a mount that stopped answering never returns, and the loop never waits on one.
pub fn request(paths: Vec<String>, id: usize, replies: Sender<OpMsg>) {
    std::thread::spawn(move || {
        let _ = replies.send(OpMsg::Meta { line: answer(&paths, &iomount::mount_body(), id) });
    });
}

// Sample output: {"t":"kind","kinds":{"/home/gm/Downloads":"dir","/home/gm/a.txt":"file","/gone":""}}, each asked path once, in the order asked.
fn answer(paths: &[String], body: &str, id: usize) -> String {
    let mut seen = std::collections::HashSet::new();
    let kinds: Vec<String> = paths.iter().filter(|path| seen.insert(path.as_str()))
        .map(|path| format!(r#""{}":"{}""#, escape(path), kind(path, body))).collect();
    let tag = if id > 0 { format!(r#", "id":{}"#, id) } else { String::new() };
    format!(r#"{{"t":"kind","kinds":{{{}}}{}}}"#, kinds.join(","), tag)
}

// "" where the stat fails or the mount does not answer in time, and for a relative path, which names this process's own directory and nothing the client means.
fn kind(path: &str, body: &str) -> String {
    if !path.starts_with('/') {
        return String::new();
    }
    let owned = PathBuf::from(path);
    let target = owned.clone();
    iomount::call(&owned, body, "kind", move || kind_of(&target)).unwrap_or_default().to_string()
}

fn kind_of(path: &std::path::Path) -> &'static str {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => "dir",
        Ok(_) => "file",
        Err(_) => "",
    }
}

#[cfg(test)]
#[path = "kindreq_tests.rs"]
mod tests;
