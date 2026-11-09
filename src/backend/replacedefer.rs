// A transfer's deferred Replace session: trash each replaced item now, attribute every one to its gio entry at settle.
use crate::backend::undo::Step;
use crate::error::FleaError;
use flea::bytename::ExactText as _;
use std::path::{Path, PathBuf};

// One merge's replaces under a single before/after list pair: each gio trash stays synchronous, so N replaces cost 2 lists instead of 2N, and a marked put-back restores at settle.
pub(crate) struct Deferred {
    before: Option<Vec<crate::backend::trash::Entry>>,
    listed: bool,
    // Each trashed path with the local time its trash began, so an unattributed step names when it went, not when the batch settled.
    trashed: Vec<(PathBuf, String)>,
    home: Vec<(PathBuf, String)>,
    // The Trash and parent folders an in-process rename left unsynced, synced once when the batch settles.
    unsynced: std::collections::BTreeSet<PathBuf>,
    // Whether the first renamed item's folder, which shares the home Trash's filesystem, is on a drive Flea confirms.
    trash_durable: Option<bool>,
}

impl Deferred {
    pub(crate) fn begin() -> Deferred {
        // The first list waits for the first trash, so a batch that replaces nothing lists nothing.
        Deferred { before: None, listed: false, trashed: Vec::new(), home: Vec::new(), unsynced: std::collections::BTreeSet::new(), trash_durable: None }
    }
    // Trash one replaced file now, refusing a path already gone before gio runs, the Issue 88 rule.
    pub(crate) fn trash_one(&mut self, dst: &Path, cancel: &std::sync::atomic::AtomicBool) -> Result<(), String> {
        // A refusal for any other reason keeps today's message, whatever gio answered.
        if dst.symlink_metadata().is_err() {
            return Err(crate::backend::collide::TRASH_REFUSED.to_string());
        }
        // A location with no Trash of its own keeps the old item in the home Trash, which names its own entry, so it needs no list.
        if !crate::backend::hometrash::trashable(dst) {
            return crate::backend::hometrash::trash_to_home(dst, cancel).map(|entry| self.keep_home(dst, entry));
        }
        // An item on the home Trash's own filesystem is renamed there in-process, which also names its own entry.
        if let Some(moved) = crate::backend::hometrash::rename_to_home(dst, &mut self.unsynced, &mut self.trash_durable) {
            return moved.map(|entry| self.keep_home(dst, entry));
        }
        if !self.listed {
            self.before = crate::backend::trash::list();
            self.listed = true;
        }
        // Without the before list no settle can attribute, so the Replace is refused before anything moves.
        if self.before.is_none() {
            return Err(crate::backend::collide::TRASH_REFUSED.to_string());
        }
        // Stamped before gio runs, as a chunk is, so the deletion date falls after it and undo's window starts there.
        let began = crate::backend::hometrash::deletion_date_local();
        let _ = crate::backend::trash::gio(&["trash", "--", &dst.exact_text()]);
        if dst.symlink_metadata().is_ok() {
            return Err(crate::backend::collide::TRASH_REFUSED.to_string());
        }
        self.trashed.push((dst.to_path_buf(), began));
        Ok(())
    }
    // Records a home Trash entry with its own URI, so neither attribute nor settle lists for it.
    fn keep_home(&mut self, dst: &Path, entry: crate::backend::trash::Entry) {
        self.home.push((dst.to_path_buf(), entry.uri));
        self.trashed.push((dst.to_path_buf(), crate::backend::hometrash::deletion_date_local()));
    }
    // One list now attributes one failed replace, so its put-back restores at once.
    pub(crate) fn attribute(&self, dst: &Path) -> crate::backend::trash::Entry {
        // A home Trash entry carries its own URI, so no gio list can attribute it.
        if let Some((_, uri)) = self.home.iter().find(|(path, _)| path == dst) {
            return crate::backend::trash::Entry { original: dst.to_path_buf(), uri: uri.clone(), before: Vec::new(), trashed_at: String::new() };
        }
        let after = crate::backend::trash::list();
        self.before.as_ref().zip(after.as_ref())
            .and_then(|(b, a)| crate::backend::trash::newest_entry_for(b, a, dst))
            .unwrap_or_else(|| match self.before_uris(dst) {
                Some(uris) => crate::backend::trash::Entry::unresolved(dst.to_path_buf(), uris, self.trashed_at(dst)),
                None => crate::backend::trash::Entry::unresolved(dst.to_path_buf(), Vec::new(), String::new()),
            })
    }
    // The before list's URIs for this one path, so an unattributed settle still resolves at undo; None when that list was never read.
    fn before_uris(&self, dst: &Path) -> Option<Vec<String>> {
        self.before.as_ref().map(|before| crate::backend::trash::uris_at(before, dst))
    }
    // The time this path's newest trash began; a path this session never trashed takes the time now.
    fn trashed_at(&self, dst: &Path) -> String {
        self.trashed.iter().rev().find(|(path, _)| path == dst).map_or_else(crate::backend::hometrash::deletion_date_local, |(_, at)| at.clone())
    }
    // The renames' folders sync once, then one list-after attributes every trashed path whose step is still journaled.
    pub(crate) fn settle(&mut self, steps: &mut [Step]) {
        crate::backend::hometrash::sync_folders(&mut self.unsynced);
        if self.trashed.is_empty() {
            return;
        }
        let home = std::mem::take(&mut self.home);
        // A batch whose every Replace went to the home Trash already holds each URI, so it lists nothing.
        let after = if self.trashed.iter().all(|(dst, _)| home.iter().any(|(path, _)| path == dst)) {
            None
        } else {
            match crate::backend::trash::list() {
                // One retry, so a single dropped list does not strand every Replace in the batch.
                None => crate::backend::trash::list(),
                some => some,
            }
        };
        let pair = self.before.as_ref().zip(after.as_ref());
        for (dst, at) in std::mem::take(&mut self.trashed) {
            // A home Trash entry carries its own URI, so it lands here and never in the gio pairing.
            if let Some((_, uri)) = home.iter().find(|(path, _)| path == &dst) {
                for step in steps.iter_mut() {
                    if let Step::Trashed(existing) = step {
                        if existing.original == dst {
                            existing.uri.clone_from(uri);
                            break;
                        }
                    }
                }
                continue;
            }
            let found = pair.and_then(|(b, a)| crate::backend::trash::newest_entry_for(b, a, &dst));
            for step in steps.iter_mut() {
                if let Step::Trashed(existing) = step {
                    // Only an unattributed step takes resolution data; a put-back already filled its URI.
                    if existing.original == dst && existing.uri.is_empty() {
                        match &found {
                            Some(entry) => existing.uri.clone_from(&entry.uri),
                            // The path and the time stay, so undo resolves the URI from a fresh list.
                            None => {
                                if let Some(uris) = self.before_uris(&dst) {
                                    existing.before = uris;
                                    existing.trashed_at.clone_from(&at);
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
}

// Replace through the batch's deferred session: trash now, journal now, and a failed landing attributes its own entry and puts it back at once like the old path did.
pub(crate) fn replacing_deferred(dst: &Path, steps: &mut Vec<Step>, deferred: &mut Deferred, cancel: &std::sync::atomic::AtomicBool, land: impl FnOnce(&mut Vec<Step>) -> Result<(), FleaError>) -> Result<(), FleaError> {
    if let Err(reason) = deferred.trash_one(dst, cancel) {
        return Err(FleaError { where_: "transfer".into(), path: dst.to_string_lossy().into(), msg: reason });
    }
    let at = steps.len();
    steps.push(Step::Trashed(crate::backend::trash::Entry::unresolved(dst.to_path_buf(), Vec::new(), String::new())));
    let mut outcome = land(steps);
    if outcome.is_err() && crate::backend::collide::put_back_vacant(dst, steps, at) {
        let entry = deferred.attribute(dst);
        if let Step::Trashed(slot) = &mut steps[at] {
            slot.uri.clone_from(&entry.uri);
        }
        match crate::backend::trash::restore(&entry) {
            Ok(()) => steps.truncate(at),
            Err(restore) => {
                if let Err(error) = &mut outcome {
                    error.msg.push_str(&format!("{} ({})", crate::backend::collide::STILL_IN_TRASH, restore.msg));
                }
            }
        }
    }
    outcome
}
