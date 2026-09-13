// Permanent deletion reuses the disk-backed tree review and durable recovery path used by Trash.
use super::menu_actions::Selected;
use super::trashdelete::PathReview;
use super::trashmanifest::{Cancellation, Manifest};
use crate::json::escape;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(1);

struct Root {
    review: PathReview,
    path: PathBuf,
    selected: usize,
}

fn check_cancel(cancel: &Cancellation) -> Result<(), String> {
    cancel.check().map_err(|_| "Permanent deletion cancelled.".into())
}

pub(crate) struct Review {
    pub token: usize,
    pub count: usize,
    pub bytes: u64,
    roots: Vec<Root>,
    selection: Vec<Selected>,
}

impl Review {
    pub fn prepare(items: &[Selected], scratch: &Path, cancel: &Cancellation) -> Result<Self, String> {
        let mut selected: Vec<_> = items.iter().collect();
        selected.sort_by(|a, b| a.path.cmp(&b.path));
        selected.dedup_by(|a, b| a.path == b.path);
        if selected.is_empty() { return Err("No items were selected for permanent deletion.".into()); }
        let mut manifest = Manifest::new(scratch)?;
        let mut roots: Vec<Root> = Vec::new();
        let mut covered: Option<PathBuf> = None;
        let mut bytes = 0u64;
        for item in &selected {
            check_cancel(cancel)?;
            let metadata = item.current()?;
            if covered.as_ref().is_some_and(|parent| item.path.starts_with(parent)) {
                roots.last_mut().unwrap().selected += 1;
                continue;
            }
            covered = metadata.is_dir().then(|| item.path.clone());
            let (review, size) = PathReview::prepare(item.path.clone(), &mut manifest, scratch, cancel)?;
            item.current()?;
            bytes = bytes.checked_add(size).ok_or("The selected byte total exceeds the supported range.")?;
            roots.push(Root { review, path: item.path.clone(), selected: 1 });
        }
        for item in &selected { check_cancel(cancel)?; item.current()?; }
        let selection = selected.into_iter().cloned().collect::<Vec<_>>();
        let review = Self { token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed), count: selection.len(), bytes, roots, selection };
        review.validate(cancel)?;
        Ok(review)
    }

    pub fn validate(&self, cancel: &Cancellation) -> Result<(), String> {
        for root in &self.roots {
            check_cancel(cancel)?;
            if !root.review.unchanged(cancel)? {
                return Err("Selected items changed; review a fresh deletion confirmation.".into());
            }
        }
        Ok(())
    }

    pub fn delete(&self, recovery: &Path, cancel: &Cancellation) -> Result<String, String> {
        self.validate(cancel)?;
        let mut deleted = 0;
        let mut failed = 0;
        let mut first_error = String::new();
        let mut cancelled = false;
        for root in &self.roots {
            if cancel.check().is_err() { cancelled = true; break; }
            // A claimed root completes or restores its survivors before cancellation stops the next root.
            match root.review.delete(recovery) {
                Ok(()) => deleted += root.selected,
                Err(error) => {
                    failed += root.selected;
                    if first_error.is_empty() { first_error = format!("{}: {}", root.path.display(), error); }
                }
            }
        }
        let error = if failed > 0 {
            format!("{} selected items were not completely deleted. First failure: {}", failed, first_error)
        } else { String::new() };
        Ok(format!(r#""deleted":{},"failed":{},"cancelled":{},"error":"{}","remaining":[{}]"#,
            deleted, failed, cancelled, escape(&error), self.remaining()))
    }

    fn remaining(&self) -> String {
        self.selection.iter().filter(|item| item.current().is_ok())
            .map(|item| format!(r#""{}""#, escape(&item.path.to_string_lossy()))).collect::<Vec<_>>().join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::symlink;

    fn selected(path: &Path) -> Selected { Selected::inspect(path.to_str().unwrap()).unwrap() }
    fn deletion_sandbox(d: &TestDir, paths: &[&Path]) {
        assert!(!d.path().as_os_str().is_empty() && d.path().is_absolute());
        assert!(d.path().join(".flea-test-sandbox").is_file());
        for path in paths { assert!(!path.as_os_str().is_empty() && path.is_absolute() && path.starts_with(d.path())); }
    }

    #[test]
    fn confirmed_nested_selection_deletes_once_and_never_follows_links() {
        let d = TestDir::new("menu-delete");
        let folder = d.dir("folder");
        let child = d.file("folder/child", "payload");
        let target = d.file("target", "untouched");
        let link = d.join("link");
        symlink(&target, &link).unwrap();
        let review = Review::prepare(&[selected(&child), selected(&folder), selected(&link), selected(&child)], d.path(), &Cancellation::default()).unwrap();
        assert_eq!(review.count, 3);
        assert_eq!(review.roots.len(), 2);
        assert_eq!(review.bytes, 7 + link.symlink_metadata().unwrap().len());
        let recovery = d.join("recovery");
        deletion_sandbox(&d, &[&folder, &child, &link, &recovery]);
        let result = review.delete(&recovery, &Cancellation::default()).unwrap();
        assert!(result.contains(r#""deleted":3,"failed":0,"cancelled":false"#));
        assert!(!folder.exists());
        assert!(link.symlink_metadata().is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched");
        assert!(result.contains(r#""remaining":[]"#));
    }

    #[test]
    fn changed_descendants_and_cancelled_reviews_refuse_before_deletion() {
        let d = TestDir::new("menu-delete-refusal");
        let folder = d.dir("folder");
        let child = d.file("folder/child", "original");
        let review = Review::prepare(&[selected(&folder)], d.path(), &Cancellation::default()).unwrap();
        d.file("folder/new-arrival", "preserved");
        let recovery = d.join("recovery");
        deletion_sandbox(&d, &[&folder, &child, &recovery]);
        assert!(review.delete(&recovery, &Cancellation::default()).unwrap_err().contains("changed"));
        assert_eq!(std::fs::read_to_string(&child).unwrap(), "original");
        let cancel = Cancellation::default();
        let current = Review::prepare(&[selected(&folder)], d.path(), &cancel).unwrap();
        cancel.next();
        assert!(current.delete(&recovery, &cancel).unwrap_err().contains("cancelled"));
        assert!(folder.join("new-arrival").exists());
    }

    #[test]
    fn surviving_selection_never_reports_a_replacement_identity() {
        let d = TestDir::new("menu-delete-survivors");
        let first = d.file("first", "original");
        let kept = d.file("kept", "survivor");
        let review = Review::prepare(&[selected(&first), selected(&kept)], d.path(), &Cancellation::default()).unwrap();
        let moved = d.join("first-moved");
        deletion_sandbox(&d, &[&first, &kept, &moved]);
        std::fs::rename(&first, &moved).unwrap();
        d.file("first", "replacement");
        assert_eq!(review.remaining(), format!(r#""{}""#, escape(kept.to_str().unwrap())));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "replacement");
    }
}
