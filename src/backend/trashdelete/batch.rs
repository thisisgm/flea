// A restore batch confirms intents before claims and completed moves before durable completion markers.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;

type Completed = Vec<(usize, Option<Op>, Recovery)>;

#[derive(Clone)]
pub(in crate::backend::trashdelete) struct Ticket {
    index: usize,
    completed: Rc<RefCell<Completed>>,
}

impl Ticket {
    pub(super) fn keep(self, recovery: Recovery, effect: Option<Op>) {
        self.completed
            .borrow_mut()
            .push((self.index, effect, recovery));
    }
}

pub(in crate::backend::trashdelete) struct Cleanup {
    path: PathBuf,
    quarantine: File,
    parent: File,
    metadata: Option<Identity>,
}

impl Cleanup {
    pub(super) fn new(
        quarantine: &File,
        parent: &File,
        metadata: Option<Identity>,
    ) -> Result<Self, Refusal> {
        Ok(Self {
            path: std::fs::read_link(format!("/proc/self/fd/{}", quarantine.as_raw_fd())).refuse(
                Outcome::RecordLeft(None),
                "Could not retain quarantine path",
            )?,
            quarantine: quarantine
                .try_clone()
                .refuse(Outcome::RecordLeft(None), "Could not retain quarantine")?,
            parent: parent
                .try_clone()
                .refuse(Outcome::RecordLeft(None), "Could not retain Trash folder")?,
            metadata,
        })
    }

    fn unchanged(&self) -> Result<(), Refusal> {
        unchanged_quarantine(&self.path, &self.quarantine)
    }

    pub(super) fn validate(&self) -> Result<(), Refusal> {
        self.unchanged()?;
        let entries = std::fs::read_dir(fd_path(&self.quarantine, OsStr::new(".")))
            .refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?;
        for entry in entries {
            let entry = entry.refuse(Outcome::RecordLeft(None), "Quarantine unreadable")?;
            if self.metadata.is_none() || entry.file_name() != "metadata" {
                return Err(left(
                    "Recovery quarantine holds unexpected items; it was preserved.",
                    "it still holds items",
                ));
            }
        }
        if let Some(expected) = &self.metadata {
            if identity(&fd_path(&self.quarantine, OsStr::new("metadata")))
                .refuse(Outcome::RecordLeft(None), "Claimed metadata unreadable")?
                != *expected
            {
                return Err(left(
                    "Claimed Trash metadata changed; it was preserved.",
                    "the Trash record changed",
                ));
            }
        }
        Ok(())
    }

    fn clear_metadata(&self) -> Result<(), Refusal> {
        self.validate()?;
        if self.metadata.is_some() {
            std::fs::remove_file(fd_path(&self.quarantine, OsStr::new("metadata"))).refuse(
                Outcome::RecordLeft(None),
                "Could not remove confirmed restore metadata",
            )?;
        }
        Ok(())
    }

    fn remove(&self) -> Result<(), Refusal> {
        self.unchanged()?;
        std::fs::remove_dir(&self.path).refuse(
            Outcome::RecordLeft(None),
            "Could not remove empty recovery quarantine",
        )?;
        if !syncs::held_open(&self.parent) {
            self.parent.sync_all().refuse(
                Outcome::RecordLeft(None),
                "Could not sync recovery quarantine removal",
            )?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(in crate::backend::trashdelete) struct Batch {
    devices: BTreeMap<u64, File>,
    records: BTreeMap<u64, File>,
    completed: Rc<RefCell<Completed>>,
}

impl Batch {
    pub fn ticket(&self, index: usize) -> Ticket {
        Ticket {
            index,
            completed: self.completed.clone(),
        }
    }

    pub fn include(&mut self, recovery: &Recovery, files: &File) -> Result<(), Refusal> {
        #[cfg(test)]
        if FAIL.with(|fail| match fail.borrow().as_deref() {
            Some("include") => true,
            Some("later include") => !self.records.is_empty(),
            _ => false,
        }) {
            return Err(Refusal::new(
                Outcome::NoRecord(Op::Restore),
                "Injected recovery batch include failure.",
            ));
        }
        for file in [recovery.record.file(), files] {
            hold(&mut self.devices, file)?;
        }
        hold(&mut self.records, recovery.record.file())
    }

    pub fn prepare(&self) -> Result<(), Refusal> {
        confirm(&self.devices, "preparation")
            .map_err(|error| error.wrap(Outcome::NoRecord(Op::Restore)))
    }

    pub fn finish(self) -> Vec<(usize, Refusal)> {
        let completed = std::mem::take(&mut *self.completed.borrow_mut());
        if completed.is_empty() {
            return Vec::new();
        }
        let mut failures = Vec::new();
        if let Err(error) = confirm(&self.devices, "completed moves") {
            return completed
                .into_iter()
                .map(|(index, effect, _)| (index, error.wrap(Outcome::RecordLeft(effect))))
                .collect();
        }
        let mut emptied = Vec::new();
        for (index, effect, mut recovery) in completed {
            let written = recovery
                .cleanup
                .take()
                .ok_or_else(|| {
                    left(
                        "Recovery has no completed quarantine.",
                        "the temporary folder is unavailable",
                    )
                })
                .and_then(|cleanup| {
                    cleanup.clear_metadata()?;
                    Ok(cleanup)
                });
            match written {
                Ok(cleanup) => emptied.push((index, effect, recovery, cleanup)),
                Err(error) => failures.push((index, error.wrap(Outcome::RecordLeft(effect)))),
            }
        }
        if let Err(error) = confirm(&self.devices, "empty quarantines") {
            failures.extend(
                emptied
                    .into_iter()
                    .map(|(index, effect, _, _)| (index, error.wrap(Outcome::RecordLeft(effect)))),
            );
            return failures;
        }
        let mut ready = Vec::new();
        for (index, effect, mut recovery, cleanup) in emptied {
            match recovery.record.append(FINISHED) {
                Ok(()) => ready.push((index, effect, recovery, cleanup)),
                Err(error) => failures.push((
                    index,
                    left(&error, "the recovery record could not be written")
                        .wrap(Outcome::RecordLeft(effect)),
                )),
            }
        }
        if let Err(error) = confirm(&self.records, "completion markers") {
            failures.extend(
                ready
                    .into_iter()
                    .map(|(index, effect, _, _)| (index, error.wrap(Outcome::RecordLeft(effect)))),
            );
            return failures;
        }
        for (index, effect, recovery, cleanup) in ready {
            let result = cleanup.remove().and_then(|()| recovery.complete());
            if let Err(error) = result {
                let outcome = match error.outcome() {
                    Outcome::RecordUnconfirmed(_) => Outcome::RecordUnconfirmed(effect),
                    _ => Outcome::RecordLeft(effect),
                };
                failures.push((index, error.wrap(outcome)));
            }
        }
        failures
    }
}

fn hold(devices: &mut BTreeMap<u64, File>, file: &File) -> Result<(), Refusal> {
    let device = file
        .metadata()
        .refuse(
            Outcome::NoRecord(Op::Restore),
            "Could not inspect recovery filesystem",
        )?
        .dev();
    if let std::collections::btree_map::Entry::Vacant(entry) = devices.entry(device) {
        entry.insert(file.try_clone().refuse(
            Outcome::NoRecord(Op::Restore),
            "Could not retain recovery filesystem",
        )?);
    }
    Ok(())
}

extern "C" {
    fn syncfs(fd: i32) -> i32;
}

fn confirm(devices: &BTreeMap<u64, File>, phase: &str) -> Result<(), Refusal> {
    #[cfg(test)]
    PROBE.with(|probe| probe.borrow_mut().push(phase.to_string()));
    #[cfg(test)]
    if FAIL.with(|fail| fail.borrow().as_deref() == Some(phase)) {
        return Err(left(
            "Injected recovery batch confirmation failure.",
            "the filesystem did not confirm the changes",
        ));
    }
    for file in devices.values() {
        // SAFETY: the descriptor is held throughout the filesystem confirmation.
        if unsafe { syncfs(file.as_raw_fd()) } != 0 {
            let error = std::io::Error::last_os_error();
            return Err(Refusal::os(
                Outcome::RecordLeft(None),
                &format!("Could not confirm restore batch {phase}"),
                &error,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    pub(in crate::backend::trashdelete) static PROBE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    pub(in crate::backend::trashdelete) static FAIL: RefCell<Option<String>> = const { RefCell::new(None) };
}
