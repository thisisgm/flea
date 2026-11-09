// Bulk rename: one validated batch, swaps through temp names, one undo entry.
use crate::backend::ops::valid_name;
use crate::backend::renamecompat;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::error::{from_io, FleaError};
use flea::bytename::ExactText as _;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

// Linux filename components hold at most 255 bytes, the same bound the TUI batch enforces.
const NAME_BYTES: usize = 255;
// The staged names swaps ride through, named in the error when one strands them.
const TEMP_PREFIX: &str = ".flea-bulk-";

// The landed steps with the batch counts, factored out so the worker outcome stays readable.
pub(crate) type BulkOutcome = (Vec<Step>, Result<(usize, usize), FleaError>);

// Sample output: {"t":"bulkrenamed","ok":5,"unchanged":1}
pub fn bulkrenamed_line(ok: usize, unchanged: usize) -> String {
    format!(r#"{{"t":"bulkrenamed","ok":{},"unchanged":{}}}"#, ok, unchanged)
}

fn refused(message: &str) -> FleaError {
    FleaError { where_: "bulkrename".to_string(), path: String::new(), msg: message.to_string() }
}

// Names the item that went missing or was replaced, so a long batch says which one.
fn changed(path: &Path) -> FleaError {
    FleaError { path: path.to_string_lossy().into_owned(), ..refused("Bulk rename refused: a selected item changed.") }
}

struct Item {
    source: PathBuf,
    dest: PathBuf,
    before: ItemIdentity,
}

// The whole batch is checked before anything moves, so a refusal touches nothing.
fn plan(paths: &[String], names: &[String]) -> Result<(Vec<Item>, usize), FleaError> {
    if paths.is_empty() || paths.len() != names.len() {
        return Err(refused("Bulk rename needs the same number of names as items."));
    }
    let sources: Vec<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    let unique: BTreeSet<&PathBuf> = sources.iter().collect();
    if unique.len() != sources.len() || sources.iter().any(|p| !p.is_absolute()) {
        return Err(refused("Bulk rename needs unique absolute paths."));
    }
    let parent = sources[0].parent().ok_or_else(|| refused("Bulk rename works in one folder at a time."))?.to_path_buf();
    for source in &sources {
        if source.parent() != Some(parent.as_path()) {
            return Err(refused("Bulk rename works in one folder at a time."));
        }
    }
    // One identity per source, so a replaced file cannot be renamed as the old one.
    let mut before: Vec<ItemIdentity> = Vec::with_capacity(sources.len());
    for source in &sources {
        let meta = source.symlink_metadata().map_err(|_| changed(source))?;
        before.push(ItemIdentity::record(&meta));
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut items: Vec<Item> = Vec::with_capacity(sources.len());
    let mut unchanged = 0usize;
    for ((source, name), identity) in sources.into_iter().zip(names.iter()).zip(before) {
        if !valid_name(name) || name.contains(['\r', '\n']) || name.len() > NAME_BYTES {
            return Err(refused("Bulk rename refused: a name is invalid."));
        }
        if !seen.insert(name.clone()) {
            return Err(refused("Bulk rename refused: two names collide."));
        }
        let dest = parent.join(name);
        if dest == source {
            unchanged += 1;
            continue;
        }
        items.push(Item { source, dest, before: identity });
    }
    let sources_now: BTreeSet<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    for item in &items {
        match item.dest.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) if sources_now.contains(&item.dest) => {}
            Ok(_) => return Err(refused("Bulk rename refused: a destination already exists.")),
            Err(e) => return Err(from_io("bulkrename", &item.dest.to_string_lossy(), &e)),
        }
    }
    Ok((items, unchanged))
}

fn confirm_dir(path: &Path) {
    let mut confirm = crate::backend::durable::Durability::begin(path);
    if confirm.durable {
        if let Some(parent) = path.parent() {
            confirm.touch(parent);
        }
        if let Err(error) = confirm.flush_dirs() {
            eprintln!("flea: rename {} landed but the drive did not confirm the folder: {}", path.display(), error);
        }
    }
}

fn move_one(from: &Path, to: &Path, before: ItemIdentity) -> Result<Step, FleaError> {
    move_one_with(from, to, before, ItemIdentity::inspect)
}

fn move_one_with(from: &Path, to: &Path, before: ItemIdentity, inspect: impl Fn(&Path) -> Result<ItemIdentity, FleaError>) -> Result<Step, FleaError> {
    renamecompat::rename_path(from, to).map_err(|error| FleaError {
        where_: "bulkrename".to_string(),
        path: error.path,
        msg: error.msg,
    })?;
    confirm_dir(to);
    // The rename has landed, so an unreadable identity keeps the step with the one it moved with.
    let after = inspect(to).unwrap_or_else(|_| before.clone());
    Ok(Step::Moved { from: from.to_path_buf(), to: to.to_path_buf(), before, after, rename_fat: false })
}

// Swaps and cycles ride through temp names in the same folder, so a to b and b to a works.
fn execute(items: Vec<Item>) -> (Vec<Step>, Result<usize, FleaError>) {
    // Each source is read again before the first move, so one replaced since the plan refuses the whole batch.
    for item in &items {
        match item.source.symlink_metadata() {
            Ok(meta) if item.before.same_item(&ItemIdentity::record(&meta)) => {}
            _ => return (Vec::new(), Err(changed(&item.source))),
        }
    }
    let sources: BTreeSet<PathBuf> = items.iter().map(|item| item.source.clone()).collect();
    let dests: BTreeSet<PathBuf> = items.iter().map(|item| item.dest.clone()).collect();
    // A source another item lands on is the occupant, so it is the one that moves aside.
    let staged: BTreeSet<PathBuf> = sources.intersection(&dests).cloned().collect();
    let mut steps: Vec<Step> = Vec::new();
    let mut live: BTreeMap<PathBuf, (PathBuf, ItemIdentity)> = BTreeMap::new();
    // Occupants move aside first, each temp exclusive so it cannot take an existing name.
    for item in &items {
        if !staged.contains(&item.source) {
            live.insert(item.source.clone(), (item.source.clone(), item.before.clone()));
            continue;
        }
        let mut counter = 0u32;
        let temp = loop {
            counter += 1;
            let candidate = item.source.parent().unwrap_or(Path::new("/"))
                .join(format!("{TEMP_PREFIX}{}-{}-{}", std::process::id(), steps.len(), counter));
            if sources.contains(&candidate) || dests.contains(&candidate) {
                continue;
            }
            match candidate.symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break candidate,
                _ => continue,
            }
        };
        match move_one(&item.source, &temp, item.before.clone()) {
            Ok(step) => {
                let after = match &step {
                    Step::Moved { after, .. } => after.clone(),
                    _ => item.before.clone(),
                };
                steps.push(step);
                live.insert(item.source.clone(), (temp, after));
            }
            Err(error) => return (steps, Err(error)),
        }
    }
    for item in &items {
        let (from, before) = live.get(&item.source).cloned().unwrap_or((item.source.clone(), item.before.clone()));
        match move_one(&from, &item.dest, before.clone()) {
            Ok(step) => {
                live.insert(item.source.clone(), (item.dest.clone(), before));
                steps.push(step);
            }
            Err(error) => return (steps, Err(stranded_error(error, &live))),
        }
    }
    let ok = steps.len();
    (steps, Ok(ok))
}

// A failed second loop strands what is still staged, so the error names the temp leaves and the undo that restores them.
fn stranded_error(mut error: FleaError, live: &BTreeMap<PathBuf, (PathBuf, ItemIdentity)>) -> FleaError {
    let mut temps: Vec<String> = live.values()
        .filter_map(|(at, _)| at.file_name().map(|n| n.to_string_lossy().to_string()))
        .filter(|name| name.starts_with(TEMP_PREFIX)).collect();
    temps.sort();
    if temps.is_empty() {
        return error;
    }
    error.msg = format!("{} Staged copies remain under {}. Undo restores them.", error.msg, temps.join(", "));
    error
}

// Like rename, a batch answers inline on a local mount and on a worker past the deadline on a remote one.
pub(crate) fn do_bulkrename(out: &mut impl Write, ops: &mut super::opsdispatch::Ops, paths: Vec<String>, names: Vec<String>) -> Vec<PathBuf> {
    if ops.live.running().is_some() {
        let e = super::opsreq::op_err("bulkrename", "", "an operation is already running");
        writeln!(out, "{}", super::proto::error_line(&e)).ok();
        out.flush().ok();
        return Vec::new();
    }
    let sources: Vec<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    let folder = paths
        .first()
        .and_then(|first| flea::bytename::view(first).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("/"));
    // A source or the folder on a pending mount waits, so the late journal lands first.
    if super::opsdispatch::pending_busy_for(
        out,
        ops,
        "bulkrename",
        &sources,
        std::slice::from_ref(&folder),
    ) {
        return Vec::new();
    }
    let body = super::iomount::mount_body();
    let key = folder.clone();
    match super::iomount::slow_write_with(
        &key,
        &body,
        "bulkrename",
        super::iomount::CALL_DEADLINE,
        move || Ok(bulk_work(paths, names)),
    ) {
        super::iomount::SlowWrite::Ready(outcome) => land_bulkrename(out, ops, outcome),
        super::iomount::SlowWrite::Slow { mount, rx } => {
            writeln!(
                out,
                "{}",
                super::proto::slow_line(
                    "bulkrename",
                    &key.exact_text(),
                    &super::iomount::slow_sentence(&mount, super::iomount::SLOW_BULK_RENAME)
                )
            )
            .ok();
            out.flush().ok();
            // The pending id is only a number: the slot stays with whatever holds it, so a transfer beside this write keeps its id and its cancel.
            let id = ops.claim_id();
            ops.pending.push(super::opsdispatch::SlowClaim {
                id,
                path: key.exact_text().to_string(),
                mount,
            });
            let tx = ops.tx.clone();
            let failed = super::iomount::internal_failure("bulkrename", &key);
            std::thread::spawn(move || {
                let result = rx.recv().unwrap_or(Ok((Vec::new(), Err(failed))));
                let _ = tx.send(super::opsreq::OpMsg::BulkRenameDone { id, result });
            });
            // The slow path lands later through BulkRenameDone, which the session patches on arrival.
            Vec::new()
        }
    }
}

// The whole batch is planned and run as one worker job, so a remote plan never holds the loop.
fn bulk_work(paths: Vec<String>, names: Vec<String>) -> BulkOutcome {
    let (items, unchanged) = match plan(&paths, &names) {
        Ok(planned) => planned,
        Err(error) => return (Vec::new(), Err(error)),
    };
    if items.is_empty() {
        return (Vec::new(), Ok((0, unchanged)));
    }
    let want = items.len();
    let (steps, result) = execute(items);
    match result {
        Ok(_) => (steps, Ok((want, unchanged))),
        Err(error) => (steps, Err(error)),
    }
}

// One journal entry per request, so one undo reverses the in-time answer and the late one alike; answers the landed paths for a session patch.
pub(crate) fn land_bulkrename(out: &mut impl Write, ops: &mut super::opsdispatch::Ops, outcome: Result<BulkOutcome, FleaError>) -> Vec<PathBuf> {
    let (steps, result) = match outcome {
        Ok(landed) => landed,
        Err(error) => {
            writeln!(out, "{}", super::proto::error_line(&error)).ok();
            out.flush().ok();
            return Vec::new();
        }
    };
    let touched = super::undo::Journal::touched_paths(&steps);
    match result {
        Ok((ok, unchanged)) => {
            if !steps.is_empty() {
                ops.journal.push(Entry { op: "bulkrename".to_string(), steps });
            }
            writeln!(out, "{}", bulkrenamed_line(ok, unchanged)).ok();
        }
        Err(error) => {
            // What landed stays journalled as one entry, so z reverts the partial batch.
            if !steps.is_empty() {
                ops.journal.push(Entry { op: "bulkrename".to_string(), steps });
            }
            writeln!(out, "{}", super::proto::error_line(&error)).ok();
        }
    }
    out.flush().ok();
    touched
}

#[cfg(test)]
#[path = "bulkrename_tests.rs"]
mod tests;
