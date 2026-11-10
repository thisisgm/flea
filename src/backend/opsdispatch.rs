// Write dispatch: destination-keyed jobs share workers; other mutations retain their existing slot.
use crate::backend::iomount::{mount_body, mount_key, CALL_DEADLINE};
use crate::backend::ops;
use crate::backend::opsreq::{
    duplicated_line, made_line, op_err, run_trash,
    transferdone_line, transferitem_line, transferprogress_line, transferstarted_line, undone_line, usable_dest,
    LinkOutcome, OpMsg,
};
use crate::backend::listing::Listing;
use crate::backend::proto::error_line;
use crate::backend::undo::{Entry, ItemIdentity, Journal, Step};
use crate::error::FleaError;
use flea::bytename::ExactText as _;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[path = "created_request.rs"]
pub(crate) mod created_request;

// The answer for a menu route whose captured selection is gone or differs from the request; a path outside a held selection gets selected_path's own sentence.
pub(crate) const SELECTION_CHANGED: &str = "Selected items changed; reopen the menu.";

// A write that outlives dispatch, keyed by the mount it holds until publication.
pub(crate) struct SlowClaim {
    pub id: usize,
    pub path: String,
    pub mount: PathBuf,
}

// Everything the write operations own, kept apart from the listing state they never touch.
pub(crate) struct Ops {
    pub journal: super::lazy::Local<Journal>,
    pub jobs: super::jobs::Jobs,
    pub permissions: super::permissions::Permissions,
    pub picker: Option<super::picker::Picker>,
    pub menuactions: Option<super::menu_actions::MenuActions>,
    pub trashbrowser: Option<super::trashbrowse::TrashBrowser>,
    pub transfer_retry: (usize, Vec<(PathBuf, ItemIdentity)>),
    pub question: Option<super::collide::Question>,
    pub asked: usize,
    pub next_id: usize,
    // The operation on the thread and its cancel flag, shared with the stdin helper; one at a time.
    pub live: Arc<super::opscancel::Live>,
    // Pending writes; a second write on the same mount waits until the first journals.
    pub pending: Vec<SlowClaim>,
    // Trash batches announced but not yet journaled: undo and redo wait on them, every other write runs.
    pub settling: Vec<(usize, String)>,
    // Detached jobs, so a quit cancels each one; they run alongside by design.
    pub detached: Arc<super::opscancel::DetachedJobs>,
    pub tx: super::lazy::DeferredSender<OpMsg>,
}

impl Ops {
    #[cfg(test)]
    pub fn new(tx: Sender<OpMsg>) -> Ops {
        Self::with_journal(tx, Journal::new().into())
    }

    fn with_journal(tx: Sender<OpMsg>, journal: super::lazy::Local<Journal>) -> Ops {
        let live = Arc::new(super::opscancel::Live::new());
        Ops {
            journal,
            jobs: Default::default(),
            permissions: super::permissions::Permissions::default(),
            picker: None,
            menuactions: None,
            trashbrowser: None,
            transfer_retry: (0, Vec::new()),
            question: None,
            asked: 0,
            next_id: 1,
            live: Arc::clone(&live),
            pending: Vec::new(),
            settling: Vec::new(),
            detached: Arc::clone(&live.1),
            tx: tx.into(),
        }
    }

    // Production shares one undo history across every backend in the session; tests keep new().
    pub fn new_shared(tx: Sender<OpMsg>) -> Ops {
        Self::with_journal(tx, super::lazy::Local::new(|| {
            let mut journal = Journal::new();
            journal.attach_shared();
            journal
        }))
    }

    // An id with no slot claimed: archive and convert are id-keyed and run concurrently by design,
    // so they number themselves without taking the transfer's one-at-a-time slot.
    pub fn claim_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // A fresh flag per operation, so a cancel can never reach the operation after the one it was aimed at.
    pub(crate) fn claim_transfer(&mut self) -> (usize, Arc<AtomicBool>) {
        let id = self.next_id;
        self.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.live.claim(id, &cancel);
        (id, cancel)
    }
}

// Non-job mutations retain their existing busy refusal while another mutation is in flight.
fn busy(out: &mut impl Write, where_: &str) -> bool {
    let e = op_err(where_, "", "an operation is already running");
    writeln!(out, "{}", error_line(&e)).ok();
    out.flush().ok();
    true
}

// Explicit paths win; a rows form is resolved against the listing here, at request time, so the
// operation still owns a snapshot that outlives whatever the listing does next.
pub(crate) fn resolve_rows(paths: Vec<String>, rows: &[usize], base: &Path, listing: &Listing) -> Vec<String> {
    if !paths.is_empty() {
        return paths;
    }
    rows.iter()
        .filter(|&&r| r < listing.len())
        .map(|&r| flea::bytename::path_text(&listing.path(base, r)).into_owned())
        .collect()
}

pub(crate) fn start_transfer(out: &mut impl Write, ops: &mut Ops, op: &str, paths: Vec<String>, dest: &str, collide: super::collide::Ask) {
    start_transfer_checked(out, ops, op, paths, dest, None, None, collide)
}

pub(crate) fn request_menu_action(out: &mut impl Write, ops: &mut Ops, line: String, paths: Vec<String>, cursor: Option<String>) {
    let deleting = crate::json::field_str(&line, "op").as_deref() == Some("delete");
    if deleting && (ops.live.running().is_some() || ops.jobs.unfinished() > 0) {
        writeln!(out, "{}", super::menu_actions::response(&line, Err("An operation is already running.".into()))).ok();
        out.flush().ok();
        return;
    }
    // A delete names its targets by snapshot, so with no paths to check any pending write blocks it.
    let snapshot = deleting.then(|| crate::json::field_usize(&line, "id")).flatten()
        .and_then(|id| menu_sources(ops, id).ok()).flatten();
    if deleting && delete_blocked(ops, &paths, cursor.as_deref(), snapshot.as_deref()) {
        writeln!(out, "{}", super::menu_actions::response(&line, Err("An operation is already running.".into()))).ok();
        out.flush().ok();
        return;
    }
    let replies = ops.tx.clone();
    let accepted = ops.menuactions.get_or_insert_with(|| super::menu_actions::MenuActions::new(replies)).request(line, paths, cursor);
    if deleting && accepted { ops.claim_transfer(); }
}

pub(crate) fn start_menu_transfer(out: &mut impl Write, ops: &mut Ops, op: &str, id: usize, dest: &str, collide: super::collide::Ask) {
    match super::collide::menu_sources(ops, id, dest, &collide) {
        Ok((items, destination)) => {
            let paths = items.iter().map(|item| item.path.exact_text().into()).collect();
            start_transfer_checked(out, ops, op, paths, dest, Some(items), destination, collide);
        }
        Err(message) => {
            writeln!(out, "{}", error_line(&op_err("transfer", "", &message))).ok();
            out.flush().ok();
        }
    }
}

#[path = "transferstart.rs"]
mod transferstart;
use transferstart::start_transfer_checked;

// A cancel reaches the matching slot operation or detached job without changing other flags.
pub(crate) fn cancel_transfer(out: &mut impl Write, ops: &mut Ops, id: usize) {
    ops.live.cancel(id);
    ops.jobs.advance(out, &ops.detached, &ops.tx);
}

// One gate for every write: each resolved source and the destination wait while their mount holds a slow write.
pub(crate) fn pending_on_any(ops: &Ops, sources: &[PathBuf], dests: &[PathBuf]) -> bool {
    let body = if ops.pending.is_empty() { return false } else { mount_body() };
    sources.iter().chain(dests.iter()).any(|p| pending_on(ops, &body, &p.exact_text()))
}

// Same gate with the standard refusal, so the slot-taking writes share one answering line.
pub(crate) fn pending_busy_for(out: &mut impl Write, ops: &Ops, where_: &str, sources: &[PathBuf], dests: &[PathBuf]) -> bool {
    if pending_on_any(ops, sources, dests) {
        busy(out, where_);
        return true;
    }
    false
}
// A path whose mount holds a pending slow write waits, so the late journal lands first.
pub(crate) fn pending_on(ops: &Ops, body: &str, path: &str) -> bool {
    let key = mount_key(&flea::bytename::view(path), body);
    ops.pending.iter().any(|held| held.mount == key)
}

// Undo and redo wait while any slow write is pending, naming its path, so no reversal races the late journal.
fn pending_busy(out: &mut impl Write, ops: &Ops, where_: &str) -> bool {
    if let Some(path) = ops.pending.first().map(|held| held.path.clone()).or_else(|| super::trashland::settling_path(ops)) {
        let denied = op_err(where_, &path, "an operation is already running");
        writeln!(out, "{}", error_line(&denied)).ok();
        out.flush().ok();
        return true;
    }
    if ops.journal.deferred.is_some() { return busy(out, where_); }
    false
}

// A delete names its targets by snapshot, so with no paths to check any pending write blocks it.
fn delete_blocked(ops: &Ops, paths: &[String], cursor: Option<&str>, selection: Option<&[super::menu_actions::Selected]>) -> bool {
    let mut sources: Vec<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    if let Some(at) = cursor {
        sources.push(flea::bytename::exact(at));
    }
    if let Some(items) = selection {
        sources.extend(items.iter().map(|item| item.path.clone()));
    }
    if sources.is_empty() {
        return !ops.pending.is_empty();
    }
    pending_on_any(ops, &sources, &[])
}

// A landed Done clears only its own pending entry, so a second mount's write stays busy.
pub(crate) fn forget_pending(ops: &mut Ops, id: usize) {
    ops.pending.retain(|held| held.id != id);
}

pub(crate) fn menu_sources(ops: &Ops, id: usize) -> Result<Option<Vec<super::menu_actions::Selected>>, String> {
    if id == 0 { return Ok(None); }
    ops.menuactions.as_ref().ok_or_else(|| super::menu_actions::SELECTION_CHANGED.to_string())
        .and_then(|menu| menu.selection(id)).map(Some)
}

pub(crate) fn start_trash(out: &mut impl Write, ops: &mut Ops, paths: Vec<String>, menu_id: usize, request: usize) {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "trash");
        return;
    }
    let selection = match menu_sources(ops, menu_id) {
        Ok(selection) => selection,
        Err(message) => { writeln!(out, "{}", error_line(&op_err("trash", "", &message))).ok(); out.flush().ok(); return; }
    };
    // A path on a pending mount waits; writes elsewhere run beside the held write.
    let mut sources: Vec<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    if let Some(items) = &selection {
        sources.extend(items.iter().map(|item| item.path.clone()));
    }
    if pending_busy_for(out, ops, "trash", &sources, &[]) {
        return;
    }
    let (id, _) = ops.claim_transfer();
    writeln!(out, r#"{{"t":"trashstarted","id":{},"n":{},"cancellable":false}}"#, id, paths.iter().filter(|path| !path.is_empty()).count()).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || run_trash(id, request, paths, tx, selection));
}

pub(crate) fn start_duplicate(out: &mut impl Write, ops: &mut Ops, path: &str, menu_id: usize) {
    created_request::duplicate(out, ops, path, menu_id, 0);
}

// A menu rename captures the selected identity before its worker outlives the menu and listing.
pub(crate) fn do_menu_rename(out: &mut impl Write, ops: &mut Ops, path: &str, to: &str, id: usize) -> Vec<PathBuf> {
    let selected = ops.menuactions.as_ref().ok_or_else(|| super::menu_actions::SELECTION_CHANGED.to_string())
        .and_then(|menu| menu.selected_path(id, &flea::bytename::view(path)));
    match selected {
        Ok(item) => {
            if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
                busy(out, "rename");
                return Vec::new();
            }
            let from = path.to_string();
            let name = to.to_string();
            do_rename_with(out, ops, path, CALL_DEADLINE, move || {
                item.current().map_err(|error| op_err("rename", &from, &error))?;
                ops::rename(&flea::bytename::view(&from), &name)
            })
        }
        Err(error) => { writeln!(out, "{}", error_line(&op_err("rename", path, &error))).ok(); out.flush().ok(); Vec::new() }
    }
}

// Even a local rename can wait on the filesystem; the dispatcher only accepts its immutable paths.
pub(crate) fn do_rename(out: &mut impl Write, ops: &mut Ops, path: &str, to: &str) -> Vec<PathBuf> {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "rename");
        return Vec::new();
    }
    let from = path.to_string();
    let name = to.to_string();
    do_rename_with(out, ops, path, CALL_DEADLINE, move || ops::rename(&flea::bytename::view(&from), &name))
}

pub(crate) fn do_rename_with<F>(out: &mut impl Write, ops: &mut Ops, path: &str, deadline: Duration, work: F) -> Vec<PathBuf>
where F: FnOnce() -> Result<(PathBuf, Vec<Step>), FleaError> + Send + 'static,
{
    let parent = flea::bytename::view(path).parent().unwrap_or(Path::new("/")).to_path_buf();
    if pending_busy_for(out, ops, "rename", &[flea::bytename::exact(path)], &[parent]) { return Vec::new(); }
    super::renamework::start(ops, path.to_string(), deadline, work);
    Vec::new()
}

// A local mkdir answers inline; a remote mkdir uses the bounded slow-write worker.
pub(crate) fn do_mkdir(out: &mut impl Write, ops: &mut Ops, parent: &str, name: &str, default_base: Option<&str>) -> Vec<PathBuf> {
    do_mkdir_requested(out, ops, parent, name, default_base, 0)
}
pub(crate) fn do_mkdir_requested(out: &mut impl Write, ops: &mut Ops, parent: &str, name: &str, default_base: Option<&str>, request: usize) -> Vec<PathBuf> {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        let mut legacy = Vec::new();
        busy(&mut legacy, "mkdir");
        created_request::refused(out, request, "made", legacy);
        return Vec::new();
    }
    let base = parent.to_string();
    let given = name.to_string();
    let default_base = default_base.map(str::to_owned);
    let work = move || match default_base.as_deref() {
        Some(default_base) => ops::mkdir_with_base(&flea::bytename::view(&base), &given, Some(default_base)),
        None => ops::mkdir(&flea::bytename::view(&base), &given),
    };
    if request == 0 { do_mkdir_with(out, ops, parent, CALL_DEADLINE, work) }
    else { created_request::mkdir_with(out, ops, parent, request, CALL_DEADLINE, work) }
}
// The work is a parameter so tests hold it on a channel while production runs the real mkdir.
pub(crate) fn do_mkdir_with<F>(out: &mut impl Write, ops: &mut Ops, parent: &str, deadline: Duration, work: F) -> Vec<PathBuf>
where
    F: FnOnce() -> Result<(PathBuf, Vec<Step>), FleaError> + Send + 'static,
{
    created_request::mkdir_with(out, ops, parent, 0, deadline, work)
}
// One journal entry per request, so one undo reverses the in-time answer and the late one alike.
fn land_mkdir(out: &mut impl Write, ops: &mut Ops, result: Result<(PathBuf, Vec<Step>), FleaError>) -> Vec<PathBuf> {
    let touched = match &result {
        Ok((_, steps)) => Journal::touched_paths(steps),
        Err(_) => Vec::new(),
    };
    match result {
        Ok((dir, steps)) => {
            ops.journal.push(Entry { op: "mkdir".to_string(), steps });
            writeln!(out, "{}", made_line(true, &dir.exact_text())).ok();
        }
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
        }
    }
    out.flush().ok();
    touched
}

// Answers every path the reversal may have touched, failed or not, so a session patches its index.
pub(crate) fn do_undo(out: &mut impl Write, ops: &mut Ops) -> super::undo::Reached {
    do_undo_matching(out, ops, None)
}

pub(crate) fn do_undo_matching(
    out: &mut impl Write,
    ops: &mut Ops,
    token: Option<&super::undo::UndoToken>,
) -> super::undo::Reached {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "undo");
        return Default::default();
    }
    if pending_busy(out, ops, "undo") {
        return Default::default();
    }
    match if let Some(key) = token {
        ops.journal.undo_for_job(key)
    } else {
        ops.journal.undo()
    } {
        Ok(op) => writeln!(out, "{}", undone_line(&op, true)).ok(),
        Err(e) => writeln!(out, "{}", error_line(&e)).ok(),
    };
    out.flush().ok();
    // A failing older step stops after newer steps already reversed, so a failed undo patches too.
    ops.journal.take_last_touched()
}

// A link the journal cannot identify is removed at once, so only a named leftover stays.
fn remove_made_link(to: &Path) -> std::io::Result<()> {
    if fail_link_verify() {
        return Err(std::io::Error::from_raw_os_error(super::movebatch::ESTALE));
    }
    std::fs::remove_file(to)
}

// The identity read beside that cleanup, failing together on a flaky mount.
fn inspect_made_link(to: &Path) -> Result<ItemIdentity, crate::error::FleaError> {
    if fail_link_verify() {
        let stale = std::io::Error::from_raw_os_error(super::movebatch::ESTALE);
        return Err(crate::error::from_io("journal", &to.to_string_lossy(), &stale));
    }
    ItemIdentity::inspect(to)
}

#[cfg(test)]
thread_local! {
    static FAIL_LINK_VERIFY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Both the identity read and its cleanup fail, the way EACCES or ESTALE on a flaky mount breaks both.
#[cfg(test)]
pub fn test_fail_link_verify(fail: bool) {
    FAIL_LINK_VERIFY.with(|v| v.set(fail));
}

fn fail_link_verify() -> bool {
    #[cfg(test)]
    {
        FAIL_LINK_VERIFY.with(|v| v.get())
    }
    #[cfg(not(test))]
    {
        false
    }
}

// Paste as links: the slot and the collide answer are decided here, the links land beside the loop.
pub(crate) fn start_link(
    out: &mut impl Write,
    ops: &mut Ops,
    op: &str,
    paths: Vec<String>,
    dest: &str,
    collide: super::collide::Ask,
) {
    start_link_held(out, ops, op, paths, dest, collide, None);
}
pub(crate) fn start_link_held(out: &mut impl Write, ops: &mut Ops, op: &str, paths: Vec<String>, dest: &str,
    collide: super::collide::Ask, held: Option<Vec<super::linkheld::Identity>>) {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "link");
        return;
    }
    let dest_path = match usable_dest(dest) {
        Ok(d) => d,
        Err(mut e) => {
            e.where_ = "link".to_string();
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            return;
        }
    };
    // A source or target on a pending mount waits, before the question is taken.
    let sources: Vec<PathBuf> = paths.iter().map(flea::bytename::exact).collect();
    let target = dest_path.clone();
    if pending_busy_for(out, ops, "link", &sources, &[target]) {
        return;
    }
    let policy = collide.policy(ops.question.take(), &dest_path).for_batch(&paths);
    ops.claim_transfer();
    let tx = ops.tx.clone();
    let owned_op = op.to_string();
    let owned_dest = dest.to_string();
    thread::spawn(move || {
        let done = run_link_held(&owned_op, paths, dest_path, policy, held.as_deref());
        let entry = Entry { op: "link".to_string(), steps: done.steps };
        let _ = tx.send(OpMsg::Linked { ok: done.ok, failed: done.failed, skipped: done.skipped,
            entry, note: done.note, first_err: done.first_err, dest: owned_dest });
    });
}

// A path-keyed handshake holds the real link worker without delaying unrelated tests.
#[cfg(test)]
struct HeldLink {
    dest: PathBuf,
    entered: Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
#[cfg(test)]
static HELD_LINK: std::sync::Mutex<Option<HeldLink>> = std::sync::Mutex::new(None);
#[cfg(test)]
pub(crate) fn test_hold_link(dest: &Path) -> (Sender<()>, std::sync::mpsc::Receiver<()>) {
    let (release, wait) = std::sync::mpsc::channel();
    let (entered, started) = std::sync::mpsc::channel();
    *HELD_LINK.lock().unwrap() = Some(HeldLink { dest: dest.to_path_buf(), entered, release: wait });
    (release, started)
}

// One exclusive link per source, replacing through the trash when the card chose it.
#[cfg(test)]
pub(crate) fn run_link(op: &str, paths: Vec<String>, dest: PathBuf, policy: super::collide::Policy) -> LinkOutcome {
    run_link_held(op, paths, dest, policy, None)
}
fn run_link_held(op: &str, paths: Vec<String>, dest: PathBuf, policy: super::collide::Policy,
    held: Option<&[super::linkheld::Identity]>) -> LinkOutcome {
    #[cfg(test)]
    {
        let held = {
            let mut hook = HELD_LINK.lock().unwrap();
            if hook.as_ref().is_some_and(|held| held.dest == dest) { hook.take() } else { None }
        };
        if let Some(held) = held {
            let _ = held.entered.send(());
            let _ = held.release.recv();
        }
    }
    let kind = match op {
        "absolute" => super::link::LinkKind::Absolute,
        "hard" => super::link::LinkKind::Hard,
        _ => super::link::LinkKind::Relative,
    };
    let mut steps: Vec<Step> = Vec::new();
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut first_err = String::new();
    // Each stranded replace sentence, and a mixed batch's first failure, reported on the linked line.
    let mut note = String::new();
    for source in &paths {
        let src = flea::bytename::view(source); let src = &*src;
        let shown = flea::bytename::display(source);
        if !super::linkheld::matches(held, src) || src.symlink_metadata().is_err() {
            failed += 1;
            if first_err.is_empty() { first_err = format!("the link source {shown} changed or no longer exists"); }
            continue;
        }
        let Some(dst) = super::link::dest_path(&dest, src) else {
            failed += 1;
            if first_err.is_empty() { first_err = format!("{shown} has no file name"); }
            continue;
        };
        let here = src.parent() == Some(dest.as_path());
        match policy.place(src, dst.clone(), here, false) {
            super::collide::Place::Skip => { skipped += 1; }
            super::collide::Place::Refuse(msg) => {
                failed += 1;
                if first_err.is_empty() { first_err = format!("{shown}: {msg}"); }
            }
            super::collide::Place::Land { to, replace } => {
                let mut replaced: Vec<super::trash::Entry> = Vec::new();
                if replace {
                    match super::trash::trash(std::slice::from_ref(&to)) {
                        (trashed, 0, _) => replaced = trashed,
                        _ => {
                            failed += 1;
                            if first_err.is_empty() { first_err = format!("{shown}: {}", super::collide::TRASH_REFUSED); }
                            continue;
                        }
                    }
                }
                let made = if !super::linkheld::matches(held, src) {
                    Err(op_err("link", source, "the held link source changed before creation"))
                } else { match &kind {
                    super::link::LinkKind::Absolute => super::link::create_absolute(src, &to),
                    super::link::LinkKind::Hard => super::link::create_hard(src, &to),
                    super::link::LinkKind::Relative => super::link::create_relative(src, &to),
                }};
                match made {
                    Ok(()) => {
                        match inspect_made_link(&to) {
                            Ok(identity) => {
                                for entry in replaced {
                                    steps.push(Step::Trashed(entry));
                                }
                                steps.push(Step::Linked { path: to, identity,
                                    source: src.to_path_buf(), kind: kind.clone() });
                                ok += 1;
                            }
                            Err(e) => {
                                // A failed cleanup leaves the link behind, so its result is reported, never ignored.
                                if let Err(remove) = remove_made_link(&to) {
                                    // The link holds their name, so restoring them can only fail and they stay in the Trash.
                                    let sentence = format!("{}; the link at {} could not be removed: {}", e.msg, to.to_string_lossy(), crate::error::io_message(&remove));
                                    // The sentence lives in note, so a landed batch still reports it.
                                    let sentence = if replaced.is_empty() { sentence } else { super::collide::kept_in_trash(&sentence) };
                                    if note.is_empty() {
                                        note = sentence;
                                    } else {
                                        note.push_str("; ");
                                        note.push_str(&sentence);
                                    }
                                    failed += 1;
                                    continue;
                                }
                                for entry in replaced {
                                    if to.symlink_metadata().is_err()
                                        && super::trash::restore(&entry).is_ok() {
                                        continue;
                                    }
                                    steps.push(Step::Trashed(entry));
                                }
                                failed += 1;
                                if first_err.is_empty() { first_err = format!("{shown}: {}", e.msg); }
                            }
                        }
                    }
                    Err(e) => {
                        // Put the trashed item back when the link left its name free.
                        for entry in replaced {
                            if to.symlink_metadata().is_err()
                                && super::trash::restore(&entry).is_ok() {
                                continue;
                            }
                            steps.push(Step::Trashed(entry));
                        }
                        failed += 1;
                        if first_err.is_empty() { first_err = format!("{shown}: {}", e.msg); }
                    }
                }
            }
        }
    }
    LinkOutcome { ok, failed, skipped, steps, first_err, note }
}

// Tests drive the worker directly, because the fault hooks it reads are thread-local.
#[cfg(test)]
pub(crate) fn test_reported_link(o: &mut Ops, buf: &mut Vec<u8>, op: &str, paths: Vec<String>, dest: PathBuf, policy: super::collide::Policy) {
    let done = run_link(op, paths, dest.clone(), policy);
    let entry = Entry { op: "link".to_string(), steps: done.steps };
    report_op(buf, o, OpMsg::Linked { ok: done.ok, failed: done.failed, skipped: done.skipped,
        entry, note: done.note, first_err: done.first_err, dest: dest.exact_text().to_string() });
}

// Show original answers beside the loop, because read_link and canonicalize can stall on a dead mount.
pub(crate) fn start_link_target(ops: &Ops, path: &str, id: usize) {
    let owned = path.to_string();
    let tx = ops.tx.clone();
    thread::spawn(move || {
        let mut buf = Vec::new();
        do_link_target(&mut buf, &owned, id);
        let line = String::from_utf8_lossy(&buf).trim_end_matches('\n').to_string();
        let _ = tx.send(OpMsg::Meta { line });
    });
}

// Show original: reveal the symlink target in its own folder, never resolve a non-link.
pub(crate) fn do_link_target(out: &mut impl Write, path: &str, id: usize) {
    do_link_target_held(out, path, id, None);
}
pub(crate) fn do_link_target_held(out: &mut impl Write, path: &str, id: usize, expected: Option<&str>) {
    let target = flea::bytename::view(path); let target = &*target;
    let text = match std::fs::read_link(target) {
        Ok(t) => t,
        Err(e) => {
            writeln!(out, "{}", error_line(&op_err("linktarget", path, &crate::error::io_message(&e)))).ok();
            out.flush().ok();
            return;
        }
    };
    if expected.is_some_and(|expected| text.as_os_str() != expected) {
        writeln!(out, "{}", error_line(&op_err("linktarget", path, "the selected link changed before Follow could run"))).ok();
        out.flush().ok();
        return;
    }
    let absolute = if text.is_absolute() {
        text.clone()
    } else {
        target.parent().unwrap_or(Path::new("/")).join(&text)
    };
    let (directory, name) = link_target_parts(&absolute);
    writeln!(out, "{}", super::proto::linktarget_line(path, &directory, &name, id)).ok();
    out.flush().ok();
}

// A trailing .. canonicalizes whole, any other target only its folder, as create_relative splits it.
fn link_target_parts(absolute: &Path) -> (String, String) {
    let lexical = || {
        let directory = absolute.parent().unwrap_or(Path::new("/")).exact_text().to_string();
        let name = absolute.file_name().map(|n| n.exact_text().to_string()).unwrap_or_default();
        (directory, name)
    };
    if matches!(absolute.components().next_back(), Some(Component::ParentDir)) {
        let Ok(resolved) = absolute.canonicalize() else { return lexical() };
        return (
            resolved.parent().unwrap_or(Path::new("/")).exact_text().to_string(),
            resolved.file_name().map(|n| n.exact_text().to_string()).unwrap_or_default(),
        );
    }
    let Some(folder) = absolute.parent() else { return lexical() };
    let Ok(folder) = folder.canonicalize() else { return lexical() };
    let name = absolute.file_name().map(|n| n.exact_text().to_string()).unwrap_or_default();
    (folder.exact_text().to_string(), name)
}

// Permissions batch: one entry holds every path Apply changed, so one undo restores all.
pub(crate) fn do_permissions_batch(out: &mut impl Write, ops: &mut Ops, paths: Vec<String>, modes: Vec<String>, id: usize) {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        writeln!(out, "{}", super::proto::permissions_batch_line(id, false, "", "an operation is already running")).ok();
        out.flush().ok();
        return;
    }
    if paths.len() != modes.len() {
        writeln!(out, "{}", super::proto::permissions_batch_line(id, false, "", "every selected item needs its own target mode")).ok();
        out.flush().ok();
        return;
    }
    let items: Vec<(PathBuf, String)> = paths.into_iter().zip(modes).map(|(p, m)| (flea::bytename::exact(p), m)).collect();
    // The reply names the first target mode.
    let shown = items.first().map(|(_, m)| m.clone()).unwrap_or_default();
    // A target on a pending mount waits; writes elsewhere run beside the held write.
    {
        let sources: Vec<PathBuf> = items.iter().map(|(p, _)| p.clone()).collect();
        if pending_on_any(ops, &sources, &[]) {
            writeln!(out, "{}", super::proto::permissions_batch_line(id, false, &shown, "an operation is already running")).ok();
            out.flush().ok();
            return;
        }
    }
    match super::permissions::apply_many(&items) {
        Ok(steps) => {
            ops.journal.push(Entry { op: "permissions".to_string(), steps });
            writeln!(out, "{}", super::proto::permissions_batch_line(id, true, &shown, "")).ok();
        }
        Err(failed) => {
            // A failed rollback journals what stayed applied, else nothing.
            if !failed.applied.is_empty() {
                ops.journal.push(Entry { op: "permissions".to_string(), steps: failed.applied });
            }
            writeln!(out, "{}", super::proto::permissions_batch_line(id, false, &shown, &failed.msg)).ok();
        }
    }
    out.flush().ok();
}

pub(crate) fn do_newfile(out: &mut impl Write, ops: &mut Ops, parent: &str, name: &str, id: usize) -> Vec<PathBuf> {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        let request = format!(r#"{{"op":"newFile","id":{}}}"#, id);
        writeln!(out, "{}", super::menu_actions::response(&request, Err("An operation is already running.".into()))).ok();
        out.flush().ok();
        return Vec::new();
    }
    // A parent on a pending mount waits; writes elsewhere run beside the held write.
    if pending_on_any(ops, &[flea::bytename::exact(parent)], &[]) {
        let request = format!(r#"{{"op":"newFile","id":{}}}"#, id);
        writeln!(out, "{}", super::menu_actions::response(&request, Err("An operation is already running.".into()))).ok();
        out.flush().ok();
        return Vec::new();
    }
    let created = super::menu_actions::create_file(&flea::bytename::view(parent), name);
    let touched = created.as_ref().map(|(path, _)| vec![path.clone()]).unwrap_or_default();
    let result = created.map(|(path, identity)| {
        ops.journal.push(Entry { op: "newfile".into(), steps: vec![super::undo::Step::MadeFile { path: path.clone(), identity }] });
        format!(r#""path":"{}""#, crate::json::escape(&path.exact_text()))
    });
    let request = format!(r#"{{"op":"newFile","id":{}}}"#, id);
    writeln!(out, "{}", super::menu_actions::response(&request, result)).ok();
    out.flush().ok();
    touched
}

pub(crate) fn start_redo(out: &mut impl Write, ops: &mut Ops) {
    if ops.live.running().is_some() || ops.jobs.unfinished() > 0 {
        busy(out, "redo");
        return;
    }
    if pending_busy(out, ops, "redo") {
        return;
    }
    let (op, n) = match ops.journal.redo_info() {
        Ok(info) => info,
        Err(error) => {
            writeln!(out, "{}", error_line(&error)).ok();
            out.flush().ok();
            return;
        }
    };
    let (id, cancel) = ops.claim_transfer();
    let mut journal = std::mem::replace(&mut *ops.journal, Journal::new());
    writeln!(out, r#"{{"t":"redostarted","id":{},"n":{},"op":"{}"}}"#, id, n, crate::json::escape(&op)).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || {
        let result = journal.redo(id, &cancel, &tx);
        // The worker owns this journal alone, so the take cannot race the loop.
        let touched = journal.take_last_touched();
        let _ = tx.send(OpMsg::RedoDone {
            journal: Box::new(journal),
            result,
            touched,
        });
    });
}

// Every message an operation thread sends, written out and, when terminal, recorded in the journal.
pub(crate) fn report_op(out: &mut impl Write, ops: &mut Ops, mut msg: OpMsg) {
    // A publisher's own return never restarts it, so a held lock costs one bounded attempt per later event.
    let returned = matches!(msg, OpMsg::JournalDone { .. });
    if let OpMsg::TransferDone {
        id, entry, failed, ..
    }
    | OpMsg::MergeDone {
        id, entry, failed, ..
    } = &mut msg
    {
        let owned = std::mem::replace(
            entry,
            Entry {
                op: String::new(),
                steps: Vec::new(),
            },
        );
        if let Some(job) = ops.jobs.rows.iter_mut().find(|job| job.id == *id) {
            job.undo_key = if *failed == 0 {
                ops.journal.push_for_job(owned)
            } else {
                ops.journal.push(owned);
                None
            };
        } else {
            ops.journal.push(owned);
        }
    }
    super::jobevents::observe(out, ops, &msg);
    match msg {
        OpMsg::CreationReceipt { request, inner } => {
            created_request::report(out, ops, request, *inner);
            return;
        }
        OpMsg::SendStatus { .. }
        | OpMsg::JobEntries { .. }
        | OpMsg::JobReady { .. }
        | OpMsg::JobSample { .. } => {}
        OpMsg::JobDone { id, line, .. } => {
            ops.live.finished_if(id);
            if !line.is_empty() {
                writeln!(out, "{}", line).ok();
            }
        }
        OpMsg::ActionReady { request, result } => super::actions::ready(out, ops, request, result),
        OpMsg::MenuDeleteDone { line } => {
            ops.live.finished();
            writeln!(out, "{}", line).ok();
        }
        #[cfg(test)]
        OpMsg::SlotDone { line } => {
            if let Some(id) = crate::json::field_usize(&line, "id") {
                ops.live.finished_if(id);
            } else {
                ops.live.finished();
            }
            writeln!(out, "{}", line).ok();
        }
        OpMsg::Progress { id, index, name, bytes, total, scanned } => {
            writeln!(out, "{}", transferprogress_line(id, index, &name, bytes, total, scanned)).ok();
        }
        OpMsg::Item { id, index, name, ok, err } => {
            writeln!(out, "{}", transferitem_line(id, index, &name, ok, &err)).ok();
        }
        OpMsg::TransferDone {
            id,
            ok,
            failed,
            skipped,
            cancelled,
            retry,
            durable,
            note,
            refused,
            ..
        } => {
            ops.live.finished_if(id);
            ops.transfer_retry = (id, retry);
            writeln!(out, "{}", transferdone_line(id, ok, failed, skipped, cancelled, &ops.transfer_retry.1, durable, &note, &refused)).ok();
        }
        done @ OpMsg::MergeDone { .. } => {
            let line = super::merge::transferdone_merge_line(&done);
            let OpMsg::MergeDone { id, retry, .. } = done else {
                return;
            };
            ops.live.finished_if(id);
            ops.transfer_retry = (id, retry);
            writeln!(out, "{}", line).ok();
        }
        landed @ (OpMsg::TrashAnnounced { .. } | OpMsg::Trashed { .. }) => super::trashland::land(out, ops, landed),
        OpMsg::Linked { ok, failed, skipped, entry, note, first_err, dest } => {
            ops.journal.push(entry);
            ops.live.finished();
            if (failed == 0 && first_err.is_empty()) || ok > 0 || skipped > 0 {
                // A mixed batch reports its first failure here, never silently, ahead of any stranded sentence.
                let note = if first_err.is_empty() { note } else if note.is_empty() { first_err } else { format!("{first_err}; {note}") };
                writeln!(out, "{}", super::proto::linked_line(ok, failed, skipped, &note)).ok();
            } else {
                // An earlier failure never hides a stranded replace, so the note rides along.
                let msg = if first_err.is_empty() { note } else if note.is_empty() { first_err } else { format!("{first_err}; {note}") };
                writeln!(out, "{}", error_line(&op_err("link", &dest, &msg))).ok();
            }
        }
        OpMsg::Asked { turn, question, line } => if super::collide::landed(ops, turn, question) { writeln!(out, "{}", line).ok(); },
        OpMsg::Pasted { line, entry } => super::pasteimage::land(out, ops, line, entry),
        // Rename execution landed; its ordered journal publication also belongs to a worker.
        OpMsg::RenameDone { id, result } => {
            if ops.pending.iter().any(|held| held.id == id) {
                super::renamework::publish(ops, super::renamework::Publication::Rename(id, result));
            }
        }
        OpMsg::RenameSlow { id, line } => {
            if ops.pending.iter().any(|held| held.id == id) { writeln!(out, "{}", line).ok(); }
        }
        OpMsg::JournalDone { journal, renames } => {
            super::renamework::published(out, ops, *journal, renames)
        }
        OpMsg::MkdirDone { id, result } => {
            land_mkdir(out, ops, result);
            forget_pending(ops, id);
            ops.live.finished_if(id);
        }
        OpMsg::BulkRenameDone { id, result } => {
            super::bulkrename::land_bulkrename(out, ops, result);
            forget_pending(ops, id);
            ops.live.finished_if(id);
        }
        OpMsg::SetTagsDone { id, result } => {
            super::tagsreq::land(out, ops, result);
            forget_pending(ops, id);
            ops.live.finished_if(id);
        }
        // A convert journals what it published, so z removes those outputs and never an older operation.
        OpMsg::Converted { id, line, steps } => {
            if !steps.is_empty() {
                ops.journal.push(Entry {
                    op: "convert".to_string(),
                    steps,
                });
            }
            writeln!(out, "{}", line).ok();
            ops.detached.remove(id);
        }
        // Meta never claims the operation slot, so it does not clear it either.
        OpMsg::Meta { line } => {
            writeln!(out, "{}", line).ok();
        }
        // The job leaves the quit registry here, so a drain that sees it empty has already written every line.
        OpMsg::DetachedDone { id, line } => { writeln!(out, "{}", line).ok(); ops.detached.remove(id); }
        OpMsg::Duplicated { ok, path, err, entry } => {
            ops.journal.push(entry);
            ops.live.finished();
            if ok {
                writeln!(out, "{}", duplicated_line(true, &path)).ok();
            } else {
                writeln!(out, "{}", error_line(&op_err("duplicate", "", &err))).ok();
            }
        }
        OpMsg::RedoDone { journal, result, .. } => {
            ops.journal = (*journal).into();
            ops.live.finished();
            match result {
                Ok(op) => writeln!(out, r#"{{"t":"redone","op":"{}","ok":true}}"#, crate::json::escape(&op)).ok(),
                Err(error) => writeln!(out, "{}", error_line(&error)).ok(),
            };
        }
    }
    if !returned {
        super::renamework::flush_pending(ops);
    }
    ops.jobs.refresh_undo(out, &ops.journal);
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use crate::backend::testdir::TestDir;
    use crate::backend::undo::Step;
    use std::sync::mpsc::channel;

    fn do_rename(out: &mut Vec<u8>, ops: &mut Ops, path: &str, to: &str) -> Vec<PathBuf> {
        super::super::renamework::test_request(out, ops, |out, ops| super::do_rename(out, ops, path, to))
    }
    fn do_menu_rename(out: &mut Vec<u8>, ops: &mut Ops, path: &str, to: &str, id: usize) -> Vec<PathBuf> {
        super::super::renamework::test_request(out, ops, |out, ops| super::do_menu_rename(out, ops, path, to, id))
    }
    fn do_rename_with<F>(out: &mut Vec<u8>, ops: &mut Ops, path: &str, deadline: Duration, work: F) -> Vec<PathBuf>
    where F: FnOnce() -> Result<(PathBuf, Vec<Step>), FleaError> + Send + 'static {
        super::super::renamework::test_request(out, ops, |out, ops| super::do_rename_with(out, ops, path, deadline, work))
    }
    fn report_op(out: &mut Vec<u8>, ops: &mut Ops, message: OpMsg) {
        super::super::renamework::test_request(out, ops, |out, ops| { super::report_op(out, ops, message); Vec::new() });
    }

    fn ops() -> Ops {
        let (tx, _rx) = channel();
        Ops::new(tx)
    }

    fn ops_link() -> (Ops, std::sync::mpsc::Receiver<OpMsg>) {
        let (tx, rx) = channel();
        (Ops::new(tx), rx)
    }

    // A link answers beside the loop, so its line arrives through its message like every spawned operation.
    fn link_reported(buf: &mut Vec<u8>, o: &mut Ops, rx: &std::sync::mpsc::Receiver<OpMsg>) {
        let msg = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(matches!(&msg, OpMsg::Linked { .. }), "link answered with another message");
        report_op(buf, o, msg);
    }

    fn out() -> Vec<u8> {
        Vec::new()
    }

    fn text(buf: &[u8]) -> String {
        String::from_utf8_lossy(buf).to_string()
    }

    #[test]
    fn each_operation_claims_a_new_id_and_its_own_cancel_flag() {
        let mut o = ops();
        let (first, first_flag) = o.claim_transfer();
        o.live.finished();
        let (second, second_flag) = o.claim_transfer();
        assert_eq!((first, second), (1, 2));
        first_flag.store(true, Ordering::Relaxed);
        assert!(
            !second_flag.load(Ordering::Relaxed),
            "a cancel aimed at the first operation must never reach the one after it"
        );
    }

    #[test]
    fn a_cancel_for_an_operation_that_is_not_running_does_nothing() {
        let mut o = ops();
        let (id, flag) = o.claim_transfer();
        let mut buf = out();
        cancel_transfer(&mut buf, &mut o, id + 99);
        assert!(!flag.load(Ordering::Relaxed), "a stale id must not cancel the live operation");
        cancel_transfer(&mut buf, &mut o, id);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn permanent_delete_owns_the_mutation_slot_and_releases_it_on_refusal() {
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        o.claim_transfer();
        request_menu_action(&mut buf, &mut o, r#"{"op":"delete","id":5,"token":1}"#.into(), vec![], None);
        assert!(text(&buf).contains(r#""op":"delete","ok":false"#));
        assert!(text(&buf).contains("already running"));
        assert!(o.menuactions.is_none(), "busy refusal must not start a competing service");
        o.live.finished();
        buf.clear();
        request_menu_action(&mut buf, &mut o, r#"{"op":"delete","id":5,"token":1}"#.into(), vec![], None);
        assert!(o.live.running().is_some());
        let message = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(matches!(&message, OpMsg::MenuDeleteDone { .. }));
        report_op(&mut buf, &mut o, message);
        assert!(o.live.running().is_none());
        assert!(text(&buf).contains(crate::backend::menu_actions::SELECTION_CHANGED));
    }

    #[test]
    fn menu_rename_checks_only_the_requested_captured_identity() {
        let d = TestDir::new("menu-rename");
        let first = d.file("first", "first");
        let second = d.file("second", "second");
        let foreign = d.file("foreign", "foreign");
        let (tx, rx) = channel();
        let mut o = Ops::new(tx.clone());
        let menu = super::super::menu_actions::MenuActions::new(tx);
        menu.request(r#"{"op":"snapshot","id":5}"#.into(), vec![first.to_string_lossy().into(), second.to_string_lossy().into()], None);
        let OpMsg::Meta { line } = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() else { panic!("snapshot reply"); };
        assert!(line.contains(r#""ok":true"#));
        o.menuactions = Some(menu);
        for path in [&first, &second, &foreign] { assert!(path.is_absolute() && path.starts_with(d.path())); }
        let mut buf = out();
        do_menu_rename(&mut buf, &mut o, &first.to_string_lossy(), "first-renamed", 5);
        assert!(text(&buf).contains(r#""t":"renamed","ok":true"#));
        buf.clear();
        do_menu_rename(&mut buf, &mut o, &second.to_string_lossy(), "second-renamed", 5);
        assert!(text(&buf).contains(r#""t":"renamed","ok":true"#), "a prior successful rename must not invalidate the next captured item");
        assert_eq!(o.journal.len(), 2);
        buf.clear();
        do_menu_rename(&mut buf, &mut o, &foreign.to_string_lossy(), "wrong", 5);
        assert!(text(&buf).contains("That item is no longer selected"));
        d.file("first", "replacement");
        buf.clear();
        do_menu_rename(&mut buf, &mut o, &first.to_string_lossy(), "wrong", 5);
        assert!(text(&buf).contains("Selected item changed"));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "replacement");
        assert_eq!(std::fs::read_to_string(&foreign).unwrap(), "foreign");
        buf.clear();
        do_menu_rename(&mut buf, &mut o, &first.to_string_lossy(), "wrong", 4);
        assert!(text(&buf).contains(crate::backend::menu_actions::SELECTION_CHANGED));
        assert!(!d.join("wrong").exists());
    }

    #[test]
    fn a_rename_records_its_reversal_and_undo_puts_the_name_back() {
        let d = TestDir::new("dispatchrename");
        let mut o = ops();
        let from = d.file("before.txt", "body");
        let mut buf = out();
        do_rename(&mut buf, &mut o, &from.to_string_lossy(), "after.txt");
        assert!(d.join("after.txt").exists());
        assert!(text(&buf).contains(r#""t":"renamed","ok":true"#));
        assert_eq!(o.journal.len(), 1);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#"{"t":"undone","op":"rename","ok":true}"#));
        assert!(from.exists(), "undo put the old name back");
        assert!(o.journal.is_empty());
    }

    // Journal::undo propagates with `?` and do_undo hands that error straight to error_line.
    #[test]
    fn a_failed_rename_reversal_answers_the_rename_kind_and_not_undo() {
        let d = TestDir::new("dispatchundorename");
        let mut o = ops();
        let from = d.file("before.txt", "body");
        let mut buf = out();
        do_rename(&mut buf, &mut o, &from.to_string_lossy(), "after.txt");
        // Something takes the old name back before the undo, so the reversal's own rename refuses it.
        d.file("before.txt", "squatter");
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        let line = text(&buf);
        assert!(line.contains(r#""t":"error","where":"rename""#), "{}", line);
        assert!(!line.contains(r#""where":"undo""#), "an undo must not re-stamp the kind its step answered");
    }

    #[test]
    fn a_refused_rename_records_nothing_to_undo() {
        let d = TestDir::new("dispatchrefuse");
        let mut o = ops();
        let from = d.file("a.txt", "a");
        d.file("b.txt", "b");
        let mut buf = out();
        do_rename(&mut buf, &mut o, &from.to_string_lossy(), "b.txt");
        assert!(text(&buf).contains(r#""t":"error","where":"rename""#), "the refusal is an error line, not a silent no-op");
        assert!(o.journal.is_empty(), "a rename that did not happen must not be undoable");
        assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "b");
    }

    #[test]
    fn explicit_paths_win_and_a_rows_form_resolves_against_the_listing() {
        let mut l = Listing::new();
        l.push("sub", true);
        l.push("a.txt", false);
        l.push("b.txt", false);
        let base = Path::new("/home/gm");
        // A wide selection reaches the backend as indices, because the client only holds its own window.
        assert_eq!(
            resolve_rows(Vec::new(), &[1, 2], base, &l),
            vec!["/home/gm/a.txt".to_string(), "/home/gm/b.txt".to_string()]
        );
        // A row past the end is dropped rather than panicking or naming the base directory itself.
        assert_eq!(resolve_rows(Vec::new(), &[99], base, &l), Vec::<String>::new());
        // Explicit paths are never second-guessed against the listing.
        assert_eq!(
            resolve_rows(vec!["/elsewhere/c.txt".to_string()], &[0, 1, 2], base, &l),
            vec!["/elsewhere/c.txt".to_string()]
        );
        assert_eq!(resolve_rows(Vec::new(), &[], base, &l), Vec::<String>::new());
    }

    #[test]
    fn a_second_operation_while_one_runs_is_refused_as_data_rather_than_queued_invisibly() {
        let d = TestDir::new("dispatchbusy");
        let mut o = ops();
        o.claim_transfer();
        let mut buf = out();
        start_trash(&mut buf, &mut o, vec![d.file("a.txt", "a").to_string_lossy().to_string()], 0, 0);
        assert!(text(&buf).contains("an operation is already running"));
        assert!(d.join("a.txt").exists(), "the refused operation touched nothing");
    }

    #[test]
    fn a_terminal_message_clears_the_running_slot_so_the_next_operation_is_accepted() {
        let mut o = ops();
        o.claim_transfer();
        assert!(o.live.running().is_some());
        let mut buf = out();
        report_op(
            &mut buf,
            &mut o,
            OpMsg::Trashed { id: 1, request: 0, ok: 1, failed: 0, entry: Entry { op: "trash".to_string(), steps: vec![Step::Created { path: "/x".into() }] }, reason: String::new(), announced: false },
        );
        assert!(o.live.running().is_none(), "the cap would otherwise refuse every operation for the rest of the session");
        assert_eq!(o.journal.len(), 1);
        assert_eq!(text(&buf).trim(), r#"{"t":"trashed","id":0,"ok":1,"failed":0,"moved":[]}"#);
    }

    #[test]
    fn a_new_folder_records_its_reversal_and_undo_removes_it() {
        let d = TestDir::new("dispatchmkdir");
        let mut o = ops();
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &d.path().to_string_lossy(), "photos", None);
        assert!(d.join("photos").is_dir());
        assert_eq!(text(&buf).trim(), format!(r#"{{"t":"made","ok":true,"path":"{}"}}"#, d.join("photos").display()));
        assert_eq!(o.journal.len(), 1);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#"{"t":"undone","op":"mkdir","ok":true}"#));
        assert!(!d.join("photos").exists(), "undo removed the folder it made");
        assert!(o.journal.is_empty());
    }

    #[test]
    fn a_refused_new_folder_records_nothing_to_undo() {
        let d = TestDir::new("dispatchmkdirrefuse");
        let mut o = ops();
        d.file("taken", "t");
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &d.path().to_string_lossy(), "taken", None);
        assert!(text(&buf).contains(r#""t":"error","where":"mkdir""#), "the refusal is an error line, not a silent no-op");
        assert!(o.journal.is_empty(), "a folder that was not made must not be undoable");
        assert_eq!(std::fs::read_to_string(d.join("taken")).unwrap(), "t");
    }

    #[test]
    fn link_runs_beside_the_loop_and_claims_the_slot() {
        let d = TestDir::new("dispatchlinkasync");
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        assert!(buf.is_empty(), "link answered on the calling thread: {}", text(&buf));
        assert!(o.live.running().is_some(), "link holds no slot while it runs");
        let msg = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(matches!(&msg, OpMsg::Linked { .. }), "link answered with another message");
        report_op(&mut buf, &mut o, msg);
        let line = text(&buf);
        assert!(line.contains(r#""t":"linked""#) && line.contains(r#""ok":1"#), "{}", line);
        assert!(o.live.running().is_none(), "the slot stayed claimed after the answer");
        assert_eq!(o.journal.len(), 1);
    }

    #[test]
    fn paste_as_links_answers_one_line_and_undo_removes_them() {
        let d = TestDir::new("dispatchlink");
        let (mut o, rx) = ops_link();
        let src = d.dir("src");
        let a = d.file("src/a.txt", "a");
        let b = d.file("src/b.txt", "b");
        let dest = d.dir("dest");
        let _ = src;
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()],
            &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert_eq!(text(&buf).trim(), r#"{"t":"linked","ok":2,"failed":0,"skipped":0}"#);
        assert_eq!(o.journal.len(), 1);
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "a");
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#"{"t":"undone","op":"link","ok":true}"#));
        assert!(std::fs::symlink_metadata(dest.join("a.txt")).is_err());
        assert!(std::fs::symlink_metadata(dest.join("b.txt")).is_err());
        assert!(a.exists() && b.exists(), "undo removes the links, never the sources");
    }

    #[test]
    fn an_existing_name_is_refused_and_journals_nothing() {
        let d = TestDir::new("dispatchlinkrefuse");
        let (mut o, rx) = ops_link();
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        d.file("dest/a.txt", "someone else");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert!(text(&buf).contains(r#""t":"error","where":"link""#) && text(&buf).contains(&a.to_string_lossy().into_owned()));
        assert!(o.journal.is_empty(), "a link that was not made must not be undoable");
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "someone else");
    }

    #[test]
    fn a_mixed_hard_batch_names_its_first_refusal_on_the_linked_line() {
        let d = TestDir::new("dispatchlinkmixed");
        let (mut o, rx) = ops_link();
        let a = d.file("a.txt", "a");
        let sub = d.dir("sub");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "hard",
            vec![a.to_string_lossy().to_string(), sub.to_string_lossy().to_string()],
            &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        let line = text(&buf);
        assert!(line.contains(r#""ok":1"#) && line.contains(r#""failed":1"#), "{}", line);
        assert!(line.contains("a hard link cannot point to a folder"), "mixed failure silent: {}", line);
        assert!(line.contains(&sub.to_string_lossy().into_owned()), "mixed failure unnamed: {}", line);
        assert_eq!(o.journal.len(), 1);
    }

    #[test]
    fn a_hard_link_to_a_directory_is_refused() {
        let d = TestDir::new("dispatchharddir");
        let (mut o, rx) = ops_link();
        let src = d.dir("src");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "hard", vec![src.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        let line = text(&buf);
        assert!(line.contains("a hard link cannot point to a folder"), "{}", line);
        assert!(line.contains(r#""where":"link""#), "{}", line);
        assert!(std::fs::symlink_metadata(dest.join("src")).is_err());
        assert!(o.journal.is_empty());
    }

    #[test]
    fn an_unverifiable_link_whose_cleanup_fails_names_the_leftover() {
        let d = TestDir::new("dispatchlinkleftover");
        let mut o = ops();
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        // The fault hook is thread-local, so this drives the worker directly and reports its answer.
        test_fail_link_verify(true);
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        let paths = vec![a.to_string_lossy().to_string()];
        let policy = ask.policy(None, &dest).for_batch(&paths);
        test_reported_link(&mut o, &mut buf, "relative", paths, dest.clone(), policy);
        test_fail_link_verify(false);
        let line = text(&buf);
        let at = dest.join("a.txt");
        assert!(line.contains(&at.to_string_lossy().into_owned()), "leftover unnamed: {}", line);
        assert!(line.contains(&format!("the link at {} could not be removed: ", crate::json::escape(&at.to_string_lossy()))), "cleanup failure silent: {}", line);
        assert!(at.symlink_metadata().is_ok(), "the leftover stays and is reported, never dropped");
        assert!(o.journal.is_empty(), "a link never identified journals nothing");
    }

    #[test]
    fn a_replace_whose_trash_is_refused_reports_that_and_replaces_nothing() {
        use crate::backend::collide::tests::{asked, chosen, refusing_trash};
        let d = TestDir::new("dispatchlinkreplace");
        let mut o = ops();
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        let there = d.file("dest/a.txt", "someone else");
        let _trash = refusing_trash();
        // The worker runs here, so the thread-local refusal is the one it reads.
        let question = asked(7, &[&a], &dest);
        let paths = vec![a.to_string_lossy().to_string()];
        let policy = chosen("replace", question, &dest).for_batch(&paths);
        let mut buf = out();
        test_reported_link(&mut o, &mut buf, "relative", paths, dest.clone(), policy);
        drop(_trash);
        let line = text(&buf);
        assert!(line.contains(r#""t":"error""#) && line.contains("could not be moved to Trash"), "{}", line);
        assert_eq!(std::fs::read_to_string(&there).unwrap(), "someone else");
        assert!(o.journal.is_empty(), "a refused replace journals nothing");
    }

    #[test]
    fn a_link_target_ending_in_dotdot_reveals_the_resolved_folder() {
        let d = TestDir::new("dispatchlinktargetdotdot");
        let b = d.dir("a/b");
        let up = b.join("up");
        std::os::unix::fs::symlink("..", &up).unwrap();
        let mut buf = out();
        do_link_target(&mut buf, &up.to_string_lossy(), 3);
        let line = text(&buf);
        // The link names its grandparent, so Show original reveals that folder with its leaf.
        assert!(line.contains(&format!("\"directory\":\"{}\"", d.path().display())), "{}", line);
        assert!(line.contains("\"name\":\"a\"") && line.contains("\"id\":3"), "{}", line);
        let c = d.dir("a/b/c");
        let sib = b.join("sib");
        std::os::unix::fs::symlink("../b/c", &sib).unwrap();
        let _ = c;
        let mut buf = out();
        do_link_target(&mut buf, &sib.to_string_lossy(), 5);
        let line = text(&buf);
        assert!(line.contains(&format!("\"directory\":\"{}\"", b.display())), "{}", line);
        assert!(line.contains("\"name\":\"c\"") && line.contains("\"id\":5"), "{}", line);
        assert!(!line.contains(".."), "a joined .. leaked onto the wire: {}", line);
    }

    #[test]
    fn link_target_answers_from_a_thread_as_meta() {
        let d = TestDir::new("dispatchlinktargetthread");
        let target = d.file("real.txt", "r");
        let link = d.join("l");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let (tx, rx) = channel();
        let o = Ops::new(tx);
        start_link_target(&o, &link.to_string_lossy(), 3);
        let OpMsg::Meta { line } = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() else { panic!("no meta line"); };
        assert!(line.contains(r#""t":"linktarget""#) && line.contains(r#""id":3"#), "{}", line);
        assert!(line.contains(&format!("\"directory\":\"{}\"", d.path().display())), "{}", line);
        assert!(line.contains("\"name\":\"real.txt\""), "{}", line);
    }

    #[test]
    fn a_link_into_an_unwritable_folder_answers_link() {
        use std::os::unix::fs::PermissionsExt;
        let d = TestDir::new("dispatchlinknowrite");
        let mut o = ops();
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o555)).unwrap();
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative", vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
        let line = text(&buf);
        assert!(line.contains(r#""where":"link""#), "{}", line);
        assert!(line.contains("that folder cannot be written"), "usable_dest refused, not the link: {}", line);
        assert!(o.journal.is_empty());
    }

    #[test]
    fn a_folder_put_at_a_link_name_survives_undo() {
        let d = TestDir::new("dispatchlinkkept");
        let (mut o, rx) = ops_link();
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert!(text(&buf).contains(r#""t":"linked""#));
        let at = dest.join("a.txt");
        std::fs::remove_file(&at).unwrap();
        let folder = d.dir("dest/a.txt");
        std::fs::write(folder.join("kept.txt"), "kept").unwrap();
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#""t":"error","where":"undo""#), "undo refuses a name that stopped being its link");
        assert_eq!(std::fs::read_to_string(folder.join("kept.txt")).unwrap(), "kept");
    }

    #[test]
    fn undone_links_redo_in_each_kind() {
        for op in ["relative", "absolute", "hard"] {
            let d = TestDir::new("dispatchlinkredo");
            let (mut o, rx) = ops_link();
            let a = d.file("a.txt", "a");
            let dest = d.dir("dest");
            let mut buf = out();
            let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
            start_link(&mut buf, &mut o, op,
                vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
            link_reported(&mut buf, &mut o, &rx);
            assert!(text(&buf).contains(r#""t":"linked""#), "link lands for {op}");
            let at = dest.join("a.txt");
            let mut buf = out();
            do_undo(&mut buf, &mut o);
            assert!(text(&buf).contains(r#""t":"undone""#), "undo removes the {op} link");
            assert!(std::fs::symlink_metadata(&at).is_err());
            let (tx, _rx) = channel();
            let redone = o.journal.redo(1, &AtomicBool::new(false), &tx);
            assert_eq!(redone.unwrap(), "link", "redo recreates the {op} link");
            if op == "hard" {
                use std::os::unix::fs::MetadataExt;
                assert!(!at.symlink_metadata().unwrap().file_type().is_symlink());
                assert_eq!(a.metadata().unwrap().ino(), at.metadata().unwrap().ino());
            } else {
                assert!(at.symlink_metadata().unwrap().file_type().is_symlink());
                assert_eq!(std::fs::read_to_string(&at).unwrap(), "a");
            }
            assert!(o.journal.redo_info().is_err(), "a redone entry is undoable, not redoable again");
            let mut buf = out();
            do_undo(&mut buf, &mut o);
            assert!(text(&buf).contains(r#""t":"undone""#));
            assert!(std::fs::symlink_metadata(&at).is_err());
        }
    }

    #[test]
    fn redo_refuses_a_link_name_taken_since() {
        let d = TestDir::new("dispatchlinkredotaken");
        let (mut o, rx) = ops_link();
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        std::fs::write(dest.join("a.txt"), "someone else").unwrap();
        let (tx, _rx) = channel();
        let redone = o.journal.redo(1, &AtomicBool::new(false), &tx);
        assert_eq!(redone.unwrap_err().msg, "an item with that name is already there, so redo left it in place");
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "someone else");
    }

    #[test]
    fn undoing_a_hard_link_keeps_the_last_name_when_its_source_is_gone() {
        let d = TestDir::new("link-hard-last");
        let (mut o, rx) = ops_link();
        d.dir("src");
        let a = d.file("src/a.txt", "precious");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "hard",
            vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert!(text(&buf).contains(r#""ok":1"#), "{}", text(&buf));
        std::fs::remove_file(&a).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "precious");
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(std::fs::read_to_string(dest.join("a.txt")).is_ok(),
            "undo removed the only remaining name; undo said {}", text(&buf));
        assert!(text(&buf).contains("last name"), "{}", text(&buf));
    }

    #[test]
    fn a_link_landing_during_a_redo_is_refused_busy_and_keeps_its_undo() {
        let d = TestDir::new("link-redo-busy");
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let parent = d.dir("p");
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &parent.to_string_lossy(), "made", None);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains("undone"), "{}", text(&buf));
        let mut buf = out();
        start_redo(&mut buf, &mut o);
        assert!(text(&buf).contains("redostarted"), "{}", text(&buf));
        let a = d.file("a.txt", "a");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![a.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        assert!(text(&buf).contains("already running"), "{}", text(&buf));
        assert!(!text(&buf).contains(r#""t":"linked""#), "{}", text(&buf));
        assert!(o.journal.is_empty(), "a refused link journals nothing");
        loop {
            let msg = rx.recv().unwrap();
            let done = matches!(msg, OpMsg::RedoDone { .. });
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if done { break; }
        }
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#""op":"mkdir""#), "{}", text(&buf));
    }

    #[test]
    fn a_hard_link_of_a_symlink_undoes() {
        let d = TestDir::new("link-hard-sym");
        let (mut o, rx) = ops_link();
        d.dir("src");
        let t = d.file("src/t.txt", "t");
        let l = d.join("src/l");
        std::os::unix::fs::symlink(&t, &l).unwrap();
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "hard",
            vec![l.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert!(text(&buf).contains(r#""t":"linked""#), "{}", text(&buf));
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains("undone"), "untouched link refused by undo: {}", text(&buf));
        assert!(std::fs::symlink_metadata(dest.join("l")).is_err());
        assert!(l.symlink_metadata().is_ok(), "undo removes the link, never the source");
    }

    #[test]
    fn a_hard_link_of_a_fifo_undoes_and_redoes() {
        let d = TestDir::new("link-hard-fifo");
        let (mut o, rx) = ops_link();
        let src = d.join("src.pipe");
        assert!(std::process::Command::new("mkfifo").arg(&src).status().unwrap().success());
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "hard", vec![src.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert!(text(&buf).contains(r#""t":"linked""#), "{}", text(&buf));
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains("undone"), "fifo link refused by undo: {}", text(&buf));
        assert!(std::fs::symlink_metadata(dest.join("src.pipe")).is_err());
        let (tx, _rx) = channel();
        assert_eq!(o.journal.redo(1, &AtomicBool::new(false), &tx).unwrap(), "link");
        assert!(dest.join("src.pipe").symlink_metadata().is_ok());
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains("undone"), "{}", text(&buf));
    }

    #[test]
    fn a_link_to_a_missing_source_is_refused_per_path() {
        let d = TestDir::new("link-missing");
        let (mut o, rx) = ops_link();
        let gone = d.join("gone.txt");
        let dest = d.dir("dest");
        let mut buf = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut buf, &mut o, "relative",
            vec![gone.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        link_reported(&mut buf, &mut o, &rx);
        assert!(!text(&buf).contains(r#""ok":1"#), "a link to nothing reported success: {}", text(&buf));
        assert!(o.journal.is_empty(), "a refused link journals nothing");
        assert!(std::fs::symlink_metadata(dest.join("gone.txt")).is_err());
    }

    #[test]
    fn a_permissions_batch_during_a_redo_is_refused_busy() {
        let d = TestDir::new("perm-busy");
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let parent = d.dir("p");
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &parent.to_string_lossy(), "made", None);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        let mut buf = out();
        start_redo(&mut buf, &mut o);
        assert!(text(&buf).contains("redostarted"), "{}", text(&buf));
        let a = d.file("a.txt", "a");
        let mut buf = out();
        do_permissions_batch(&mut buf, &mut o,
            vec![a.to_string_lossy().to_string()], vec!["600".to_string()], 7);
        let line = text(&buf);
        assert!(line.contains("already running"), "{}", line);
        assert!(line.contains(r#""ok":false"#), "{}", line);
        assert!(o.journal.is_empty(), "a refused chmod journals nothing");
        loop {
            let msg = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            let done = matches!(msg, OpMsg::RedoDone { .. });
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if done { break; }
        }
    }
    // A sandbox mapped onto a hung nfs mount, so the dispatch takes its slow path for it.
    fn remote_body(dir: &std::path::Path) -> String {
        format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", dir.to_string_lossy())
    }

    // Shorter than the production deadline, so the held pair below proves the shape without waiting it out.
    const SHORT_DEADLINE: std::time::Duration = std::time::Duration::from_millis(300);

    // Margin past a slow deadline, so scheduling noise never flakes the slow proof.
    const SLOW_MARGIN: std::time::Duration = std::time::Duration::from_secs(2);

    // Outer bound for one late Done to arrive once its worker is released.
    const LAND_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

    // Enough rounds to report a transfer's own lines before the slow Done lands.
    const LAND_ROUNDS: usize = 8;

    // A held rename answers slow within CALL_DEADLINE, then journals when it lands.
    #[test]
    fn a_held_remote_rename_answers_slow_then_journals_when_it_lands() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("slowrename");
        let from = d.file("before.txt", "body");
        let path = from.to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
        let (release, wait) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        // The worker runs the real rename only after the test releases it.
        let from_work = from.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        let started = std::time::Instant::now();
        do_rename_with(&mut buf, &mut o, &path, crate::backend::iomount::CALL_DEADLINE, work);
        let waited = started.elapsed();
        let line = text(&buf);
        assert!(line.contains(r#""t":"slow""#), "a held rename answers slow, never an error: {}", line);
        assert!(line.contains(" is slow. The rename will finish on its own."), "the slow line carries its sentence: {}", line);
        assert!(line.contains(r#""op":"rename""#), "the slow line names its op: {}", line);
        assert!(waited >= crate::backend::iomount::CALL_DEADLINE, "the slow line waits out the deadline: {:?}", waited);
        assert!(waited < crate::backend::iomount::CALL_DEADLINE + SLOW_MARGIN, "the slow line answers at the deadline: {:?}", waited);
        let mut busy = out();
        do_undo(&mut busy, &mut o);
        assert!(text(&busy).contains("already running"), "undo waits for the late entry: {}", text(&busy));
        assert!(o.journal.is_empty(), "nothing is journalled until the held write lands");
        drop(release);
        let landed = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed","ok":true"#), "the late write answers its own reply: {}", text(&late));
        assert_eq!(o.journal.len(), 1, "one journal entry per request, so one undo reverses it");
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone","op":"rename","ok":true}"#), "{}", text(&undone));
        assert!(from.exists(), "undo put the old name back");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "body");
    }

    // The same pair for mkdir: a held remote mkdir answers slow, then journals on landing.
    #[test]
    fn a_held_remote_mkdir_answers_slow_then_journals_when_it_lands() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("slowmkdir");
        let parent = d.path().to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
        let (release, wait) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        let base_work = d.path().to_path_buf();
        let work = move || {
            let _ = wait.recv();
            super::ops::mkdir(&base_work, "photos")
        };
        do_mkdir_with(&mut buf, &mut o, &parent, SHORT_DEADLINE, work);
        let line = text(&buf);
        assert!(line.contains(r#""t":"slow""#), "a held mkdir answers slow, never an error: {}", line);
        assert!(line.contains(" is slow. The new folder will finish on its own."), "the mkdir line is its own sentence: {}", line);
        assert!(line.contains(r#""op":"mkdir""#), "the slow line names its op: {}", line);
        assert!(o.journal.is_empty(), "nothing is journalled until the held write lands");
        drop(release);
        let landed = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"made","ok":true"#), "the late write answers its own reply: {}", text(&late));
        assert!(d.join("photos").is_dir(), "the released mkdir landed");
        assert_eq!(o.journal.len(), 1, "one journal entry per request, so one undo reverses it");
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone","op":"mkdir","ok":true}"#), "{}", text(&undone));
        assert!(!d.join("photos").exists(), "undo removed the folder it made");
    }

    // A slow write holds its mount, so undo, redo and a write on that mount answer busy until it lands.
    #[test]
    fn an_undo_while_a_slow_write_is_in_flight_answers_busy() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("slowbusy");
        let parent = d.path().to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &parent, "older", None);
        assert!(text(&buf).contains(r#""t":"made""#), "the older mkdir journals first: {}", text(&buf));
        let victim = d.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let (release, wait) = channel::<()>();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        let mut slow = out();
        do_rename_with(&mut slow, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&slow).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&slow));
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains("already running"), "undo waits for the late entry: {}", text(&undone));
        assert_eq!(o.journal.len(), 1, "the older mkdir survives an undo aimed past it");
        let mut redone = out();
        start_redo(&mut redone, &mut o);
        assert!(text(&redone).contains("already running"), "redo waits too: {}", text(&redone));
        let mut second = out();
        do_mkdir(&mut second, &mut o, &parent, "newer", None);
        assert!(text(&second).contains("already running"), "a second write waits too: {}", text(&second));
        drop(release);
        let landed = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed""#), "the late write journals on landing: {}", text(&late));
        assert_eq!(o.journal.len(), 2, "the late entry lands behind the older one");
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone","op":"rename""#), "undo reverses the late write first: {}", text(&undone));
        assert!(victim.exists(), "undo put the rename's old name back");
        assert!(d.join("older").is_dir(), "the older mkdir is still journalled underneath");
    }

    // A slow line frees the slot at once, so a local transfer starts while the remote write is still held.
    #[test]
    fn a_slow_write_releases_the_slot_so_a_local_transfer_runs() {
        crate::backend::iomount::test_reset();
        let remote = TestDir::new("slowfreeslot");
        let victim = remote.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(remote.path()));
        let (release, wait) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        do_rename_with(&mut buf, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&buf).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&buf));
        assert!(o.live.running().is_none(), "a slow line frees the slot at once");
        let local = TestDir::new("slowfreeslotlocal");
        let source = local.file("cargo.txt", "cargo");
        let dest = local.dir("dest");
        let mut started = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"copy"}"#);
        start_transfer(&mut started, &mut o, "copy", vec![source.to_string_lossy().to_string()], &dest.to_string_lossy(), ask);
        assert!(text(&started).contains(r#""t":"transferstarted""#), "a local transfer runs beside the held write: {}", text(&started));
        drop(release);
        let mut landed = false;
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if text(&sink).contains(r#""t":"renamed""#) {
                landed = true;
                break;
            }
        }
        assert!(landed, "the released rename journals late");
        // The local transfer's own Done may still be in flight; drain its job before undo.
        for _ in 0..LAND_ROUNDS {
            if o.jobs.unfinished() == 0 {
                break;
            }
            let msg = rx.recv_timeout(LAND_BOUND).expect("the local transfer reports through the op channel");
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
        }
        assert_eq!(
            o.jobs.unfinished(),
            0,
            "the local transfer finishes its own job"
        );
        assert!(
            dest.join("cargo.txt").exists(),
            "the local transfer landed beside the held write"
        );
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone""#), "no pending entry survives the late Done: {}", text(&undone));
    }

    // A write on the slow mount waits while the slot stays free; it runs nothing and journals nothing.
    #[test]
    fn a_write_on_the_slow_mount_answers_busy_while_the_slot_stays_free() {
        crate::backend::iomount::test_reset();
        let remote = TestDir::new("slowsamemount");
        let victim = remote.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(remote.path()));
        let (release, wait) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        do_rename_with(&mut buf, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&buf).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&buf));
        assert!(o.live.running().is_none(), "a slow line frees the slot at once");
        let parent = remote.path().to_string_lossy().to_string();
        let base_work = remote.path().to_path_buf();
        let mut second = out();
        do_mkdir_with(&mut second, &mut o, &parent, SHORT_DEADLINE, move || super::ops::mkdir(&base_work, "newer"));
        assert!(text(&second).contains("already running"), "a write on the slow mount waits: {}", text(&second));
        assert!(!remote.join("newer").exists(), "the refused write touches nothing");
        assert!(o.journal.is_empty(), "the refused write journals nothing");
        drop(release);
        let landed = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed""#), "the late write journals on landing: {}", text(&late));
        assert_eq!(o.journal.len(), 1, "one journal entry per request, so one undo reverses it");
    }

    // Undo waits on the pending path until the late Done journals, then reverses it.
    #[test]
    fn an_undo_names_the_pending_path_until_the_late_done_journals() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("slowundoname");
        let victim = d.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
        let (release, wait) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        do_rename_with(&mut buf, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&buf).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&buf));
        let mut blocked = out();
        do_undo(&mut blocked, &mut o);
        let line = text(&blocked);
        assert!(line.contains("already running"), "undo waits for the late entry: {}", line);
        assert!(line.contains(&path), "undo names the pending path: {}", line);
        drop(release);
        let landed = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed""#), "the late write journals on landing: {}", text(&late));
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone","op":"rename""#), "undo reverses the late write: {}", text(&undone));
        assert!(victim.exists(), "undo put the rename's old name back");
    }

    // Two slow writes on two mounts each clear only their own entry.
    #[test]
    fn two_pending_writes_each_clear_only_their_own_entry() {
        crate::backend::iomount::test_reset();
        let first = TestDir::new("slowtwofirst");
        let second = TestDir::new("slowtwosecond");
        let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n31 1 0:46 / {} rw - nfs n:/t rw\n", first.path().to_string_lossy(), second.path().to_string_lossy());
        let _guard = crate::backend::iomount::test_hold_body(body);
        let victim = first.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let (release_first, wait_first) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait_first.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        do_rename_with(&mut buf, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&buf).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&buf));
        let parent = second.path().to_string_lossy().to_string();
        let (release_second, wait_second) = channel::<()>();
        let base_work = second.path().to_path_buf();
        let mut buf2 = out();
        let work2 = move || {
            let _ = wait_second.recv();
            super::ops::mkdir(&base_work, "photos")
        };
        do_mkdir_with(&mut buf2, &mut o, &parent, SHORT_DEADLINE, work2);
        assert!(text(&buf2).contains(r#""t":"slow""#), "a second mount's slow write answers slow beside the first: {}", text(&buf2));
        drop(release_first);
        let mut first_landed = false;
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if text(&sink).contains(r#""t":"renamed""#) {
                first_landed = true;
                break;
            }
        }
        assert!(first_landed, "the first released write journals late");
        let mut blocked = out();
        do_undo(&mut blocked, &mut o);
        let line = text(&blocked);
        assert!(line.contains("already running"), "undo still waits on the second mount: {}", line);
        assert!(line.contains(&parent), "undo names the remaining path: {}", line);
        drop(release_second);
        let mut second_landed = false;
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if text(&sink).contains(r#""t":"made""#) {
                second_landed = true;
                break;
            }
        }
        assert!(second_landed, "the second released write journals late");
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone","op":"mkdir""#), "undo reverses the newest entry first: {}", text(&undone));
        assert!(!second.join("photos").exists(), "undo removed the folder it made");
        let mut undone = out();
        do_undo(&mut undone, &mut o);
        assert!(text(&undone).contains(r#"{"t":"undone","op":"rename""#), "undo reverses the late write next: {}", text(&undone));
        assert!(victim.exists(), "undo put the rename's old name back");
    }

    // A late Done carries its own id, so it never frees a newer operation's slot.
    #[test]
    fn a_late_done_after_a_newer_claim_leaves_the_newer_claim_running() {
        crate::backend::iomount::test_reset();
        let d = TestDir::new("slowlatedone");
        let victim = d.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(d.path()));
        let (release, wait) = channel::<()>();
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let mut buf = out();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        do_rename_with(&mut buf, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&buf).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&buf));
        assert!(o.live.running().is_none(), "a slow line frees the slot at once");
        let (next, _flag) = o.claim_transfer();
        assert_eq!(o.live.running(), Some(next), "a newer operation claims the free slot beside the held write");
        drop(release);
        let landed = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed""#), "the late write still lands: {}", text(&late));
        assert_eq!(o.live.running(), Some(next), "a late Done never frees a newer operation's slot");
        assert_eq!(o.journal.len(), 1, "the late entry still journals exactly once");
        o.live.finished();
    }

    // A slow write never takes the slot: a held rename beside a running transfer keeps that transfer's id and its cancel.
    #[test]
    fn a_slow_rename_beside_a_running_transfer_keeps_the_transfer_slot() {
        crate::backend::iomount::test_reset();
        let remote = TestDir::new("slowkeepstransfer");
        let victim = remote.file("victim.txt", "v");
        let path = victim.to_string_lossy().to_string();
        let _body = crate::backend::iomount::test_hold_body(remote_body(remote.path()));
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        // A transfer holds the slot first, so the slow write must number itself without touching it.
        let (transfer, flag) = o.claim_transfer();
        assert_eq!(o.live.running(), Some(transfer));
        let (release, wait) = channel::<()>();
        let from_work = victim.clone();
        let work = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        let mut slow = out();
        do_rename_with(&mut slow, &mut o, &path, SHORT_DEADLINE, work);
        assert!(text(&slow).contains(r#""t":"slow""#), "the held rename answers slow: {}", text(&slow));
        assert_eq!(o.live.running(), Some(transfer), "the slow rename never took the transfer's slot");
        cancel_transfer(&mut slow, &mut o, transfer);
        assert!(flag.load(Ordering::Relaxed), "the transfer's cancel still reaches it");
        drop(release);
        let landed = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed""#), "the late write still lands: {}", text(&late));
        assert_eq!(o.live.running(), Some(transfer), "the late Done never frees the transfer's slot");
        assert_eq!(o.journal.len(), 1, "the late entry still journals exactly once");
        o.live.finished();
    }

    // One sweep over every write kind: each answers busy on a mount held slow and runs elsewhere.
    #[test]
    fn every_write_kind_waits_on_a_pending_mount_and_runs_elsewhere() {
        crate::backend::iomount::test_reset();
        let remote = TestDir::new("gateallremote");
        let local = TestDir::new("gatealllocal");
        let _body = crate::backend::iomount::test_hold_body(remote_body(remote.path()));
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        // One held rename keeps the remote mount pending for the whole sweep.
        let victim = remote.file("victim.txt", "v");
        let (release, wait) = channel::<()>();
        let from_work = victim.clone();
        let held = move || {
            let _ = wait.recv();
            super::ops::rename(&from_work, "after.txt")
        };
        let mut slow = out();
        do_rename_with(&mut slow, &mut o, &victim.to_string_lossy(), SHORT_DEADLINE, held);
        assert!(text(&slow).contains(r#""t":"slow""#), "the sweep holds its mount slow: {}", text(&slow));
        // A transfer on the held mount waits, and journals nothing.
        let remote_dest = remote.dir("xferdest");
        let remote_cargo = remote.file("cargo.txt", "cargo");
        let journaled = o.journal.len();
        let mut busy = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"copy"}"#);
        start_transfer(&mut busy, &mut o, "copy", vec![remote_cargo.to_string_lossy().to_string()], &remote_dest.to_string_lossy(), ask);
        assert!(text(&busy).contains("already running"), "a transfer onto a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused transfer journals nothing");
        assert!(remote_dest.read_dir().unwrap().next().is_none(), "the refused transfer lands nothing");
        // The same transfer elsewhere runs beside the held write.
        let local_dest = local.dir("xferdest");
        let local_cargo = local.file("cargo.txt", "cargo");
        let mut started = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"copy"}"#);
        start_transfer(&mut started, &mut o, "copy", vec![local_cargo.to_string_lossy().to_string()], &local_dest.to_string_lossy(), ask);
        assert!(text(&started).contains(r#""t":"transferstarted""#), "a local transfer runs beside the held write: {}", text(&started));
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the local transfer reports through the op channel");
            let terminal = matches!(&msg, OpMsg::TransferDone { .. });
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if terminal {
                break;
            }
        }
        assert!(o.live.running().is_none(), "the local transfer frees the slot it claimed");
        assert!(local_dest.join("cargo.txt").exists(), "the local transfer landed beside the held write");
        // Trash on the held mount waits, and the file stays where it was.
        let remote_trash = remote.file("t.txt", "t");
        let journaled = o.journal.len();
        let mut busy = out();
        start_trash(&mut busy, &mut o, vec![remote_trash.to_string_lossy().to_string()], 0, 0);
        assert!(text(&busy).contains("already running"), "trash on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused trash journals nothing");
        assert!(remote_trash.exists(), "the refused trash touches nothing");
        // The same trash elsewhere leaves the process with no refusal on the wire.
        let local_trash = local.file("t.txt", "t");
        let mut quiet = out();
        start_trash(&mut quiet, &mut o, vec![local_trash.to_string_lossy().to_string()], 0, 0);
        assert!(text(&quiet).contains(r#""t":"trashstarted""#), "trash elsewhere acknowledges acceptance in sync: {}", text(&quiet));
        let mut trash_journaled = None;
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the trash reports through the op channel");
            let terminal = matches!(&msg, OpMsg::Trashed { .. });
            if let OpMsg::Trashed { entry, .. } = &msg {
                trash_journaled = Some(usize::from(!entry.steps.is_empty()));
            }
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if terminal {
                break;
            }
        }
        let trash_journaled = trash_journaled.expect("the trash batch must complete before the sweep continues");
        assert!(o.live.running().is_none(), "the trash frees the slot it claimed");
        // A menu selection on the held mount waits even when the paths name files elsewhere.
        let remote_sel = remote.file("sel.txt", "s");
        let menu = super::super::menu_actions::MenuActions::new(o.tx.clone());
        assert!(menu.request(r#"{"op":"snapshot","id":5}"#.into(), vec![remote_sel.to_string_lossy().to_string()], None));
        let snap = rx.recv_timeout(LAND_BOUND).expect("the snapshot replies");
        assert!(matches!(&snap, OpMsg::Meta { .. }), "the snapshot answers Meta");
        o.menuactions = Some(menu);
        let local_other = local.file("other.txt", "o");
        let journaled = o.journal.len();
        let mut busy = out();
        start_trash(&mut busy, &mut o, vec![local_other.to_string_lossy().to_string()], 5, 0);
        assert!(text(&busy).contains("already running"), "a selection on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused trash journals nothing");
        assert!(local_other.exists(), "the refused trash touches nothing");
        // Duplicate on the held mount waits, and the source stays single.
        let remote_dup = remote.file("d.txt", "d");
        let journaled = o.journal.len();
        let mut busy = out();
        start_duplicate(&mut busy, &mut o, &remote_dup.to_string_lossy(), 0);
        assert!(text(&busy).contains("already running"), "a duplicate on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused duplicate journals nothing");
        assert!(remote_dup.exists(), "the refused duplicate touches nothing");
        // The same duplicate elsewhere runs, and a divergent selection still waits.
        let local_dup = local.file("d.txt", "d");
        let mut quiet = out();
        start_duplicate(&mut quiet, &mut o, &local_dup.to_string_lossy(), 0);
        assert!(text(&quiet).is_empty(), "a duplicate elsewhere answers nothing in sync: {}", text(&quiet));
        let landed = rx.recv_timeout(LAND_BOUND).expect("the duplicate reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"duplicated""#), "the duplicate lands elsewhere: {}", text(&late));
        let local_divergent = local.file("divergent.txt", "d");
        let journaled = o.journal.len();
        let mut busy = out();
        start_duplicate(&mut busy, &mut o, &local_divergent.to_string_lossy(), 5);
        assert!(text(&busy).contains("already running"), "a divergent selection on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused duplicate journals nothing");
        // Rename on the held mount waits, and the old name stays.
        let remote_rename = remote.file("r.txt", "r");
        let journaled = o.journal.len();
        let mut busy = out();
        do_rename(&mut busy, &mut o, &remote_rename.to_string_lossy(), "r2.txt");
        assert!(text(&busy).contains("already running"), "a rename on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused rename journals nothing");
        assert!(remote_rename.exists() && !remote.join("r2.txt").exists(), "the refused rename touches nothing");
        // The same rename elsewhere lands at once.
        let local_rename = local.file("r.txt", "r");
        let mut done = out();
        do_rename(&mut done, &mut o, &local_rename.to_string_lossy(), "r2.txt");
        assert!(text(&done).contains(r#""t":"renamed","ok":true"#), "a rename elsewhere lands: {}", text(&done));
        // Mkdir on the held mount waits, and the folder never appears.
        let journaled = o.journal.len();
        let mut busy = out();
        do_mkdir(&mut busy, &mut o, &remote.path().to_string_lossy(), "heldsub", None);
        assert!(text(&busy).contains("already running"), "a mkdir on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused mkdir journals nothing");
        assert!(!remote.join("heldsub").exists(), "the refused mkdir touches nothing");
        // The same mkdir elsewhere lands at once.
        let mut done = out();
        do_mkdir(&mut done, &mut o, &local.path().to_string_lossy(), "photos", None);
        assert!(text(&done).contains(r#""t":"made","ok":true"#), "a mkdir elsewhere lands: {}", text(&done));
        // A new file on the held mount waits, and nothing is created.
        let journaled = o.journal.len();
        let mut busy = out();
        do_newfile(&mut busy, &mut o, &remote.path().to_string_lossy(), "held.txt", 21);
        assert!(text(&busy).contains("already running"), "a new file on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused new file journals nothing");
        assert!(!remote.join("held.txt").exists(), "the refused new file touches nothing");
        // The same new file elsewhere lands at once.
        let mut done = out();
        do_newfile(&mut done, &mut o, &local.path().to_string_lossy(), "fresh.txt", 22);
        assert!(text(&done).contains(r#""ok":true"#), "a new file elsewhere lands: {}", text(&done));
        assert!(local.join("fresh.txt").exists(), "the new file landed elsewhere");
        // A link onto the held mount waits, and nothing is linked.
        let remote_linkdest = remote.dir("linkdest");
        let remote_link = remote.file("l.txt", "l");
        let journaled = o.journal.len();
        let mut busy = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut busy, &mut o, "relative", vec![remote_link.to_string_lossy().to_string()], &remote_linkdest.to_string_lossy(), ask);
        assert!(text(&busy).contains("already running"), "a link onto a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused link journals nothing");
        assert!(remote_linkdest.read_dir().unwrap().next().is_none(), "the refused link lands nothing");
        // The same link elsewhere runs beside the held write, answering through the op channel.
        let local_linkdest = local.dir("linkdest");
        let local_link = local.file("l.txt", "l");
        let mut quiet = out();
        let ask = crate::backend::collide::Ask::parse(r#"{"c":"link"}"#);
        start_link(&mut quiet, &mut o, "relative", vec![local_link.to_string_lossy().to_string()], &local_linkdest.to_string_lossy(), ask);
        assert!(text(&quiet).is_empty(), "a link elsewhere answers nothing in sync: {}", text(&quiet));
        let landed = rx.recv_timeout(LAND_BOUND).expect("the link reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"linked""#), "a link elsewhere lands: {}", text(&late));
        // A delete on the held mount waits, and the file stays.
        let remote_del = remote.file("del.txt", "d");
        let journaled = o.journal.len();
        let mut busy = out();
        request_menu_action(&mut busy, &mut o, r#"{"op":"delete","id":6,"token":1}"#.into(), vec![remote_del.to_string_lossy().to_string()], None);
        assert!(text(&busy).contains("already running"), "a delete on a pending mount waits: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused delete journals nothing");
        assert!(remote_del.exists(), "the refused delete touches nothing");
        // The same delete elsewhere reaches the menu's own expiry rather than the gate.
        let local_del = local.file("del.txt", "d");
        let mut quiet = out();
        request_menu_action(&mut quiet, &mut o, r#"{"op":"delete","id":6,"token":1}"#.into(), vec![local_del.to_string_lossy().to_string()], None);
        assert!(text(&quiet).is_empty(), "a delete elsewhere passes the gate in sync: {}", text(&quiet));
        let msg = rx.recv_timeout(LAND_BOUND).expect("the delete answers through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, msg);
        assert!(text(&late).contains(crate::backend::menu_actions::SELECTION_CHANGED), "a delete elsewhere reaches the menu's own expiry: {}", text(&late));
        assert!(!text(&late).contains("already running"), "a delete elsewhere is never the gate: {}", text(&late));
        assert!(o.live.running().is_none(), "the expired delete frees the slot it claimed");
        // A compress naming the held mount waits before any tool runs.
        let remote_compress = remote.file("c.txt", "c");
        let journaled = o.journal.len();
        let mut busy = out();
        let formats = std::sync::Arc::new(crate::backend::archive::Formats::from_tools(true, true));
        crate::backend::archivereq::start_archive(&mut busy, &mut o, std::sync::Arc::clone(&formats), "compress",
            vec![remote_compress.to_string_lossy().to_string()], "tar".into(), PathBuf::new(), remote.join("out.tar"), 0, String::new(), false);
        assert!(text(&busy).contains("already running"), "a compress on a pending mount waits: {}", text(&busy));
        assert!(!text(&busy).contains("archivestarted"), "the refused compress starts nothing: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused compress journals nothing");
        assert!(!remote.join("out.tar").exists(), "the refused compress touches nothing");
        // The same compress elsewhere starts, and a divergent selection still waits.
        let local_compress = local.file("c.txt", "c");
        let mut started = out();
        crate::backend::archivereq::start_archive(&mut started, &mut o, std::sync::Arc::clone(&formats), "compress",
            vec![local_compress.to_string_lossy().to_string()], "tar".into(), PathBuf::new(), local.join("out.tar"), 0, String::new(), false);
        assert!(text(&started).contains("archivestarted"), "a compress elsewhere starts: {}", text(&started));
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the compress reports through the op channel");
            let terminal = matches!(&msg, OpMsg::JobDone { .. });
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if terminal {
                break;
            }
        }
        assert!(o.detached.is_empty(), "the compress leaves the quit registry");
        assert_eq!(
            o.jobs.unfinished(),
            0,
            "the archive terminal result completes its job"
        );
        let local_divergent = local.file("c2.txt", "c");
        let mut busy = out();
        crate::backend::archivereq::start_archive(&mut busy, &mut o, std::sync::Arc::clone(&formats), "compress",
            vec![local_divergent.to_string_lossy().to_string()], "tar".into(), PathBuf::new(), local.join("out2.tar"), 5, String::new(), false);
        assert!(text(&busy).contains("already running"), "a divergent selection on a pending mount waits: {}", text(&busy));
        assert!(!text(&busy).contains("archivestarted"), "the refused compress starts nothing: {}", text(&busy));
        // An extract of the held mount waits before any tool runs.
        let remote_archive = remote.file("pkg.tar", "p");
        let remote_exdest = remote.dir("exdest");
        let journaled = o.journal.len();
        let mut busy = out();
        crate::backend::archivereq::start_archive(&mut busy, &mut o, std::sync::Arc::clone(&formats), "extract",
            Vec::new(), "tar".into(), remote_archive.clone(), remote_exdest.clone(), 0, String::new(), false);
        assert!(text(&busy).contains("already running"), "an extract on a pending mount waits: {}", text(&busy));
        assert!(!text(&busy).contains("archivestarted"), "the refused extract starts nothing: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused extract journals nothing");
        // The same extract elsewhere starts, even when its archive is not one.
        let bogus = local.join("pkg.tar");
        std::fs::write(&bogus, "not an archive").unwrap();
        let fresh = local.join("exdest-fresh");
        let mut started = out();
        crate::backend::archivereq::start_archive(&mut started, &mut o, std::sync::Arc::clone(&formats), "extract",
            Vec::new(), "tar".into(), bogus, fresh, 0, String::new(), false);
        assert!(text(&started).contains("archivestarted"), "an extract elsewhere starts: {}", text(&started));
        for _ in 0..LAND_ROUNDS {
            let msg = rx.recv_timeout(LAND_BOUND).expect("the extract reports through the op channel");
            let terminal = matches!(&msg, OpMsg::JobDone { .. });
            let mut sink = out();
            report_op(&mut sink, &mut o, msg);
            if terminal {
                break;
            }
        }
        assert!(o.live.running().is_none(), "the extract frees the slot it claimed");
        assert_eq!(
            o.jobs.unfinished(),
            0,
            "the archive terminal result completes its job"
        );
        // A convert of the held mount waits before anything is inspected.
        let remote_convert = remote.file("in.png", "pixels");
        let journaled = o.journal.len();
        let mut busy = out();
        crate::backend::archivereq::start_convert(&mut busy, &mut o, remote_convert.clone(), remote.join("out.jpg"), false, 0, 71, false);
        assert!(text(&busy).contains("already running"), "a convert on a pending mount waits: {}", text(&busy));
        assert!(!text(&busy).contains("convertstarted"), "the refused convert starts nothing: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused convert journals nothing");
        assert!(!remote.join("out.jpg").exists(), "the refused convert touches nothing");
        // The same convert elsewhere passes the gate, and a divergent selection still waits.
        let local_convert = local.file("in.png", "pixels");
        let mut past = out();
        crate::backend::archivereq::start_convert(&mut past, &mut o, local_convert.clone(), local.join("out.jpg"), false, 0, 72, false);
        assert!(!text(&past).contains("already running"), "a convert elsewhere passes the gate: {}", text(&past));
        if text(&past).contains("convertstarted") {
            let landed = rx.recv_timeout(LAND_BOUND).expect("the convert reports through the op channel");
            assert!(matches!(&landed, OpMsg::DetachedDone { .. }), "a convert leaves the quit registry");
            let mut sink = out();
            report_op(&mut sink, &mut o, landed);
        }
        let local_in2 = local.file("in2.png", "pixels");
        let mut busy = out();
        crate::backend::archivereq::start_convert(&mut busy, &mut o, local_in2, local.join("out2.jpg"), false, 5, 73, false);
        assert!(text(&busy).contains("already running"), "a divergent selection on a pending mount waits: {}", text(&busy));
        // A convert probe only reads, so it never waits on the gate.
        let mut probe = out();
        crate::backend::archivereq::start_convert(&mut probe, &mut o, remote_convert, remote.join("out.jpg"), false, 0, 74, true);
        assert!(text(&probe).contains("convertchecked"), "a probe answers past the gate: {}", text(&probe));
        assert!(!text(&probe).contains("already running"), "a probe is never the gate: {}", text(&probe));
        // A permissions batch on the held mount waits, and the mode stays.
        let remote_perm = remote.file("p.txt", "p");
        let journaled = o.journal.len();
        let mut busy = out();
        do_permissions_batch(&mut busy, &mut o, vec![remote_perm.to_string_lossy().to_string()], vec!["644".to_string()], 81);
        assert!(text(&busy).contains("already running"), "permissions on a pending mount wait: {}", text(&busy));
        assert_eq!(o.journal.len(), journaled, "the refused permissions batch journals nothing");
        // The same batch elsewhere applies at once.
        let local_perm = local.file("p.txt", "p");
        let mut done = out();
        do_permissions_batch(&mut done, &mut o, vec![local_perm.to_string_lossy().to_string()], vec!["644".to_string()], 82);
        assert!(text(&done).contains(r#""ok":true"#), "permissions elsewhere apply: {}", text(&done));
        // Eight known writes journal once; Trash adds one only when its completed receipt has steps.
        let journaled = o.journal.len();
        drop(release);
        let landed = rx.recv_timeout(LAND_BOUND).expect("the released write reports through the op channel");
        let mut late = out();
        report_op(&mut late, &mut o, landed);
        assert!(text(&late).contains(r#""t":"renamed""#), "the released rename journals late: {}", text(&late));
        assert_eq!(journaled, 0, "the accepted rename holds later entries behind its reservation");
        assert_eq!(o.journal.len(), 8 + trash_journaled, "every completed write journals exactly once; an empty Trash receipt journals nothing");
        assert!(o.live.running().is_none(), "no pending entry survives the late Done");
        assert!(o.pending.is_empty(), "the late Done clears its own pending entry");
    }

    // An absent menu service holds no selection, so every menu route answers the one sentence.
    const SELECTION_GONE: &str = "Selected items changed; reopen the menu.";

    #[test]
    fn a_menu_route_with_no_menu_service_says_the_selection_changed() {
        let d = TestDir::new("dispatchnomenu");
        let o = ops();
        assert_eq!(menu_sources(&o, 5).unwrap_err(), SELECTION_GONE);
        assert!(menu_sources(&o, 0).unwrap().is_none(), "a request with no menu names no selection");
        let mut o = ops();
        let mut buf = out();
        do_menu_rename(&mut buf, &mut o, &d.file("a.txt", "a").to_string_lossy(), "b.txt", 5);
        assert!(text(&buf).contains(SELECTION_GONE), "{}", text(&buf));
    }
}
