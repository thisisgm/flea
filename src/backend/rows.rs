use crate::backend::aliases::Aliases;
use crate::backend::appicon;
use crate::backend::extclass;
use crate::backend::gvfsroot::gvfs_class;
use crate::backend::icons::{Names, ANY_EXECUTE_BIT};
use crate::backend::kind::Kinds;
use crate::backend::listing::Listing;
use crate::backend::meta::{thumbnailable, Meta};
use crate::backend::mime::Db;
use crate::backend::mountinfo;
use crate::backend::tags;
use crate::backend::thumbspec::Thumbnailers;
use crate::json::escape;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// A row is a short name, two flags, three numbers and an icon name, so this rarely reallocates.
const ROW_BYTES_ESTIMATE: usize = 128;

thread_local! {
    // The last window base's mount table, its link-resolved path and its class.
    static BASE_CLASS: RefCell<Option<(String, PathBuf, &'static str)>> = const { RefCell::new(None) };
}

// The link walk runs every answer, so a retargeted link or a new mount table classes afresh; an unchanged base skips statfs and sysfs.
fn base_class(base: &Path, body: &str) -> &'static str {
    if let Some(class) = extclass::early_class(base, body) {
        return class;
    }
    let owned = extclass::resolved_in(base, body);
    BASE_CLASS.with(|last| {
        let mut last = last.borrow_mut();
        if let Some((seen, at, class)) = &*last {
            if seen == body && *at == owned {
                return *class;
            }
        }
        #[cfg(test)]
        class_tests::CLASSED.with(|classed| classed.set(classed.get() + 1));
        let class = extclass::resolved_class(&owned, body);
        *last = Some((body.to_string(), owned, class));
        class
    })
}

pub(crate) fn empty_line(start: usize, ms: f64) -> String {
    format!(r#"{{"t":"rows","start":{start},"rows":[],"kinds":[],"ms":{ms:.3}}}"#)
}

// The rows a window asks for: the first index, and the cost of their stat pass in milliseconds.
pub(crate) struct Window {
    pub start: usize,
    pub ms: f64,
}

// The tables every row reads its type, icon and thumbnail from.
pub(crate) struct Services<'a> {
    pub mime: &'a Db,
    pub icons: &'a Names,
    pub aliases: &'a Aliases,
    pub thumbs: &'a Thumbnailers,
    pub kinds: &'a mut Kinds,
}

pub fn rows_line(
    base: &Path,
    l: &Listing,
    metas: &[Meta],
    window: Window,
    services: Services<'_>,
) -> String {
    // A gvfs path names its own class and reads no mount table; every other base reads it once, here.
    let body = if gvfs_class(base).is_some() { String::new() } else { mountinfo::read_body() };
    rows_line_in(base, &body, l, metas, window, services)
}

// The window against a mount table the caller read, so a test can stand an exFAT or btrfs mount where it cannot make one.
fn rows_line_in(
    base: &Path,
    body: &str,
    l: &Listing,
    metas: &[Meta],
    window: Window,
    services: Services<'_>,
) -> String {
    rows_line_body(base, body, l, metas, window, services, None)
}

pub(crate) fn rows_line_cached(
    base: &Path, body: &str, l: &Listing, metas: &[Meta], window: Window, services: Services<'_>,
    cache: &mut super::searchrowcache::Cache,
) -> String {
    rows_line_body(base, body, l, metas, window, services, Some(cache))
}

fn rows_line_body(
    base: &Path, body: &str, l: &Listing, metas: &[Meta], window: Window, services: Services<'_>,
    mut cache: Option<&mut super::searchrowcache::Cache>,
) -> String {
    let Window { start, ms } = window;
    let Services { mime, icons, aliases, thumbs, kinds } = services;
    let kind_revision = kinds.refresh();
    if let Some(cache) = cache.as_deref_mut() { cache.kind_revision(kind_revision); }
    // Field names stay one character: a window serialises on every scroll.
    let mut s = String::with_capacity(metas.len() * ROW_BYTES_ESTIMATE);
    s.push_str(&format!(r#"{{"t":"rows","start":{},"rows":["#, start));
    // Tags are read for exactly the rows a window emits, and never on a network or phone mount, where each read is a round trip.
    let tag_ok = !matches!(base_class(base, body), "network" | "phone");
    // A type with no POSIX modes reports the mount's 0755 for every file, so its execute bits say nothing; Row.p keeps the reported mode.
    let gvfs_modes = crate::backend::gvfsroot::gvfs_modes_are_real(base);
    let mut device_modes = HashMap::new();
    let mut lexical_modes: HashMap<(u64, &str), bool> = HashMap::new();
    // Clamped like stat_range, so a stale start cannot panic the backend.
    let n = metas.len().min(l.len().saturating_sub(start));
    // Distinct Kind strings this one response uses, so the per-row cost is an integer, not a repeated string.
    let mut kind_index: HashMap<String, usize> = HashMap::new();
    let mut kind_list: Vec<String> = Vec::new();
    for (i, m) in metas[..n].iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let name = l.name(start + i);
        // Files are reached by the exact path; the kind is read off the lossy spelling, where the extension is.
        let (path, shown) = (l.path(base, start + i), flea::bytename::display(name));
        if let Some((row, kind)) = cache.as_deref().and_then(|cache| cache.rendered(&path)) {
            let k = match kind_index.get(kind) {
                Some(&idx) => idx,
                None => {
                    let idx = kind_list.len();
                    kind_index.insert(kind.clone(), idx);
                    kind_list.push(kind.clone());
                    idx
                }
            };
            // Only the response-local kind index changes when a cached row moves to another window.
            let at = row.find(",\"k\":").expect("cached row kind") + 5;
            let end = at + row[at..].bytes().take_while(u8::is_ascii_digit).count();
            s.push_str(&row[..at]);
            s.push_str(&k.to_string());
            s.push_str(&row[end..]);
            continue;
        }
        let gvfs_modes = if name.contains('/') {
            crate::backend::gvfsroot::gvfs_modes_are_real(&path)
        } else {
            gvfs_modes
        };
        // The stat's dev names the filesystem through symlinks; unlisted devices cache lexical decisions by device and parent without another stat.
        let real_modes = gvfs_modes.unwrap_or_else(|| {
            device_modes
                .entry(m.dev)
                .or_insert_with(|| mountinfo::modes_are_real_for_dev(m.dev, body))
                .unwrap_or_else(|| {
                    let parent = name.rfind('/').map_or("", |at| &name[..at.max(1)]);
                    *lexical_modes.entry((m.dev, parent)).or_insert_with(|| {
                        #[cfg(test)]
                        mount_tests::MODE_PATH_BUILDS.with(|builds| builds.set(builds.get() + 1));
                        mountinfo::modes_are_real(&path, body)
                    })
                })
        });
        let is_dir = l.is_dir(start + i);
        // Looked up once and shared with the icon and the Kind text, because it is the only per-row allocation here.
        let mime_name = mime.lookup(&shown);
        // A directory or a special file is never thumbnailed, so the spec lookup is skipped for one; see AGENTS.md "Thumbnail requests".
        let can_thumb = thumbnailable(m.mode)
            && (mime_name.is_some_and(|m| thumbs.for_mime(m, aliases).is_some())
                || super::model3d::embedded_thumbnail_supported(&shown));
        // Only a thumbnailable AppImage carries a, so Images mode can draw its own icon without admitting every executable.
        let appimage = if can_thumb && mime_name.is_some_and(|m| appicon::is_appimage(m, aliases)) {
            r#","a":true"#
        } else {
            ""
        };
        // A symlink to a directory carries d:false by contract, so only the ICON follows the target.
        let folder_icon = is_dir || m.target_is_dir;
        let icon_mode = if real_modes {
            m.mode
        } else {
            m.mode & !ANY_EXECUTE_BIT
        };
        let icon = icons.icon_for(mime_name, folder_icon, icon_mode);
        let kind = kind_for(folder_icon, mime_name, icon, aliases, kinds);
        let k = match kind_index.get(&kind) {
            Some(&idx) => idx,
            None => {
                let idx = kind_list.len();
                kind_index.insert(kind.clone(), idx);
                kind_list.push(kind.clone());
                idx
            }
        };
        // Only a directory carries its filesystem id, because only a directory can be a drop
        // destination: canDrop requires d true, so a file row's device would never be read. On the
        // 100k scale fixture, which holds no directories at all, this adds nothing whatsoever.
        let dev = if is_dir { format!(r#","v":{}"#, m.dev) } else { String::new() };
        // Only a symlink carries a target, the same conditional shape v uses, and never both: a symlink's d is false.
        let link = if m.target.is_empty() { String::new() } else { format!(r#","l":"{}""#, escape(&m.target)) };
        // Only a tagged row carries tg, its names in stored order and canonical case, the same conditional shape v and l use.
        let tg = if tag_ok {
            let names = tags::read(&path).unwrap_or_default();
            let mut seen = Vec::with_capacity(names.len());
            for tag in &names {
                if let Some(known) = tags::known(tag) {
                    if !seen.contains(&known) {
                        seen.push(known);
                    }
                }
            }
            if seen.is_empty() {
                String::new()
            } else {
                format!(r#","tg":[{}]"#, seen.iter().map(|t| format!(r#""{}""#, escape(t))).collect::<Vec<_>>().join(","))
            }
        } else {
            String::new()
        };
        // The icon is escaped like every other field: the escape is the contract, not an optimisation.
        let row_at = s.len();
        let outline = super::outline::row_fields(l, start + i);
        let born = m.birth.map_or_else(|| "null".to_string(), |time| time.to_string());
        let source = m.ino.map_or_else(String::new, |ino| format!(r#","source":"{}:{ino}:{}""#, m.dev, m.mode & super::meta::S_IFMT));
        s.push_str(&format!(
            r#"{{"n":"{}","d":{},"s":{},"m":{},"b":{},"p":{},"i":"{}","t":{},"k":{}{}{}{}{}{}{}{}}}"#,
            escape(name),
            is_dir,
            m.size,
            m.mtime,
            born,
            m.mode,
            escape(icon),
            can_thumb,
            k,
            source,
            link,
            dev,
            tg,
            appimage,
            outline,
            if real_modes { "" } else { r#","x":false"# }
        ));
        if let Some(cache) = cache.as_deref_mut() {
            cache.remember(&path, &s[row_at..], &kind);
        }
    }
    let kinds_json: String = kind_list.iter().map(|k| format!(r#""{}""#, escape(k))).collect::<Vec<_>>().join(",");
    s.push_str(&format!(r#"],"kinds":[{}],"ms":{:.3}}}"#, kinds_json, ms));
    s
}

// A directory has no MIME type in this model, so its Kind is one hardcoded string, see docs/protocol.md "rows".
// A name no glob matched, and a type with no comment of its own, are both "a kind nothing here can
// describe". They read as Data rather than as an icon name, because an icon name is an internal
// string and the Kind column is a human description. Data is the canvas's own word for it.
pub const UNKNOWN_KIND: &str = "Data";

fn kind_for(folder_icon: bool, mime_name: Option<&str>, _icon: &str, aliases: &Aliases, kinds: &mut Kinds) -> String {
    if folder_icon {
        return "Folder".to_string();
    }
    match mime_name {
        Some(m) => kinds.comment(aliases.canonical(m)).unwrap_or_else(|| UNKNOWN_KIND.to_string()),
        None => UNKNOWN_KIND.to_string(),
    }
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rows_mount_tests.rs"]
mod mount_tests;

#[cfg(test)]
#[path = "rows_class_tests.rs"]
mod class_tests;
