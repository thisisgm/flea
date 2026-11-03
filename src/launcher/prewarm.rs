use crate::backend::aliases::Aliases;
use crate::backend::icons::Names;
use crate::backend::kind::Kinds;
use crate::backend::meta::stat_range;
use crate::backend::mime::Db;
use crate::backend::fsinfo::dev_of;
use crate::backend::proto::listed_line;
use crate::backend::rowguard::{stamped, FIRST_LISTING};
use crate::backend::rows::rows_line;
use crate::backend::scan::scan;
use crate::backend::sort::sort_by_name;
use crate::backend::thumbspec::Thumbnailers;
use crate::error::{from_io, FleaError};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

// Owner-only: the listing names every file in the directory.
const PREWARM_MODE: u32 = 0o600;

// Overlaps Qt init instead of queueing behind it, see AGENTS.md "Prewarm".
pub fn write_prewarm(path: &str, first: usize, dest: &Path) -> Result<(), FleaError> {
    // The pid keeps two launchers off each other's file, see AGENTS.md "Predictable path writes".
    let tmp = PathBuf::from(format!("{}.{}.tmp", dest.display(), std::process::id()));
    let wrote = write_to_tmp(path, first, dest, &tmp);
    if wrote.is_err() {
        // Our own temp file only: dest is the caller's path and exit status is the contract.
        let _ = fs::remove_file(&tmp);
    }
    wrote
}

fn write_to_tmp(path: &str, first: usize, dest: &Path, tmp: &Path) -> Result<(), FleaError> {
    // Prewarm never asks for dotfiles: it mirrors list's own default, see docs/protocol.md.
    let (mut listing, read_ms) = scan(path, false)?;
    let sort_ms = sort_by_name(&mut listing, false, false);

    // corner: unlink our own leftover then create exclusively, see AGENTS.md.
    let _ = fs::remove_file(tmp);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PREWARM_MODE)
        .open(tmp)
        .map_err(|e| from_io("prewarm", &tmp.display().to_string(), &e))?;
    let mut out = BufWriter::new(file);

    writeln!(out, "{}", listed_line(listing.len(), read_ms, sort_ms, dev_of(&PathBuf::from(path)), path, crate::backend::ops::dir_writable(std::path::Path::new(path))))
        .map_err(|e| from_io("prewarm", &tmp.display().to_string(), &e))?;
    let (metas, ms) = stat_range(&PathBuf::from(path), &listing, 0, first);
    // Its own copy: prewarm is one shot, so there is no loop to hoist the load out of.
    let (mime, icons, aliases) = (Db::load(), Names::load(), Aliases::load());
    let thumbs = Thumbnailers::load(&aliases);
    let mut kinds = Kinds::new();
    let rows = rows_line(&PathBuf::from(path), &listing, &metas, 0, ms, &mime, &icons, &aliases, &thumbs, &mut kinds);
    writeln!(out, "{}", stamped(rows, FIRST_LISTING))
        .map_err(|e| from_io("prewarm", &tmp.display().to_string(), &e))?;
    out.flush()
        .map_err(|e| from_io("prewarm", &tmp.display().to_string(), &e))?;
    drop(out);

    // Rename last, so the UI never reads a half-written file.
    fs::rename(tmp, dest).map_err(|e| from_io("prewarm", &dest.display().to_string(), &e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dirsizeworker::Worker;
    use crate::backend::state::{State, Tables};
    use crate::backend::thumbs::Pool;
    use crate::json::{field_str, field_usize};
    use std::sync::{mpsc::channel, Arc};

    // The numbering a backend's first list answers in: the State run() starts with, through the list arm's own adopt.
    fn backend_first_listing(path: &str, cache: PathBuf) -> Option<usize> {
        let ((events, _events), (results, _results)) = (crate::backend::events::channel(), channel());
        let (mut st, tb) = (State::new(Worker::new(events)), Tables::load());
        let pool = Pool::new(1, results, cache, Arc::clone(&tb.aliases), Arc::clone(&tb.thumbs));
        let fd = -1;
        let done = crate::backend::iomount::list_dir(path.to_string(), false, 2, r#"{"c":"list"}"#.to_string(), Arc::clone(&tb.mime), fd, crate::backend::events::channel::<crate::backend::events::Event>().0, &crate::backend::iomount::mount_body()).expect("list");
        let mut out = Vec::new();
        crate::backend::run::adopt_listed(&mut out, &mut st, &pool, &tb, path, done, false);
        field_usize(String::from_utf8(out).unwrap().lines().nth(1)?, "listing")
    }

    #[test]
    fn the_rows_line_names_the_numbering_the_backends_first_list_answers_in() {
        let dir = crate::backend::testdir::TestDir::new("prewarm-listing");
        let listed = dir.dir("listed");
        dir.file("listed/a.txt", "a");
        let dest = dir.join("prewarm.json");
        write_prewarm(listed.to_str().unwrap(), 2, &dest).expect("prewarm");
        let text = fs::read_to_string(&dest).unwrap();
        let rows = text.lines().nth(1).expect("a rows line");
        assert_eq!(field_str(rows, "t").as_deref(), Some("rows"));
        let first = backend_first_listing(listed.to_str().unwrap(), dir.join("cache"));
        assert_eq!(first, Some(FIRST_LISTING as usize), "a backend's first list answers in FIRST_LISTING");
        assert_eq!(field_usize(rows, "listing"), first, "the file stands in for that first list: {}", rows);
        assert!(rows.ends_with(&format!(",\"listing\":{}}}", FIRST_LISTING)), "the numbering rides last, as write_window stamps it: {}", rows);
    }
}
