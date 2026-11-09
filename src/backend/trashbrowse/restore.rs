// The Trash view's restore: each target is claimed and moved back to its original folder, and every folder it touched syncs once at the end.
use super::*;

impl Session {
    pub(super) fn restore(&mut self, line: &str) -> Result<String, String> {
        let mut thumbnails = ThumbnailPurge(Vec::new());
        if field_bool(line, "all") { self.items = list()?; }
        let targets = self.targets(line)?;
        // A target whose original sits on a pending mount waits, so the late write lands first.
        if self.targets_blocked(&targets)? {
            return Err("an operation is already running".into());
        }
        let syncs = crate::backend::trashdelete::FolderSyncs::open();
        let mut done = 0;
        let mut failures = Vec::new();
        let mut cursor = 0;
        while let Some(bytes) = targets.records().next(&mut cursor)? {
            let text = record_text(bytes)?;
            let item = record_item(&text)?;
            let mut retained = field_str(&text, "identity").unwrap_or_default();
            let restored = detail(&item).map_err(String::from).and_then(|current| {
                if !field_bool(line, "all") {
                    require_selected_identity(&current, &field_str(&text, "identity").unwrap_or_default())?;
                }
                let backing = current.backing.as_ref().ok_or_else(|| unchecked(Op::Restore))?;
                let original = flea::bytename::exact(&item.original);
                let result =
                    backing.restore(&original, &crate::backend::trashdelete::recovery_root()?);
                if let (Ok(()), Some(parent)) = (&result, original.parent()) {
                    crate::backend::trashdelete::FolderSyncs::restored(parent);
                }
                thumbnails.record(backing, &result);
                let result = result.map_err(String::from);
                if result.is_err() { retained = surviving_identity(&current); }
                result
            });
            match restored {
                Ok(_) => done += 1,
                Err(error) => failures.push(failure(&item, &retained, &error)),
            }
        }
        // Every restored item is back before the reply, and so is each folder's sync.
        drop(syncs);
        self.confirmation = None;
        self.selection = None;
        Ok(format!(r#""done":{},"failed":{},"failures":[{}]"#, done, failures.len(), failures.join(",")))
    }
}
