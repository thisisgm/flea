// Mount Trash entries use the existing rows and reviewed restore/delete machinery.
use super::*;
use std::os::unix::fs::MetadataExt;

// Keep freeform original paths out of GIO's space-joined metadata column.
pub(super) fn metadata(
    read: &impl Fn(&[&str]) -> Result<String, Refusal>,
) -> Result<String, Refusal> {
    read(&[
        "list",
        "-u",
        "-h",
        "-a",
        "standard::target-uri,trash::deletion-date",
        "trash:///",
    ])
    .or_else(|_| {
        read(&[
            "list",
            "-u",
            "-h",
            "-a",
            "standard::target-uri",
            "trash:///",
        ])
    })
}

// Sample gio line: "trash:///a.txt\t1\t(regular)\tstandard::target-uri=file:///mnt/.Trash-1000/files/a.txt trash::deletion-date=2026-10-18T09:00:00".
pub(super) fn merge(items: &mut Vec<Item>, dates: &mut HashMap<String, String>, gio_text: &str) {
    let mut backings = HashSet::new();
    for line in gio_text.lines() {
        if let Some(path) = line
            .split_ascii_whitespace()
            .find_map(|field| field.strip_prefix("standard::target-uri=file://"))
        {
            backings.insert(PathBuf::from(
                <std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(
                    crate::paths::percent_decode_bytes(path),
                ),
            ));
        }
    }
    for record in super::super::topdirtrash::list() {
        if backings.insert(record.path) {
            dates.insert(record.entry.uri.clone(), record.deleted);
            items.push(Item {
                uri: record.entry.uri,
                original: record.entry.original.exact_text().into_owned(),
            });
        }
    }
}

pub(super) fn detail(item: &Item) -> Result<Detail, Refusal> {
    detail_with(&super::super::topdirtrash::resolver(), item)
}

pub(super) fn detail_with(
    resolve: &impl Fn(&str) -> Option<super::super::topdirtrash::Record>,
    item: &Item,
) -> Result<Detail, Refusal> {
    let record = resolve(&item.uri)
        .ok_or_else(|| damaged("The mount Trash entry is no longer available."))?;
    if record.entry.original.exact_text() != item.original {
        return Err(damaged(
            "The mount Trash entry's original location changed.",
        ));
    }
    let meta = record
        .path
        .symlink_metadata()
        .map_err(|error| damaged(&error.to_string()))?;
    let identity = format!("l{}:{}", meta.dev(), meta.ino());
    let backing = record.backing;
    if backing.provider_identity() != identity || !backing.unchanged() {
        return Err(damaged("The mount Trash entry changed while reading it."));
    }
    let directory = meta.is_dir();
    static TYPES: std::sync::LazyLock<(crate::backend::mime::Db, crate::backend::icons::Names)> =
        std::sync::LazyLock::new(|| {
            (
                crate::backend::mime::Db::load(),
                crate::backend::icons::Names::load(),
            )
        });
    let (types, icons) = &*TYPES;
    let name = record
        .entry
        .original
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let icon = icons
        .icon_for(types.lookup(&name), directory, meta.mode())
        .to_string();
    Ok(Detail {
        item: item.clone(),
        size: meta.len(),
        deleted: record.deleted,
        identity,
        directory,
        icon,
        backing: Some(backing),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{testdir::TestDir, topdirtrash};

    #[test]
    fn mount_entries_merge_with_home_rows_and_deduplicate_a_gio_visible_backing() {
        let d = TestDir::new("trashbrowse-topdir-merge");
        d.dir("mount");
        topdirtrash::test_set_mount_body(Some(format!(
            "1 0 1:1 /sub {} rw - btrfs /dev/a rw\n",
            d.join("mount").display()
        )));
        let original = d.file("mount/same.txt", "mount bytes");
        let entry = topdirtrash::rename_to_topdir(
            &mut topdirtrash::Batch::read(),
            &original,
            &mut Default::default(),
        )
        .unwrap()
        .unwrap();
        let home = Item {
            uri: "trash:///.flea-topdir-notes.txt".into(),
            original: d.join("home/same.txt").exact_text().into_owned(),
        };
        assert!(
            !home.uri.starts_with(topdirtrash::PREFIX),
            "a home filename cannot enter the mount URI namespace"
        );
        let mut items = vec![home.clone()];
        let mut dates = HashMap::new();
        merge(&mut items, &mut dates, "");
        assert_eq!(items.len(), 2);
        assert_ne!(items[0].uri, items[1].uri);
        let mounted = items.pop().unwrap();
        let row = detail(&mounted).unwrap();
        assert_eq!(row.size, 11);
        assert_eq!(row.item.original, original.exact_text());
        assert!(!selection_identity(&row).is_empty());
        let mut rows = details::many(std::slice::from_ref(&mounted)).unwrap();
        assert_eq!(rows.pop().unwrap().item, mounted);
        let record = topdirtrash::resolve(&entry.uri).unwrap();
        let visible = format!(
            "trash:///gio-mount\t1\t(regular)\tstandard::target-uri=file://{} trash::deletion-date=2026-11-10T00:00:00\n",
            crate::backend::hometrash::percent_encode(record.path.as_os_str().as_encoded_bytes())
        );
        items.push(Item {
            uri: "trash:///gio-mount".into(),
            original: mounted.original,
        });
        merge(&mut items, &mut dates, &visible);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], home);
        topdirtrash::test_set_mount_body(None);
    }

    #[test]
    fn empty_trash_sweeps_an_orphan_record_in_a_mount_trash_and_keeps_listed_entries() {
        use crate::backend::trashmanifest::{Cancellation, Manifest};
        let d = TestDir::new("trashbrowse-topdir-sweep");
        d.dir("mount");
        topdirtrash::test_set_mount_body(Some(format!(
            "1 0 1:1 /sub {} rw - btrfs /dev/a rw\n",
            d.join("mount").display()
        )));
        let original = d.file("mount/kept.txt", "mount bytes");
        topdirtrash::rename_to_topdir(
            &mut topdirtrash::Batch::read(),
            &original,
            &mut Default::default(),
        )
        .unwrap()
        .unwrap();
        let (mut items, mut dates) = (Vec::new(), HashMap::new());
        merge(&mut items, &mut dates, "");
        let store = d.join("mount").join(format!(
            ".Trash-{}",
            crate::backend::manifestdir::current_uid()
        ));
        let orphan = store.join("info/gone.txt.trashinfo");
        std::fs::write(
            &orphan,
            "[Trash Info]\nPath=gone.txt\nDeletionDate=2026-11-10T00:00:00\n",
        )
        .unwrap();
        let (scratch, recovery) = (d.dir("scratch"), d.join("recovery"));
        let mut trees = Manifest::new(&scratch).unwrap();
        let sweep = super::super::sweep::Sweep::review(
            &items,
            &mut trees,
            &scratch,
            &Cancellation::default(),
            |_| Ok(String::new()),
        )
        .unwrap();
        let left = sweep.remove(
            &recovery,
            &Cancellation::default(),
            &mut ThumbnailPurge(Vec::new()),
        );
        assert_eq!(left, 0);
        assert!(
            orphan.symlink_metadata().is_err(),
            "the mount Trash's orphan record is swept"
        );
        assert!(
            store.join("files/kept.txt").is_file()
                && store.join("info/kept.txt.trashinfo").is_file()
        );
        topdirtrash::test_set_mount_body(None);
    }
}
