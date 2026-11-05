use crate::backend::aliases::Aliases;
use crate::backend::mime::Db;
use crate::backend::thumbcache::{Cache, Hit};
use crate::backend::thumbs::{run_one, Job, Outcome, Tables};
use crate::backend::thumbspec::Thumbnailers;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Main rule 4: the shelf draws a thumbnail in the mark slot, and it has no listing to address rows
// in, so it asks by path. One line per path, the cache file or `none`, then a tab and the path
// back, because the card asked for a set and answers can only be matched by the path they name.
pub fn command(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("flea: shelf thumb takes at least one path");
        return 2;
    }
    let mime = Db::load();
    let aliases = Arc::new(Aliases::load());
    let specs = Arc::new(Thumbnailers::load(&aliases));
    let root = crate::backend::thumbcache::default_root();
    let cache = Cache::at(root.clone());
    for path in rest {
        let file = PathBuf::from(path);
        let answer = thumb_of(&file, &mime, &aliases, &specs, &cache, &root);
        println!("{}", line(answer.as_deref(), path));
    }
    0
}

// The cache is the answer whenever it holds one; a miss is produced through the writer every other
// part of Flea produces one with, so the shelf and the pane share one cache and one failure record.
/// Find or generate the cached thumbnail for a shelf item.
fn thumb_of(
    file: &Path,
    mime: &Db,
    aliases: &Arc<Aliases>,
    specs: &Arc<Thumbnailers>,
    cache: &Cache,
    root: &Path,
) -> Option<String> {
    let meta = std::fs::metadata(file).ok().filter(|m| m.is_file())?;
    let name = file.file_name()?.to_str()?;
    let kind = mime
        .lookup(name)
        .filter(|m| specs.for_mime(m, aliases).is_some())
        .map(str::to_string)?;
    let mtime = meta.mtime();
    match cache.lookup(file, mtime, specs.fingerprint()) {
        Hit::Ready(ready) => Some(ready.to_string_lossy().to_string()),
        Hit::Failed => None,
        // A miss is produced here, in this process, with the tables the pane's own pool uses: the
        // shelf asks for one path and waits, so it needs no queue and no worker.
        Hit::Miss => {
            let tables = Tables {
                aliases: Arc::clone(aliases),
                specs: Arc::clone(specs),
                cache: Cache::at(root.to_path_buf()),
                worker: None,
            };
            let mut job = Job { replies: None, path: file.to_path_buf(), mtime, trash_identity: None, mime: kind, trace: None };
            match run_one(&tables, &mut job) {
                Outcome::Ready(made) => Some(made.to_string_lossy().to_string()),
                Outcome::Failed | Outcome::Cancelled => None,
            }
        }
    }
}

// The answer the card parses: the cache file or `none`, a tab, then the path that was asked for,
// because a set of paths can only be matched to its answers by the path each one names.
fn line(answer: Option<&str>, path: &str) -> String {
    format!("{}\t{}", answer.unwrap_or("none"), path)
}

#[cfg(test)]
#[path = "shelfthumb_tests.rs"]
mod tests;
