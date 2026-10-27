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
use std::collections::HashMap;
use std::path::Path;

// A row is a short name, two flags, three numbers and an icon name, so this rarely reallocates.
const ROW_BYTES_ESTIMATE: usize = 128;

pub fn rows_line(
    base: &Path,
    l: &Listing,
    metas: &[Meta],
    start: usize,
    ms: f64,
    mime: &Db,
    icons: &Names,
    aliases: &Aliases,
    thumbs: &Thumbnailers,
    kinds: &mut Kinds,
) -> String {
    // A gvfs path names its own class and reads no mount table; every other base reads it once, here.
    let body = if gvfs_class(base).is_some() { String::new() } else { mountinfo::read_body() };
    rows_line_in(base, &body, l, metas, start, ms, mime, icons, aliases, thumbs, kinds)
}

// The window against a mount table the caller read, so a test can stand an exFAT or btrfs mount where it cannot make one.
#[allow(clippy::too_many_arguments)]
fn rows_line_in(
    base: &Path,
    body: &str,
    l: &Listing,
    metas: &[Meta],
    start: usize,
    ms: f64,
    mime: &Db,
    icons: &Names,
    aliases: &Aliases,
    thumbs: &Thumbnailers,
    kinds: &mut Kinds,
) -> String {
    // Field names stay one character: a window serialises on every scroll.
    let mut s = String::with_capacity(metas.len() * ROW_BYTES_ESTIMATE);
    s.push_str(&format!(r#"{{"t":"rows","start":{},"rows":["#, start));
    // Clamped like stat_range, so a stale start cannot panic the backend.
    let n = metas.len().min(l.len().saturating_sub(start));
    let indices: Vec<usize> = (start..start + n).collect();
    let kind_list = push_rows(&mut s, base, body, l, &metas[..n], &indices, false, mime, icons, aliases, thumbs, kinds);
    finish_line(&mut s, &kind_list, ms);
    s
}

// A filter window's rows (issue 315): the row shape above, each carrying "r", the listing row it is,
// because the rows a filter shows are a set and not a run, and start here is a view position.
#[allow(clippy::too_many_arguments)]
pub fn frows_line(
    base: &Path,
    l: &Listing,
    metas: &[Meta],
    indices: &[usize],
    start: usize,
    ms: f64,
    mime: &Db,
    icons: &Names,
    aliases: &Aliases,
    thumbs: &Thumbnailers,
    kinds: &mut Kinds,
) -> String {
    let body = if gvfs_class(base).is_some() { String::new() } else { mountinfo::read_body() };
    let n = metas.len().min(indices.len());
    let mut s = String::with_capacity(n * ROW_BYTES_ESTIMATE);
    s.push_str(&format!(r#"{{"t":"frows","start":{},"rows":["#, start));
    let kind_list = push_rows(&mut s, base, &body, l, &metas[..n], &indices[..n], true, mime, icons, aliases, thumbs, kinds);
    finish_line(&mut s, &kind_list, ms);
    s
}

fn finish_line(s: &mut String, kind_list: &[String], ms: f64) {
    let kinds_json: String = kind_list.iter().map(|k| format!(r#""{}""#, escape(k))).collect::<Vec<_>>().join(",");
    s.push_str(&format!(r#"],"kinds":[{}],"ms":{:.3}}}"#, kinds_json, ms));
}

// The rows themselves, one per meta and named by listing row, and the Kind dictionary they index:
// shared by the window's rows line and the filter's frows line, whose rows also carry r.
#[allow(clippy::too_many_arguments)]
fn push_rows(
    s: &mut String,
    base: &Path,
    body: &str,
    l: &Listing,
    metas: &[Meta],
    indices: &[usize],
    with_row: bool,
    mime: &Db,
    icons: &Names,
    aliases: &Aliases,
    thumbs: &Thumbnailers,
    kinds: &mut Kinds,
) -> Vec<String> {
    // Tags are read for exactly the rows a window emits, and never on a network or phone mount, where each read is a round trip.
    let tag_ok = !matches!(extclass::classify_in(base, body), "network" | "phone");
    // A type with no POSIX modes reports the mount's 0755 for every file, so its execute bits say nothing; Row.p keeps the reported mode.
    let gvfs_modes = crate::backend::gvfsroot::gvfs_modes_are_real(base);
    let mut device_modes = HashMap::new();
    let mut lexical_modes: HashMap<(u64, &str), bool> = HashMap::new();
    // Distinct Kind strings this one response uses, so the per-row cost is an integer, not a repeated string.
    let mut kind_index: HashMap<String, usize> = HashMap::new();
    let mut kind_list: Vec<String> = Vec::new();
    for (i, m) in metas.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let row = indices[i];
        let name = l.name(row);
        let gvfs_modes = if name.contains('/') { crate::backend::gvfsroot::gvfs_modes_are_real(&base.join(name)) } else { gvfs_modes };
        // The stat's dev names the filesystem through symlinks; unlisted devices cache lexical decisions by device and parent without another stat.
        let real_modes = gvfs_modes.unwrap_or_else(|| {
            device_modes.entry(m.dev).or_insert_with(|| mountinfo::modes_are_real_for_dev(m.dev, body))
                .unwrap_or_else(|| {
                    let parent = name.rfind('/').map_or("", |at| &name[..at.max(1)]);
                    *lexical_modes.entry((m.dev, parent)).or_insert_with(|| {
                        #[cfg(test)]
                        mount_tests::MODE_PATH_BUILDS.with(|builds| builds.set(builds.get() + 1));
                        mountinfo::modes_are_real(&base.join(name), body)
                    })
                })
        });
        let is_dir = l.is_dir(row);
        // Looked up once and shared with the icon and the Kind text, because it is the only per-row allocation here.
        let mime_name = mime.lookup(name);
        // A directory or a special file is never thumbnailed, so the spec lookup is skipped for one; see AGENTS.md "Thumbnail requests".
        let can_thumb = thumbnailable(m.mode) && mime_name.is_some_and(|m| thumbs.for_mime(m, aliases).is_some());
        // Only a thumbnailable AppImage carries a, so Images mode can draw its own icon without admitting every executable.
        let appimage = if can_thumb && mime_name.is_some_and(|m| appicon::is_appimage(m, aliases)) { r#","a":true"# } else { "" };
        // A symlink to a directory carries d:false by contract, so only the ICON follows the target.
        let folder_icon = is_dir || m.target_is_dir;
        let icon_mode = if real_modes { m.mode } else { m.mode & !ANY_EXECUTE_BIT };
        let icon = icons.icon_for(mime_name, folder_icon, icon_mode);
        let kind = kind_for(folder_icon, mime_name, icon, aliases, kinds);
        let k = match kind_index.get(&kind) {
            Some(&idx) => idx,
            None => {
                let idx = kind_list.len();
                kind_index.insert(kind.clone(), idx);
                kind_list.push(kind);
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
            let names = tags::read(&base.join(name)).unwrap_or_default();
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
        s.push_str(&format!(
            r#"{{"n":"{}","d":{},"s":{},"m":{},"p":{},"i":"{}","t":{},"k":{}{}{}{}{}{}{}}}"#,
            escape(name),
            is_dir,
            m.size,
            m.mtime,
            m.mode,
            escape(icon),
            can_thumb,
            k,
            // Only a filter window's row names its listing row, right after k, where every reader of a rows line expects nothing new.
            if with_row { format!(r#","r":{}"#, row) } else { String::new() },
            link,
            dev,
            tg,
            appimage,
            if real_modes { "" } else { r#","x":false"# }
        ));
    }
    kind_list
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
