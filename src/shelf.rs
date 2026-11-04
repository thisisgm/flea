// The drop shelf's own state, and the single-use token a drag out of it carries. The bar widget
// reads shelf.json and never writes it; every write is here, under the same lock discipline ui.json
// takes. DragOut rule 1: no row index crosses the process boundary, only a token bound to entries.
use crate::error::io_message;
use crate::jsondoc::{self, Json};
use crate::uistore;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DIR: &str = "omarchy/flea-shelf";
const PILE: &str = "shelf.json";
const DRAGS: &str = "drags.json";
const DRAGS_LOCK: &str = "drags.json.lock";
const PILE_LOCK: &str = "shelf.json.lock";

pub struct Shelf {
    pile: PathBuf,
    pile_lock: PathBuf,
    // The drag half of the shelf is src/shelfdrag.rs, which is where these two are used.
    pub(crate) drags: PathBuf,
    pub(crate) lock: PathBuf,
}

impl Shelf {
    pub fn user() -> Result<Shelf, String> {
        Ok(Shelf::at(&uistore::state_home()?))
    }

    pub fn at(state_dir: &Path) -> Shelf {
        let dir = state_dir.join(DIR);
        Shelf {
            pile: dir.join(PILE),
            pile_lock: dir.join(PILE_LOCK),
            drags: dir.join(DRAGS),
            lock: dir.join(DRAGS_LOCK),
        }
    }

    // Read by this file's own tests and by Settings > Plugins, which lists the pins.
    pub fn pile_file(&self) -> &Path {
        &self.pile
    }

    // Never fails: the bar draws an empty shelf for a file that is missing or unreadable, and so does this.
    #[cfg(test)]
    pub fn pile(&self) -> Vec<String> {
        let text = match fs::read_to_string(&self.pile) {
            Ok(text) => text,
            Err(_) => return Vec::new(),
        };
        let doc = match jsondoc::parse(&text) {
            Ok(doc) => doc,
            Err(_) => return Vec::new(),
        };
        let items = match doc.get("items").and_then(Json::as_array) {
            Some(items) => items,
            None => return Vec::new(),
        };
        items.iter().filter_map(|item| item.get("path").and_then(Json::as_str)).map(String::from).collect()
    }

    // Rule 3: the shelf changes only after completion. A copy leaves every reference, a move removes
    // the ones the transfer engine reported moved, and a failure or a cancel leaves the rest. Rule
    // 10: a pinned row is never consumed, so its entry is re-pointed by `repoint` instead.
    pub fn settle(&self, moved: &[String]) -> Result<(), String> {
        if moved.is_empty() {
            return Ok(());
        }
        self.write_pile(|items| {
            items
                .into_iter()
                .filter(|item| match item.get("path").and_then(Json::as_str) {
                    Some(path) => !moved.iter().any(|gone| gone == path) || is_pinned(item),
                    None => false,
                })
                .collect()
        })
    }

    // Which of the paths the shelf holds are pinned, which is what a move has to re-point.
    pub fn pinned_among(&self, paths: &[String]) -> Vec<String> {
        let doc = crate::persistdoc::read_pile(&self.pile).unwrap_or_else(|_| crate::persistdoc::empty());
        let items: Vec<Json> = doc.get("items").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
        items
            .iter()
            .filter(|item| is_pinned(item))
            .filter_map(|item| item.get("path").and_then(Json::as_str))
            .filter(|path| paths.iter().any(|asked| asked == path))
            .map(String::from)
            .collect()
    }

    // Main rule 10: a pinned row is always there. The flag rides the pile's own entry, so the bar's
    // reader needs no second file, and pinning a path the shelf is not holding puts it on first.
    // An unpin never stats the file: a pinned row whose file was deleted outside Flea is exactly the
    // row that has to be unpinnable, and settle will not drop it while the flag is on.
    pub fn pin(&self, paths: &[String], pinned: bool) -> Result<(), String> {
        let mut entries = Vec::new();
        for path in paths {
            match item_of(path) {
                Ok(entry) => entries.push(entry),
                Err(e) if pinned => return Err(e),
                Err(_) => entries.push(Json::Obj(vec![
                    ("path".to_string(), Json::Str(absolute_or(path))),
                    ("folder".to_string(), Json::Bool(false)),
                ])),
            }
        }
        self.write_pile(move |mut items| {
            for entry in entries {
                let path = entry.get("path").and_then(Json::as_str).map(String::from);
                let mut held = false;
                for item in items.iter_mut() {
                    if item.get("path").and_then(Json::as_str).map(String::from) != path {
                        continue;
                    }
                    held = true;
                    *item = with_pinned(item, pinned);
                }
                if !held && pinned {
                    items.push(with_pinned(&entry, true));
                }
            }
            items
        })
    }

    // SettingsRest rule 4: the panel orders the pins, which is a move within the pile's own pinned
    // entries; the loose entries keep their places.
    pub fn order(&self, path: &str, to: usize) -> Result<(), String> {
        let wanted = path.to_string();
        self.write_pile(move |items| {
            let slots: Vec<usize> = (0..items.len()).filter(|at| is_pinned(&items[*at])).collect();
            let named = |at: &&usize| items[**at].get("path").and_then(Json::as_str) == Some(wanted.as_str());
            let from = match slots.iter().position(|at| named(&at)) {
                Some(at) if to < slots.len() => at,
                _ => return items,
            };
            let mut pinned: Vec<Json> = slots.iter().map(|at| items[*at].clone()).collect();
            let moved = pinned.remove(from);
            pinned.insert(to, moved);
            let mut out = items;
            for (slot, item) in slots.into_iter().zip(pinned) {
                out[slot] = item;
            }
            out
        })
    }

    // Rule 10 again: Move on a pinned row moves the file and the pin follows it to the new path.
    pub fn repoint(&self, moved: &[(String, String)]) -> Result<(), String> {
        if moved.is_empty() {
            return Ok(());
        }
        self.write_pile(|items| {
            items
                .into_iter()
                .map(|item| {
                    let path = item.get("path").and_then(Json::as_str).unwrap_or_default().to_string();
                    match moved.iter().find(|(from, _)| *from == path) {
                        Some((_, to)) => with_path(&item, to),
                        None => item,
                    }
                })
                .collect()
        })
    }

    // Actions rule 6: Add to shelf is the same call a drop makes, and a path the shelf already holds
    // is not added twice, because what the shelf holds is a reference and not a copy.
    pub fn add(&self, paths: &[String]) -> Result<(), String> {
        let mut entries = Vec::new();
        for path in paths {
            entries.push(item_of(path)?);
        }
        self.write_pile(move |mut items| {
            for entry in entries {
                let path = entry.get("path").and_then(Json::as_str).map(String::from);
                let held = items
                    .iter()
                    .any(|item| item.get("path").and_then(Json::as_str).map(String::from) == path);
                if !held {
                    items.push(entry);
                }
            }
            items
        })
    }

    // Summon: clearing takes the whole pile out and hands it back, so the caller can keep it as the
    // last pile. Read and write are one locked step, or a drop landing meanwhile would be lost.
    pub fn clear(&self) -> Result<Vec<Json>, String> {
        let mut taken = Vec::new();
        self.write_pile(|items| {
            taken = items;
            Vec::new()
        })?;
        Ok(taken)
    }

    // And putting one back: what the pile was comes back, so a pile that was not empty becomes the
    // last pile in its turn rather than disappearing under the one being restored.
    pub fn put(&self, pile: Vec<Json>) -> Result<Vec<Json>, String> {
        let mut was = Vec::new();
        self.write_pile(|items| {
            was = items;
            pile
        })?;
        Ok(was)
    }

    // Every write of the pile goes through one lock, so a click that adds and a move that settles
    // cannot land on top of each other.
    fn write_pile(&self, change: impl FnOnce(Vec<Json>) -> Vec<Json>) -> Result<(), String> {
        let dir = self.pile.parent().ok_or("the shelf has no folder to write in")?;
        uistore::make_dir(dir)?;
        let lock = uistore::take_lock(&self.pile_lock)?;
        let held = self.held()?;
        let next = Json::Obj(vec![("items".to_string(), Json::Arr(change(held)))]);
        let written = uistore::replace(&self.pile, &jsondoc::render(&next));
        uistore::unlock(&lock).map_err(|e| format!("{} could not be unlocked: {}", self.pile_lock.display(), io_message(&e)))?;
        written
    }

    // Sample input, the whole of shelf.json:
    // {"items":[{"path":"/home/gm/Work/a.txt","folder":false,"pinned":true}]}
    // Absent, empty or blank is an empty pile; anything unreadable, unparsable or not a pile is never written over.
    pub(crate) fn held(&self) -> Result<Vec<Json>, String> {
        let doc = crate::persistdoc::read_pile(&self.pile)?;
        Ok(doc.get("items").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default())
    }

}

pub fn is_pinned(item: &Json) -> bool {
    item.get("pinned").and_then(Json::as_bool) == Some(true)
}

// An entry with one field changed, because a pile entry is rewritten rather than edited in place.
fn with_pinned(item: &Json, pinned: bool) -> Json {
    rebuilt(item, |name| name != "pinned", vec![("pinned".to_string(), Json::Bool(pinned))])
}

fn with_path(item: &Json, path: &str) -> Json {
    rebuilt(item, |name| name != "path", vec![("path".to_string(), Json::Str(path.to_string()))])
}

fn rebuilt(item: &Json, keep: impl Fn(&str) -> bool, mut added: Vec<(String, Json)>) -> Json {
    let mut fields: Vec<(String, Json)> = match item {
        Json::Obj(fields) => fields.iter().filter(|(name, _)| keep(name)).cloned().collect(),
        _ => Vec::new(),
    };
    fields.append(&mut added);
    Json::Obj(fields)
}

// std::path::absolute needs Rust 1.79 over the 1.77 floor; this joins cwd, drops CurDir, and keeps // and one trailing slash.
pub(crate) fn absolute(path: &str) -> std::io::Result<PathBuf> {
    let joined = Path::new(path);
    if joined.as_os_str().is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "an empty path has no absolute form"));
    }
    let joined = if joined.is_absolute() { joined.to_path_buf() } else { std::env::current_dir()?.join(joined) };
    let raw = joined.as_os_str().as_bytes();
    let double_slash = raw.starts_with(b"//") && raw.get(2) != Some(&b'/');
    let trailing_slash = raw.last() == Some(&b'/');
    let mut out = if double_slash { PathBuf::from("//") } else { PathBuf::new() };
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::RootDir if double_slash => {}
            other => out.push(other),
        }
    }
    if trailing_slash && !out.as_os_str().as_bytes().ends_with(b"/") {
        out.as_mut_os_string().push("/");
    }
    Ok(out)
}

// A pile entry: the path the shelf holds and whether the card draws it as a folder. The size is not
// recorded, because the card asks for it per drawn row and a folder's answer goes stale on its own.
fn item_of(path: &str) -> Result<Json, String> {
    let full = absolute(path).map_err(|e| format!("{} could not be read: {}", path, io_message(&e)))?;
    let meta = fs::symlink_metadata(&full).map_err(|e| format!("{} could not be read: {}", path, io_message(&e)))?;
    Ok(Json::Obj(vec![
        ("path".to_string(), Json::Str(full.to_string_lossy().to_string())),
        ("folder".to_string(), Json::Bool(meta.is_dir())),
    ]))
}

// The path as the pile spells it, and the caller's own spelling when it cannot be resolved at all.
fn absolute_or(path: &str) -> String {
    absolute(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.to_string())
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// Main rule 4's budget: the card asks about one path it is drawing, so this answers one path. A
// directory takes the listing's own bounded walk, which is why a floor comes back marked partial and
// the card draws it with the same > prefix a list row does.
pub fn size_of(path: &str) -> Result<(u64, bool), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{} could not be read: {}", path, io_message(&e)))?;
    if meta.is_dir() {
        let walked = crate::backend::dirsize::walk(Path::new(path));
        return Ok((walked.bytes, walked.partial));
    }
    Ok((meta.len(), false))
}

#[cfg(test)]
#[path = "shelf_tests.rs"]
mod tests;
