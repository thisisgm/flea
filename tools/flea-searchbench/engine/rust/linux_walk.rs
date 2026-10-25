use rayon::Scope;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

pub const NONE: u32 = u32::MAX;
pub const KIND_FILE: u8 = 0;
pub const KIND_DIR: u8 = 1;
pub const KIND_LINK: u8 = 2;
pub const KIND_OTHER: u8 = 3;
pub const FLAG_HIDDEN: u8 = 1 << 2;
pub const FLAG_MOUNT: u8 = 1 << 3;

#[derive(Clone, Copy)]
pub struct RawEnt {
    pub name_off: u32,
    pub name_len: u16,
    pub kind: u8,
    pub size: u64,
    pub mtime: u32,
    pub child: u32,
}

#[derive(Default)]
pub struct Listing {
    pub id: u32,
    pub names: Vec<u8>,
    pub ents: Vec<RawEnt>,
}

pub fn blocked(_path: &[u8]) -> bool {
    false
}

pub fn is_mount(path: &[u8]) -> bool {
    let path = Path::new(OsStr::from_bytes(path));
    match (
        std::fs::symlink_metadata(path),
        path.parent()
            .and_then(|p| std::fs::symlink_metadata(p).ok()),
    ) {
        (Ok(meta), Some(parent)) => meta.dev() != parent.dev(),
        _ => false,
    }
}

pub fn list_one(path: &[u8]) -> Option<Listing> {
    let path = Path::new(OsStr::from_bytes(path));
    let rd = std::fs::read_dir(path).ok()?;
    let mut listing = Listing::default();
    for entry in rd {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let bytes = name.as_bytes();
        if bytes.first() == Some(&b'.') {
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path()).ok()?;
        let ft = meta.file_type();
        let mut kind = if ft.is_dir() {
            KIND_DIR
        } else if ft.is_file() {
            KIND_FILE
        } else if ft.is_symlink() {
            KIND_LINK
        } else {
            KIND_OTHER
        };
        if ft.is_dir() && is_mount(entry.path().as_os_str().as_bytes()) {
            kind |= FLAG_MOUNT;
        }
        listing.ents.push(RawEnt {
            name_off: u32::try_from(listing.names.len()).ok()?,
            name_len: u16::try_from(bytes.len()).ok()?,
            kind,
            size: meta.size(),
            mtime: meta.mtime().clamp(0, u32::MAX as i64) as u32,
            child: NONE,
        });
        listing.names.extend_from_slice(bytes);
    }
    Some(listing)
}

fn descend<'s>(
    scope: &Scope<'s>,
    path: Vec<u8>,
    id: u32,
    next: &'s AtomicU32,
    out: &'s Mutex<Vec<Listing>>,
) {
    let Some(mut listing) = list_one(&path) else {
        return;
    };
    listing.id = id;
    for entry in &mut listing.ents {
        if entry.kind & 3 == KIND_DIR && entry.kind & FLAG_MOUNT == 0 {
            let child_id = next.fetch_add(1, Ordering::Relaxed);
            entry.child = child_id;
            let mut child_path = path.clone();
            if child_path.last() != Some(&b'/') {
                child_path.push(b'/');
            }
            child_path.extend_from_slice(
                &listing.names[entry.name_off as usize..][..entry.name_len as usize],
            );
            scope.spawn(move |s| descend(s, child_path, child_id, next, out));
        }
    }
    out.lock().unwrap().push(listing);
}

pub fn scan(root: &[u8], threads: usize) -> Vec<Listing> {
    let out = Mutex::new(Vec::new());
    let next = AtomicU32::new(1);
    let budget = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.min(budget).max(1))
        .build()
        .unwrap();
    pool.install(|| rayon::scope(|s| descend(s, root.to_vec(), 0, &next, &out)));
    out.into_inner().unwrap()
}

pub fn rooted_scan(root: &Path, threads: usize) -> Vec<Listing> {
    let components: Vec<Vec<u8>> = root
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(name) => Some(name.as_bytes().to_vec()),
            _ => None,
        })
        .collect();
    let shift = components.len() as u32;
    let mut listings = scan(root.as_os_str().as_bytes(), threads);
    for listing in &mut listings {
        listing.id += shift;
        for entry in &mut listing.ents {
            if entry.child != NONE {
                entry.child += shift;
            }
        }
    }
    for (id, name) in components.into_iter().enumerate() {
        listings.push(Listing {
            id: id as u32,
            ents: vec![RawEnt {
                name_off: 0,
                name_len: name.len() as u16,
                kind: KIND_DIR,
                size: 0,
                mtime: 0,
                child: id as u32 + 1,
            }],
            names: name,
        });
    }
    listings
}
