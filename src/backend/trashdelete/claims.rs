// Shared claims preserve the same checks for single operations and bounded restore batches.
use super::*;

#[path = "restore_many.rs"]
mod many;
pub use many::{restore_many, RESTORE_BATCH_ITEMS};

struct Prepared {
    files: File,
    infos: File,
    quarantine: File,
    recovery: PathBuf,
    journal: Recovery,
}

impl Reviewed {
    pub fn restore(&self, original: &Path, recovery_root: &Path) -> Result<(), Refusal> {
        self.restore_after(original, recovery_root, || {})
    }
    pub(super) fn restore_after(
        &self,
        original: &Path,
        recovery_root: &Path,
        before_claim: impl FnOnce(),
    ) -> Result<(), Refusal> {
        self.prepare_claim(Op::Restore, recovery_root, Some(original), None)
            .and_then(|prepared| self.restore_prepared(original, prepared, before_claim))
            .map_err(Refusal::logged)
    }
    fn restore_prepared(
        &self,
        original: &Path,
        prepared: Prepared,
        before_claim: impl FnOnce(),
    ) -> Result<(), Refusal> {
        self.claimed_prepared(Op::Restore, prepared, before_claim, |quarantine| {
            let metadata = crate::backend::regfile::open_if_regular(
                &fd_path(quarantine, OsStr::new("metadata")),
                O_NOFOLLOW,
            )
            .map_err(|e| {
                Refusal::changed(format!("Could not read claimed Trash metadata: {}", e))
            })?;
            if Identity::of(
                &metadata
                    .metadata()
                    .changed("Claimed Trash metadata unreadable")?,
            ) != self.metadata
            {
                return Err(Refusal::changed(
                    "Trash metadata changed after being claimed.",
                ));
            }
            let destination = original_path(
                &metadata,
                self.path
                    .parent()
                    .and_then(Path::parent)
                    .changed("Missing Trash root.")?,
            )?;
            if destination != original
                || Identity::of(
                    &metadata
                        .metadata()
                        .changed("Claimed Trash metadata unreadable")?,
                ) != self.metadata
            {
                return Err(Refusal::new(
                    Outcome::OriginalChanged,
                    "The Trash item's original location changed; refresh the selection.",
                ));
            }
            let parent = destination
                .parent()
                .changed("Original parent is missing.")?;
            // A gone original folder is recreated first, as undo and gio restore do.
            let destination_parent = std::fs::create_dir_all(parent)
                .and_then(|()| parent.canonicalize())
                .map_err(|e| {
                    Refusal::os(
                        Outcome::Blocked,
                        &format!("Could not open original location {}", destination.display()),
                        &e,
                    )
                })?;
            let destination_directory =
                open_dir(&destination_parent).map_err(|refusal| refusal.wrap(Outcome::Blocked))?;
            rename(
                quarantine,
                OsStr::new("payload"),
                &destination_directory,
                destination
                    .file_name()
                    .changed("Original filename is missing.")?,
            )
            .map_err(|reason| {
                Refusal::new(
                    Outcome::Blocked,
                    format!(
                        "Could not restore {} without overwriting: {}",
                        destination.display(),
                        reason
                    ),
                )
                .because(reason)
            })
        })
    }
    pub(super) fn with_claimed(
        &self,
        op: Op,
        recovery_root: &Path,
        destination: Option<&Path>,
        before_claim: impl FnOnce(),
        perform: impl FnOnce(&File) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        self.claimed(op, recovery_root, destination, before_claim, perform)
            .map_err(Refusal::logged)
    }
    fn claimed(
        &self,
        op: Op,
        recovery_root: &Path,
        destination: Option<&Path>,
        before_claim: impl FnOnce(),
        perform: impl FnOnce(&File) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        let prepared = self.prepare_claim(op, recovery_root, destination, None)?;
        self.claimed_prepared(op, prepared, before_claim, perform)
    }
    fn prepare_claim(
        &self,
        op: Op,
        recovery_root: &Path,
        destination: Option<&Path>,
        batch: Option<recovery::batch::Ticket>,
    ) -> Result<Prepared, Refusal> {
        let files = open_dir(self.path.parent().changed("Missing Trash parent.")?)?;
        let infos = open_dir(
            self.info
                .parent()
                .changed("Missing Trash metadata parent.")?,
        )?;
        let name = self.path.file_name().changed("Invalid Trash filename.")?;
        let info_name = self
            .info
            .file_name()
            .changed("Invalid Trash metadata name.")?;
        if identity(&fd_path(&files, name)).changed("Trash item unreadable")? != self.payload
            || identity(&fd_path(&infos, info_name)).changed("Trash metadata unreadable")?
                != self.metadata
        {
            return Err(Refusal::new(
                Outcome::ConfirmAgain,
                "Trash item changed; confirm again.",
            ));
        }
        let quarantine_name = format!(
            ".flea-delete-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let quarantine_path = fd_path(&files, OsStr::new(&quarantine_name));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&quarantine_path)
            .map_err(|e| {
                Refusal::os(
                    Outcome::Prepare(op),
                    "Could not create Trash quarantine",
                    &e,
                )
            })?;
        let quarantine =
            open_dir(&quarantine_path).map_err(|refusal| refusal.wrap(Outcome::Prepare(op)))?;
        let recovery = self.path.parent().unwrap().join(&quarantine_name);
        let journal = match Recovery::prepare(
            recovery_root,
            recovery::RecordInput {
                source: &self.path,
                parent: &files,
                payload: &self.payload,
                paired: Some((&self.info, &self.metadata, &infos)),
                quarantine_path: &recovery,
                quarantine: &quarantine,
                tree: if destination.is_some() {
                    None
                } else {
                    self.tree.as_ref()
                },
                destination,
            },
            batch,
        ) {
            Ok(journal) => journal,
            Err(error) => {
                let _ = std::fs::remove_dir(&quarantine_path);
                return Err(error.wrap(Outcome::NoRecord(op)));
            }
        };
        Ok(Prepared {
            files,
            infos,
            quarantine,
            recovery,
            journal,
        })
    }
    fn claimed_prepared(
        &self,
        op: Op,
        prepared: Prepared,
        before_claim: impl FnOnce(),
        perform: impl FnOnce(&File) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        let Prepared {
            files,
            infos,
            quarantine,
            recovery,
            mut journal,
        } = prepared;
        let name = self.path.file_name().changed("Invalid Trash filename.")?;
        let info_name = self
            .info
            .file_name()
            .changed("Invalid Trash metadata name.")?;
        let quarantine_path = fd_path(
            &files,
            recovery.file_name().changed("Invalid quarantine name.")?,
        );
        before_claim();
        if let Err(error) = rename(&files, name, &quarantine, OsStr::new("payload")) {
            match journal.remove_empty(&quarantine_path, &quarantine, &files) {
                Ok(()) => settle(journal, op),
                Err(left) => {
                    left.logged();
                }
            }
            return Err(Refusal::new(
                Outcome::Prepare(op),
                format!("Could not claim Trash item: {}", error),
            )
            .because(error));
        }
        let info_claim = rename_claim(&infos, info_name, &quarantine, OsStr::new("metadata"));
        let checked = identity(&fd_path(&quarantine, OsStr::new("payload")))
            .ok()
            .as_ref()
            == Some(&self.payload)
            && info_claim.is_ok()
            && identity(&fd_path(&quarantine, OsStr::new("metadata")))
                .ok()
                .as_ref()
                == Some(&self.metadata);
        if !checked {
            let refused = match &info_claim {
                Err((ErrorKind::NotFound, _)) | Ok(()) => Refusal::new(
                    Outcome::ConfirmAgain,
                    "Trash changed; no operation attempted. Confirm again.",
                ),
                Err((_, reason)) => Refusal::new(
                    Outcome::Prepare(op),
                    format!("Could not claim Trash metadata: {reason}"),
                )
                .because(reason.clone()),
            };
            let payload_back = rename(&quarantine, OsStr::new("payload"), &files, name);
            let info_back = if info_claim.is_ok() {
                rename(&quarantine, OsStr::new("metadata"), &infos, info_name)
            } else {
                Ok(())
            };
            if payload_back.is_err() || info_back.is_err() {
                return Err(refused.wrap(Outcome::Kept(recovery.clone())));
            }
            match journal.remove_empty(&quarantine_path, &quarantine, &files) {
                Ok(()) => settle(journal, op),
                Err(left) => {
                    left.logged();
                }
            }
            return Err(refused);
        }
        let removal = perform(&quarantine);
        if let Err(error) = removal {
            let payload_back = rename(&quarantine, OsStr::new("payload"), &files, name);
            let info_back = rename(&quarantine, OsStr::new("metadata"), &infos, info_name);
            if payload_back.is_err() || info_back.is_err() {
                return Err(error.wrap(Outcome::Stuck(op, recovery)));
            }
            if journal
                .remove_empty(&quarantine_path, &quarantine, &files)
                .is_err()
            {
                return Err(error.wrap(Outcome::StuckInTrash(op, recovery)));
            }
            if error.outcome() == &Outcome::Cancelled {
                finish(journal, op, false).map_err(Refusal::logged)?;
                return Err(error);
            }
            settle(journal, op);
            // A refusal that is a whole sentence already says what was done: nothing was deleted, or nothing was restored.
            let whole = match (op, error.outcome()) {
                (_, Outcome::NothingDeleted) => true,
                (Op::Restore, Outcome::Blocked) => false,
                (Op::Restore, _) => true,
                _ => false,
            };
            return Err(if whole {
                error
            } else {
                error.wrap(Outcome::Partial(op))
            });
        }
        journal.remove_metadata(&quarantine).map_err(|e| {
            Refusal::os(
                Outcome::InfoLeft(op, recovery.clone()),
                "Trash metadata cleanup after completion",
                &e,
            )
        })?;
        journal
            .remove_empty(&quarantine_path, &quarantine, &files)
            .map_err(|e| e.wrap(Outcome::FolderLeft(op, Some(recovery.clone()))))?;
        finish(journal, op, true)
    }
}

#[cfg(test)]
#[path = "restore_batch_tests.rs"]
mod tests;
