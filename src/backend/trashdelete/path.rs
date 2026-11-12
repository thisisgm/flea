use super::*;
#[path = "directory.rs"]
mod directory;

// Ordinary-path deletion shares the complete tree review; only Trash requires paired .trashinfo claims.
pub struct PathReview {
    path: PathBuf,
    payload: Identity,
    tree: Records,
    pub(super) held: Option<Box<super::file_claim::FileClaim>>,
}
impl PathReview {
    pub fn prepare(
        path: PathBuf,
        manifest: &mut Manifest,
        scratch: &Path,
        cancel: &Cancellation,
    ) -> Result<(Self, u64), Refusal> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(Refusal::changed(
                "Permanent deletion requires an absolute item path; filesystem roots are refused.",
            )
            .logged());
        }
        let payload = identity(&path)
            .changed("Item unreadable")
            .map_err(Refusal::logged)?;
        let held = super::file_claim::FileClaim::for_review(&path, &payload)
            .map_err(Refusal::logged)?
            .map(Box::new);
        let (tree, bytes, _) =
            snapshot_tree(&path, &payload, manifest, scratch, cancel).map_err(Refusal::logged)?;
        Ok((
            Self {
                path,
                payload,
                tree,
                held,
            },
            bytes,
        ))
    }
    pub fn unchanged(&self, cancel: &Cancellation) -> Result<bool, Refusal> {
        if self.held.as_ref().is_some_and(|held| !held.unchanged())
            || identity(&self.path).ok().as_ref() != Some(&self.payload)
        {
            return Ok(false);
        }
        (|| {
            let parent = open_dir(self.path.parent().changed("Missing deletion parent.")?)?;
            matches(
                &self.tree,
                &parent,
                self.path
                    .file_name()
                    .changed("Missing deletion filename.")?,
                cancel,
            )
        })()
        .map_err(Refusal::logged)
    }
    pub fn delete(&self, recovery_root: &Path) -> Result<(), Refusal> {
        self.delete_after(recovery_root, || {})
    }
    pub(super) fn delete_after(
        &self,
        recovery_root: &Path,
        before_claim: impl FnOnce(),
    ) -> Result<(), Refusal> {
        self.removed(recovery_root, before_claim)
            .map_err(Refusal::logged)
    }
    fn removed(&self, recovery_root: &Path, before_claim: impl FnOnce()) -> Result<(), Refusal> {
        let op = Op::Delete;
        let parent_path = self.path.parent().changed("Missing deletion parent.")?;
        let parent = open_dir(parent_path)?;
        let name = self
            .path
            .file_name()
            .changed("Missing deletion filename.")?;
        if identity(&fd_path(&parent, name)).changed("Item unreadable")? != self.payload {
            return Err(Refusal::new(
                Outcome::ItemChanged,
                "Item changed; confirm again.",
            ));
        }
        let directory = self.payload.directory() && super::file_claim::retains(parent_path);
        let held = if let Some(held) = &self.held {
            Some(held.duplicate()?)
        } else if directory {
            Some(super::file_claim::FileClaim::open_at(
                &parent,
                name,
                &self.payload,
            )?)
        } else if self.payload.directory() {
            None
        } else {
            Some(super::file_claim::FileClaim::open(
                &parent,
                name,
                &self.payload,
            )?)
        };
        let quarantine_name = format!(
            ".flea-delete-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let quarantine_path = fd_path(&parent, OsStr::new(&quarantine_name));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&quarantine_path)
            .map_err(|e| {
                Refusal::os(
                    Outcome::Prepare(op),
                    "Could not create deletion quarantine",
                    &e,
                )
            })?;
        let quarantine =
            open_dir(&quarantine_path).map_err(|refusal| refusal.wrap(Outcome::Prepare(op)))?;
        let recovery = parent_path.join(&quarantine_name);
        let mut journal = match Recovery::begin(
            recovery_root,
            recovery::RecordInput {
                source: &self.path,
                parent: &parent,
                payload: &self.payload,
                paired: None,
                quarantine_path: &recovery,
                quarantine: &quarantine,
                tree: Some(&self.tree),
                destination: None,
            },
        ) {
            Ok(journal) => journal,
            Err(error) => {
                let _ = std::fs::remove_dir(&quarantine_path);
                return Err(error.wrap(Outcome::NoRecord(op)));
            }
        };
        let removal = if directory {
            held.as_ref()
                .ok_or_else(super::file_claim::changed)
                .and_then(|held| {
                    self.remove_directory(&parent, name, &quarantine, held, before_claim)
                })
        } else {
            before_claim();
            if let Err(error) = rename(&parent, name, &quarantine, OsStr::new("payload")) {
                match journal.remove_empty(&quarantine_path, &quarantine, &parent) {
                    Ok(()) => settle(journal, op),
                    Err(left) => {
                        left.logged();
                    }
                }
                return Err(Refusal::new(
                    Outcome::Prepare(op),
                    format!("Could not claim item for deletion: {}", error),
                )
                .because(error));
            }
            match held {
                Some(held) => held.remove(&quarantine),
                None => remove_tree(
                    &self.tree,
                    &quarantine,
                    Refusal::new(
                        Outcome::ItemChanged,
                        "Item changed after confirmation; no deletion attempted.",
                    ),
                ),
            }
        };
        if let Err(error) = removal {
            if !directory && rename(&quarantine, OsStr::new("payload"), &parent, name).is_err() {
                return Err(error.wrap(Outcome::Stuck(op, recovery)));
            }
            if journal
                .remove_empty(&quarantine_path, &quarantine, &parent)
                .is_err()
            {
                return Err(error.wrap(if directory {
                    Outcome::Stuck(op, recovery)
                } else {
                    Outcome::StuckAtOriginal(recovery)
                }));
            }
            settle(journal, op);
            return Err(if directory || error.outcome() == &Outcome::ItemChanged {
                error
            } else {
                error.wrap(Outcome::PartialAtOriginal)
            });
        }
        journal
            .remove_empty(&quarantine_path, &quarantine, &parent)
            .map_err(|e| e.wrap(Outcome::FolderLeft(op, Some(recovery.clone()))))?;
        finish(journal, op, true)
    }
}
