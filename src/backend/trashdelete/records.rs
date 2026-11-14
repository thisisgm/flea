// Per-item recovery records keep their version-1 format in single and batched restores.
use super::*;

pub(in crate::backend::trashdelete) struct RecordInput<'a> {
    pub(in crate::backend::trashdelete) source: &'a Path,
    pub(in crate::backend::trashdelete) parent: &'a File,
    pub(in crate::backend::trashdelete) payload: &'a Identity,
    pub(in crate::backend::trashdelete) paired: Option<(&'a Path, &'a Identity, &'a File)>,
    pub(in crate::backend::trashdelete) quarantine_path: &'a Path,
    pub(in crate::backend::trashdelete) quarantine: &'a File,
    pub(in crate::backend::trashdelete) tree: Option<&'a Records>,
    pub(in crate::backend::trashdelete) destination: Option<&'a Path>,
}

fn sync_record(record: &Manifest) -> Result<(), String> {
    #[cfg(test)]
    ITEM_BARRIERS.with(|count| count.set(count.get() + 1));
    record.sync()
}

impl Recovery {
    pub(in crate::backend::trashdelete) fn begin(
        root: &Path,
        input: RecordInput<'_>,
    ) -> Result<Self, Refusal> {
        Self::prepare(root, input, None)
    }
    pub(in crate::backend::trashdelete) fn prepare(
        root: &Path,
        input: RecordInput<'_>,
        batch: Option<batch::Ticket>,
    ) -> Result<Self, Refusal> {
        let RecordInput {
            source,
            parent,
            payload,
            paired,
            quarantine_path,
            quarantine,
            tree,
            destination,
        } = input;
        let metadata_identity = paired.map(|(_, identity, _)| identity.clone());
        if !root.is_absolute() {
            return Err(unkept("Recovery storage requires an absolute folder.")
                .because("the recovery folder is not valid"));
        }
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|e| {
                Refusal::os(
                    Outcome::NoRecord(Op::Delete),
                    &format!("Could not create recovery folder {}", root.display()),
                    &e,
                )
            })?;
        let directory = open_dir(root)?;
        let name = quarantine_path
            .file_name()
            .ok_or_else(|| unkept("Missing quarantine name."))?
            .to_string_lossy();
        let path = root.join(format!("{}.review", name));
        let mut record = written(
            Manifest::create(&path),
            "Could not start the recovery record",
        )?;
        let (info, metadata, info_parent) = match paired {
            Some((path, identity, parent)) => (
                path.exact_text().into_owned(),
                identity.saved(),
                Identity::of(&parent.metadata().refuse(
                    Outcome::NoRecord(Op::Delete),
                    "Trash metadata folder unreadable",
                )?)
                .saved(),
            ),
            None => Default::default(),
        };
        let text = format!(
            r#"{{"version":1,"source":"{}","parent":"{}","payload":"{}","info":"{}","metadata":"{}","infoParent":"{}","quarantine":"{}","quarantineIdentity":"{}","destination":"{}"}}"#,
            escape(&source.exact_text()),
            Identity::of(
                &parent
                    .metadata()
                    .refuse(Outcome::NoRecord(Op::Delete), "Trash folder unreadable")?
            )
            .saved(),
            payload.saved(),
            escape(&info),
            metadata,
            info_parent,
            escape(&quarantine_path.exact_text()),
            Identity::of(
                &quarantine
                    .metadata()
                    .refuse(Outcome::NoRecord(Op::Delete), "Quarantine unreadable")?
            )
            .saved(),
            escape(
                destination
                    .map(|path| path.exact_text().into_owned())
                    .unwrap_or_default()
                    .as_str()
            )
        );
        written(
            record.append(text.as_bytes()),
            "Could not write the recovery record",
        )?;
        if let Some(tree) = tree {
            let mut offset = tree.start();
            while let Some(bytes) =
                written(tree.next(&mut offset), "Could not read the Trash review")?
            {
                written(record.append(&bytes), "Could not write the recovery record")?;
            }
        }
        if batch.is_none() {
            written(sync_record(&record), "Could not sync the recovery record")?;
            directory.sync_all().map_err(|e| {
                Refusal::os(
                    Outcome::NoRecord(Op::Delete),
                    "Could not sync recovery folder",
                    &e,
                )
            })?;
        }
        Ok(Self {
            path,
            record,
            batch,
            cleanup: None,
            metadata: metadata_identity,
            metadata_pending: false,
        })
    }
    pub(in crate::backend::trashdelete) fn unbatched(mut self) -> Self {
        self.batch = None;
        self
    }
    pub(in crate::backend::trashdelete) fn complete(self) -> Result<(), Refusal> {
        self.complete_with(None)
    }
    pub(in crate::backend::trashdelete) fn complete_with(
        mut self,
        effect: Option<Op>,
    ) -> Result<(), Refusal> {
        if let Some(ticket) = self.batch.take() {
            ticket.keep(self, effect);
            return Ok(());
        }
        if identity(&self.path).refuse(Outcome::RecordLeft(None), "Recovery record unreadable")?
            != Identity::of(
                &self
                    .record
                    .file()
                    .metadata()
                    .refuse(Outcome::RecordLeft(None), "Recovery record unreadable")?,
            )
        {
            return Err(left(
                "Recovery record changed; it was preserved.",
                "the record changed",
            ));
        }
        std::fs::remove_file(&self.path).map_err(|e| {
            Refusal::os(
                Outcome::RecordLeft(None),
                &format!("Could not finish recovery record {}", self.path.display()),
                &e,
            )
        })?;
        let folder = self
            .path
            .parent()
            .refuse(Outcome::RecordUnconfirmed(None), "Missing recovery folder.")?;
        // An open restore batch syncs the folder once at its close; the record's removal is only cleanup.
        if syncs::held(folder) {
            return Ok(());
        }
        open_dir(folder)
            .map_err(|refusal| refusal.wrap(Outcome::RecordUnconfirmed(None)))?
            .sync_all()
            .map_err(|e| {
                Refusal::os(
                    Outcome::RecordUnconfirmed(None),
                    "Could not sync completed recovery record",
                    &e,
                )
            })
    }
    // A batch keeps the paired record until the payload's destination is durable.
    pub(in crate::backend::trashdelete) fn remove_metadata(
        &mut self,
        quarantine: &File,
    ) -> std::io::Result<()> {
        if self.batch.is_some() {
            self.metadata_pending = true;
            return Ok(());
        }
        std::fs::remove_file(fd_path(quarantine, OsStr::new("metadata")))
    }
    pub(in crate::backend::trashdelete) fn remove_empty(
        &mut self,
        path: &Path,
        quarantine: &File,
        parent: &File,
    ) -> Result<(), Refusal> {
        let cleanup =
            |what: &str, e: &std::io::Error| Refusal::os(Outcome::RecordLeft(None), what, e);
        unchanged_quarantine(path, quarantine)?;
        if self.batch.is_some() {
            let metadata = if self.metadata_pending {
                Some(self.metadata.clone().refuse(
                    Outcome::RecordLeft(None),
                    "Missing claimed metadata identity",
                )?)
            } else {
                None
            };
            let deferred = batch::Cleanup::new(quarantine, parent, metadata)?;
            deferred.validate()?;
            self.cleanup = Some(deferred);
            return Ok(());
        }
        let mut entries = std::fs::read_dir(fd_path(quarantine, OsStr::new(".")))
            .refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?;
        if let Some(entry) = entries.next() {
            entry.refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?;
            return Err(Refusal::new(
                Outcome::Held(path.to_path_buf()),
                format!(
                    "Recovery data remains at {}; no cleanup was recorded.",
                    path.display()
                ),
            )
            .because("it still holds items"));
        }
        quarantine
            .sync_all()
            .map_err(|e| cleanup("Could not sync empty recovery quarantine", &e))?;
        self.record
            .append(FINISHED)
            .and_then(|_| sync_record(&self.record))
            .map_err(|e| left(&e, "the recovery record could not be written"))?;
        std::fs::remove_dir(path).map_err(|e| {
            cleanup(
                &format!(
                    "Could not remove empty recovery quarantine {}",
                    path.display()
                ),
                &e,
            )
        })?;
        if syncs::held_open(parent) {
            return Ok(());
        }
        parent
            .sync_all()
            .map_err(|e| cleanup("Could not sync recovery quarantine removal", &e))
    }
}
