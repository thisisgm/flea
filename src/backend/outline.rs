//! Session expansions compose ordinary directory listings into one row numbering.
use super::{dirsize::DirSize, listing::Listing, state::State};
use crate::jsondoc::Json;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Session {
    pub open: BTreeSet<PathBuf>,
    pub disabled: bool,
    pub last: String,
}

// Sample input: {"c":"list","path":"/home/gm","first":350,"outlinePaths":["/home/gm/A"]}
fn object(line: &str) -> Vec<(String, Json)> {
    match crate::jsondoc::parse(line) {
        Ok(Json::Obj(fields)) => fields,
        _ => Vec::new(),
    }
}

fn set(fields: &mut Vec<(String, Json)>, key: &str, value: Json) {
    fields.retain(|(name, _)| name != key);
    fields.push((key.into(), value));
}

// render puts each member on its own line and escapes every string, so trimming lines drops only layout.
fn encode(fields: Vec<(String, Json)>) -> String {
    crate::jsondoc::render(&Json::Obj(fields))
        .lines()
        .map(str::trim)
        .collect()
}

impl Session {
    pub fn list(&mut self, line: &str) -> String {
        self.last = line.to_owned();
        let mut fields = object(line);
        let paths = if self.disabled {
            Vec::new()
        } else {
            self.open
                .iter()
                .map(|path| Json::Str(path.to_string_lossy().into_owned()))
                .collect()
        };
        set(&mut fields, "outlinePaths", Json::Arr(paths));
        encode(fields)
    }

    // Sample input: {"c":"sort","by":"size","desc":true,"foldersFirst":true,"anchor":"/home/gm/A"}
    pub fn sort(&mut self, line: &str) {
        let mut fields = object(&self.last);
        for (key, value) in object(line) {
            if matches!(
                key.as_str(),
                "by" | "desc" | "foldersFirst" | "groupByKind" | "hiddenLast"
            ) {
                set(&mut fields, &key, value);
            }
        }
        self.last = encode(fields);
    }
}

pub fn request(
    st: &mut State,
    rows: &[usize],
    anchor: Option<usize>,
    open: bool,
    enabled: Option<bool>,
    first: usize,
) -> Result<String, String> {
    if st.held != super::state::Held::List {
        return Err("open in place needs a folder listing".into());
    }
    let anchor = if let Some(&named) = rows.first() {
        // Marked rows outside the window's held rows arrive unchecked; a file or symlink is never is_dir, and with no folder among them the anchor's folder acts.
        let folder = |&row: &usize| row < st.listing.len() && st.listing.is_dir(row);
        let mut folders: Vec<usize> = rows.iter().copied().filter(folder).collect();
        if folders.is_empty() {
            folders.extend(anchor.filter(folder));
        }
        let Some(&lead) = folders.first() else {
            return Err(format!("row {named} is not a folder in this listing"));
        };
        for &row in &folders {
            let path = st.base.join(st.listing.name(row));
            if open {
                st.outline.open.insert(path);
            } else {
                st.outline.open.remove(&path);
            }
        }
        let at = st.listing.name(anchor.filter(|&row| row < st.listing.len()).unwrap_or(lead));
        // The re-list keeps the window's cursor row; a close that hides it selects the outermost folder closing over it.
        let select = folders
            .iter()
            .map(|&row| st.listing.name(row))
            .filter(|name| !open && at.strip_prefix(name).is_some_and(|rest| rest.starts_with('/')))
            .min_by_key(|name| name.len())
            .unwrap_or(at);
        Some(st.base.join(select))
    } else {
        let Some(enabled) = enabled else {
            return Err("an outline request names a row or enabled".into());
        };
        st.outline.disabled = !enabled;
        None
    };
    let mut fields = object(&st.outline.last);
    set(&mut fields, "c", Json::Str("list".into()));
    set(
        &mut fields,
        "path",
        Json::Str(st.base.to_string_lossy().into_owned()),
    );
    set(&mut fields, "first", Json::Num(first.to_string()));
    // The last listing's select is stale here, and a select starts the first window at its row.
    fields.retain(|(name, _)| name != "select");
    if let Some(anchor) = anchor {
        set(
            &mut fields,
            "select",
            Json::Str(anchor.to_string_lossy().into_owned()),
        );
    }
    Ok(encode(fields))
}

fn append(out: &mut Listing, from: &Listing, index: usize, prefix: &str) -> usize {
    let name = format!("{prefix}{}", from.name(index));
    let off = out.names.len() as u32;
    let row = out.push_entry(&name, from.is_dir(index), from.spans[index].is_symlink);
    if let Some(meta) = from.gio_for(index) {
        out.gio_meta.push(super::listing::GioMeta {
            name_off: off,
            ..*meta
        });
    }
    let target = from.gio_target(from.spans[index].off);
    if !target.is_empty() {
        out.gio_targets.push(super::listing::GioTarget {
            name_off: off,
            target: target.to_owned(),
        });
    }
    row
}

pub fn expand(
    listing: &mut Listing,
    base: &Path,
    hidden: bool,
    line: &str,
    mime: std::sync::Arc<super::mime::Db>,
    fd: i32,
    tx: super::events::Sender<super::events::Event>,
) -> (f64, f64) {
    // Sample input: {"c":"list","path":"/home/gm","outlinePaths":["/home/gm/A","/home/gm/A/B","/srv/C"]}
    let paths: BTreeSet<PathBuf> = crate::json::field_str_array(line, "outlinePaths")
        .into_iter()
        .map(PathBuf::from)
        .collect();
    // The session remembers folders open elsewhere, and a listing holding none of them is left as it was read.
    if !paths.iter().any(|path| path.parent() == Some(base)) {
        return (0., 0.);
    }
    let mut out = Listing::new();
    let (mut read_ms, mut sort_ms) = (0., 0.);
    out.base_dev = listing.base_dev;
    for index in 0..listing.len() {
        append(&mut out, listing, index, "");
        let path = base.join(listing.name(index));
        if !listing.is_dir(index) || !paths.contains(&path) {
            continue;
        }
        match super::iomount::list_dir(
            path.to_string_lossy().into_owned(),
            hidden,
            0,
            line.to_owned(),
            mime.clone(),
            fd,
            tx.clone(),
            &super::iomount::mount_body(),
        ) {
            Ok(child) => {
                read_ms += child.read_ms;
                sort_ms += child.sort_ms;
                if watch_refused(fd, child.watch_wd) {
                    eprintln!(
                        "flea: {} will not follow outside changes, inotify refused a watch on it",
                        path.display()
                    );
                }
                out.outline_open.insert(listing.name(index).to_owned());
                out.outline_watches.push(child.watch_wd);
                out.outline_watches
                    .extend(child.listing.outline_watches.iter().copied());
                let prefix = format!("{}/", listing.name(index));
                for i in 0..child.listing.len() {
                    append(&mut out, &child.listing, i, &prefix);
                }
                for name in child.listing.outline_open {
                    out.outline_open.insert(format!("{prefix}{name}"));
                }
            }
            // A folder that cannot be read stays closed; the listed folder and its other open folders still land.
            Err(error) => {
                let _ = tx.send(super::events::Event::AbandonWatch(error.watch_wd));
                eprintln!("flea: {} stays closed: {}", path.display(), error.error.msg);
            }
        }
    }
    *listing = out;
    (read_ms, sort_ms)
}

// The same refusal the listed folder reports: inotify works, yet this open folder got no watch.
pub fn watch_refused(fd: i32, wd: i32) -> bool {
    fd >= 0 && wd < 0
}

pub fn row_fields(listing: &Listing, index: usize) -> String {
    let name = listing.name(index);
    if listing.outline_open.is_empty() {
        return String::new();
    }
    format!(
        ",\"depth\":{},\"open\":{}",
        name.bytes().filter(|&c| c == b'/').count(),
        listing.outline_open.contains(name)
    )
}

pub fn ordered(
    listing: &mut Listing,
    base: &Path,
    mime: &super::mime::Db,
    line: &str,
) -> Result<(f64, f64, Vec<Option<DirSize>>), &'static str> {
    let mut groups = std::collections::BTreeMap::<String, Listing>::new();
    for i in 0..listing.len() {
        let name = listing.name(i);
        // Sample input: "A", "A/b.txt" and "A/B/c.txt", grouped under "", "A" and "A/B".
        let (parent, leaf) = name.rsplit_once('/').unwrap_or(("", name));
        let group = groups.entry(parent.to_owned()).or_insert_with(Listing::new);
        let before = group.names.len();
        let row = append(group, listing, i, "");
        let span = &mut group.spans[row];
        span.off += (name.len() - leaf.len()) as u32;
        span.len = leaf.len() as u32;
        // Offsets remain stable for cached GIO metadata while the span names only its leaf.
        for meta in &mut group.gio_meta {
            if meta.name_off == before as u32 {
                meta.name_off = span.off;
            }
        }
        for target in &mut group.gio_targets {
            if target.name_off == before as u32 {
                target.name_off = span.off;
            }
        }
    }
    let mut pass = 0.;
    let mut sort = 0.;
    let mut seeds = std::collections::BTreeMap::<String, Vec<Option<DirSize>>>::new();
    for (parent, group) in &mut groups {
        let (p, s, seed) = super::ordering::request(group, &base.join(parent), mime, line)?;
        pass += p;
        sort += s;
        seeds.insert(parent.clone(), seed);
    }
    // Each group's walked sizes follow its rows into the one numbering, so the sizes answered after the sort need no walk of their own.
    fn flatten(
        parent: &str,
        groups: &std::collections::BTreeMap<String, Listing>,
        seeds: &std::collections::BTreeMap<String, Vec<Option<DirSize>>>,
        out: &mut Listing,
        sized: &mut Vec<Option<DirSize>>,
    ) {
        let Some(group) = groups.get(parent) else {
            return;
        };
        let prefix = if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        };
        for i in 0..group.len() {
            append(out, group, i, &prefix);
            sized.push(seeds.get(parent).and_then(|seed| seed.get(i).copied().flatten()));
            flatten(&format!("{prefix}{}", group.name(i)), groups, seeds, out, sized);
        }
    }
    let mut out = Listing::new();
    let mut sized = Vec::new();
    out.base_dev = listing.base_dev;
    out.outline_open = listing.outline_open.clone();
    out.outline_watches = listing.outline_watches.clone();
    flatten("", &groups, &seeds, &mut out, &mut sized);
    *listing = out;
    if sized.iter().all(Option::is_none) {
        sized.clear();
    }
    Ok((pass, sort, sized))
}

pub fn watched(listing: &Listing, base: &Path, path: &Path) -> bool {
    path == base
        || path
            .strip_prefix(base)
            .ok()
            .and_then(Path::to_str)
            .is_some_and(|name| listing.outline_open.contains(name))
}
