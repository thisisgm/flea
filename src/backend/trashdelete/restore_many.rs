// Restore all claims in bounded batches, each confirmed once per phase.
use super::*;

// One batch confirmation covers at most this many claims.
pub const RESTORE_BATCH_ITEMS: usize = 64;

impl Prepared {
    // An unconfirmed batch moved nothing, so the claim goes the way a single refused claim does.
    fn abandon(self, op: Op) {
        let Prepared {
            files,
            quarantine,
            recovery,
            journal,
            ..
        } = self;
        let mut journal = journal.unbatched();
        let Some(name) = recovery.file_name() else {
            return;
        };
        match journal.remove_empty(&fd_path(&files, name), &quarantine, &files) {
            Ok(()) => settle(journal, op),
            Err(left) => {
                left.logged();
            }
        }
    }
}

// The bounded caller retains each reviewed identity until its batch has completed cleanup.
pub fn restore_many(
    items: &[(&Reviewed, &Path)],
    recovery_root: &Path,
) -> Vec<Result<(), Refusal>> {
    if let [(item, original)] = items {
        return vec![item.restore(original, recovery_root)];
    }
    let mut results = Vec::with_capacity(items.len());
    for chunk in items.chunks(RESTORE_BATCH_ITEMS) {
        let mut batch = recovery::batch::Batch::default();
        let prepared: Vec<_> = chunk
            .iter()
            .enumerate()
            .map(|(index, (item, original))| {
                let prepared = item.prepare_claim(
                    Op::Restore,
                    recovery_root,
                    Some(original),
                    Some(batch.ticket(index)),
                )?;
                match batch.include(&prepared.journal, &prepared.files) {
                    Ok(()) => Ok(prepared),
                    Err(error) => {
                        prepared.abandon(Op::Restore);
                        Err(error)
                    }
                }
            })
            .collect();
        let confirmation = batch.prepare();
        let mut current: Vec<_> = chunk
            .iter()
            .zip(prepared)
            .map(|((item, original), prepared)| {
                prepared
                    .and_then(|prepared| match &confirmation {
                        Ok(()) => item.restore_prepared(original, prepared, || {}),
                        Err(error) => {
                            prepared.abandon(Op::Restore);
                            Err(error.clone())
                        }
                    })
                    .map_err(Refusal::logged)
            })
            .collect();
        for (index, error) in batch.finish() {
            let error = error.logged();
            if current[index].is_ok() {
                current[index] = Err(error);
            }
        }
        results.extend(current);
    }
    results
}
