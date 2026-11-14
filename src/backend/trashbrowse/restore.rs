// The Trash view's restore: each target is claimed and moved back to its original folder, and every folder it touched syncs once at the end.
use super::*;

impl Session {
    pub(super) fn restore(&mut self, line: &str) -> Result<String, String> {
        let mut thumbnails = ThumbnailPurge(Vec::new());
        if field_bool(line, "all") {
            self.items = list()?;
        }
        let targets = self.targets(line)?;
        // A target whose original sits on a pending mount waits, so the late write lands first.
        if self.targets_blocked(&targets)? {
            return Err("an operation is already running".into());
        }
        let syncs = crate::backend::trashdelete::FolderSyncs::open();
        let mut done = 0;
        let mut failures = Vec::new();
        let mut cursor = 0;
        loop {
            let mut chunk = Vec::new();
            for _ in 0..crate::backend::trashdelete::RESTORE_BATCH_ITEMS {
                let Some(bytes) = targets.records().next(&mut cursor)? else {
                    break;
                };
                let text = record_text(bytes)?;
                let item = record_item(&text)?;
                let retained = field_str(&text, "identity").unwrap_or_default();
                let current = detail(&item).map_err(String::from).and_then(|mut current| {
                    if !field_bool(line, "all") {
                        require_selected_identity(&current, &retained)?;
                    }
                    let backing = current
                        .backing
                        .take()
                        .ok_or_else(|| unchecked(Op::Restore))?;
                    Ok((current, backing))
                });
                let original = flea::bytename::exact(&item.original);
                chunk.push((item, retained, original, current));
            }
            if chunk.is_empty() {
                break;
            }
            let requests: Vec<_> = chunk
                .iter()
                .filter_map(|(_, _, original, current)| {
                    current
                        .as_ref()
                        .ok()
                        .map(|(_, backing)| (backing, original.as_path()))
                })
                .collect();
            let root = crate::backend::trashdelete::recovery_root();
            let results = match &root {
                Ok(root) => crate::backend::trashdelete::restore_many(&requests, root),
                Err(_) => Vec::new(),
            };
            let mut results = results.into_iter();
            for (item, mut retained, original, current) in chunk {
                let restored = current.and_then(|(mut current, backing)| {
                    if let Err(error) = &root {
                        return Err(error.clone());
                    }
                    let result = results
                        .next()
                        .ok_or_else(|| damaged("Restore batch omitted a checked item.").shown())?;
                    if let (Ok(()), Some(parent)) = (&result, original.parent()) {
                        crate::backend::trashdelete::FolderSyncs::restored(parent);
                    }
                    thumbnails.record(&backing, &result);
                    if result.is_err() {
                        current.backing = Some(backing);
                        retained = surviving_identity(&current);
                    }
                    result.map_err(String::from)
                });
                match restored {
                    Ok(_) => done += 1,
                    Err(error) => failures.push(failure(&item, &retained, &error)),
                }
            }
        }
        // Every restored item is back before the reply, and so is each folder's sync.
        drop(syncs);
        self.confirmation = None;
        self.selection = None;
        Ok(format!(
            r#""done":{},"failed":{},"failures":[{}]"#,
            done,
            failures.len(),
            failures.join(",")
        ))
    }
}
