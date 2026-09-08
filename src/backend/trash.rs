// gio trash is the freedesktop trash this box already shares with every GTK app; nothing about the spec is written here, only an argv and a result.
use crate::error::FleaError;
use std::path::{Path, PathBuf};
use std::process::Command;

// gio trash --list prints one entry per line as "<uri>\t<original path>".
const LIST_SEP: char = '\t';

// What a successful trash recorded, which is the only thing that can reverse it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub original: PathBuf,
    pub uri: String,
}

// corner: gio on an unresponsive network mount can hang, which stalls this operation's own thread and nothing else.
fn gio(args: &[&str]) -> Option<std::process::Output> {
    Command::new("gio").args(args).output().ok()
}

// Sample line: trash:///a.txt\t/home/gm/a.txt
fn parse_list(stdout: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(2, LIST_SEP);
        let uri = match parts.next() {
            Some(u) if !u.is_empty() => u,
            _ => continue,
        };
        if let Some(original) = parts.next() {
            if !original.is_empty() {
                out.push(Entry { original: PathBuf::from(original), uri: uri.to_string() });
            }
        }
    }
    out
}

pub fn list() -> Vec<Entry> {
    match gio(&["trash", "--list"]) {
        Some(o) => parse_list(&String::from_utf8_lossy(&o.stdout)),
        None => Vec::new(),
    }
}

// The URI is captured here rather than looked up later, because gio trash --restore refuses an original
// path outright and two files trashed from one path both list that same path, so a later lookup is ambiguous.
pub fn trash(paths: &[PathBuf]) -> (Vec<Entry>, usize) {
    // A path that is not on disk before the call is nothing this call trashed, so it is counted failed and
    // never journaled: read as "gone, so it went" it became a Trashed step with no URI, and undo stopped
    // on that step and left the rest of the batch in the trash. A stale listing is where such a path
    // comes from, and the changed line is what refreshes it.
    let (present, missing): (Vec<&PathBuf>, Vec<&PathBuf>) =
        paths.iter().partition(|p| p.symlink_metadata().is_ok());
    let mut failed = missing.len();
    if present.is_empty() {
        return (Vec::new(), failed);
    }
    let before = list();
    let mut argv: Vec<String> = vec!["trash".to_string(), "--".to_string()];
    for p in &present {
        argv.push(p.to_string_lossy().to_string());
    }
    let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    // The exit status covers the whole batch, so which paths actually went is read off the filesystem instead.
    let _ = gio(&refs);
    let after = list();
    let mut ok = Vec::new();
    for p in present {
        if p.symlink_metadata().is_ok() {
            failed += 1;
            continue;
        }
        match newest_entry_for(&before, &after, p) {
            Some(e) => ok.push(e),
            // corner: the file went but gio listed no entry for it, so it is gone and simply not reversible.
            None => ok.push(Entry { original: p.clone(), uri: String::new() }),
        }
    }
    (ok, failed)
}

// Only an entry that was not already in the trash before this call can be one this call put there.
fn newest_entry_for(before: &[Entry], after: &[Entry], path: &Path) -> Option<Entry> {
    after
        .iter()
        .find(|e| e.original == path && !before.iter().any(|b| b.uri == e.uri))
        .cloned()
}

pub fn restore(entry: &Entry) -> Result<(), FleaError> {
    if entry.uri.is_empty() {
        return Err(err("this item was trashed without a trash entry, so it cannot be restored"));
    }
    match gio(&["trash", "--restore", &entry.uri]) {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => {
            let msg = String::from_utf8_lossy(&o.stderr);
            Err(err(msg.lines().last().unwrap_or("gio trash --restore failed")))
        }
        None => Err(err("gio is not available to restore from the trash")),
    }
}

fn err(msg: &str) -> FleaError {
    FleaError { where_: "undo".to_string(), path: String::new(), msg: msg.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    // No gio runs for this one, because there is nothing on disk to hand it, so the test needs no trash.
    #[test]
    fn a_path_that_was_already_gone_is_a_failure_and_not_a_step_to_undo() {
        let d = TestDir::new("trashgone");
        let gone = d.join("never-existed.txt");
        let (entries, failed) = trash(&[gone.clone(), d.join("nor-this")]);
        assert!(entries.is_empty(), "nothing this call did not do can be undone");
        assert_eq!(failed, 2);
    }

    #[test]
    fn a_list_line_splits_on_the_tab_and_keeps_a_path_containing_spaces() {
        let out = "trash:///a.txt\t/home/gm/a.txt\ntrash:///my%20file\t/home/gm/my file\n";
        let got = parse_list(out);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].uri, "trash:///a.txt");
        assert_eq!(got[0].original, PathBuf::from("/home/gm/a.txt"));
        assert_eq!(got[1].original, PathBuf::from("/home/gm/my file"));
    }

    #[test]
    fn a_malformed_list_line_is_skipped_rather_than_panicking() {
        let got = parse_list("no tab here\n\n\tmissing uri\ntrash:///ok\t/x\ntrash:///nopath\t\n");
        assert_eq!(got.len(), 1, "only the one well-formed line survives");
        assert_eq!(got[0].uri, "trash:///ok");
    }

    #[test]
    fn only_an_entry_absent_before_the_call_is_taken_as_ours() {
        let p = PathBuf::from("/home/gm/dup.txt");
        let before = vec![Entry { original: p.clone(), uri: "trash:///dup.txt".to_string() }];
        let after = vec![
            Entry { original: p.clone(), uri: "trash:///dup.txt".to_string() },
            Entry { original: p.clone(), uri: "trash:///dup.2.txt".to_string() },
        ];
        // Both entries name the same original, which is exactly why the URI is captured at trash time.
        let got = newest_entry_for(&before, &after, &p).expect("the new one");
        assert_eq!(got.uri, "trash:///dup.2.txt");
    }

    #[test]
    fn an_entry_with_no_uri_refuses_to_restore_instead_of_running_gio_with_an_empty_argument() {
        let e = Entry { original: PathBuf::from("/x"), uri: String::new() };
        let err = restore(&e).expect_err("must refuse");
        assert_eq!(err.where_, "undo");
        assert!(err.msg.contains("cannot be restored"));
    }

    #[test]
    fn trashing_nothing_runs_no_subprocess_and_reports_nothing() {
        let (ok, failed) = trash(&[]);
        assert!(ok.is_empty());
        assert_eq!(failed, 0);
    }
}
