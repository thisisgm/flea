// The single-use token a drag out of the shelf carries, and the file it lives in. DragOut rule 1:
// no row index crosses the process boundary, only a token bound to the entries it was minted for.
use crate::error::io_message;
use crate::jsondoc::{self, Json};
use crate::shelf::{absolute, Shelf};
use crate::uistore;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;

// A drag is a gesture, not a session: a token older than this is refused however it was stored.
const TOKEN_LIFE_MS: u64 = 120_000;
// Sixteen bytes of urandom, hex encoded: the token is the only thing standing between a foreign
// process and a move, so it is not a counter and not the clock.
const TOKEN_BYTES: usize = 16;

// What a redeemed token asks for: the entries as they were at the lift, and the intent fixed there.
pub struct Redeemed {
    pub moving: bool,
    pub paths: Vec<String>,
}

impl Shelf {
    // Rule 1: the token is bound to the entries and to the intent, both fixed before the platform
    // loop starts, so nothing the drag passes through can change what is being asked for.
    pub fn drag_begin(&self, moving: bool, paths: &[String], now_ms: u64) -> Result<String, String> {
        if paths.is_empty() {
            return Err("a drag carries at least one entry".to_string());
        }
        let token = mint()?;
        let mut entries = Vec::new();
        for path in paths {
            entries.push(entry_of(path)?);
        }
        let record = Json::Obj(vec![
            ("token".to_string(), Json::Str(token.clone())),
            ("intent".to_string(), Json::Str(if moving { "move" } else { "copy" }.to_string())),
            ("created".to_string(), Json::Num(now_ms.to_string())),
            ("entries".to_string(), Json::Arr(entries)),
        ]);
        self.write_drags(|drags| {
            let mut kept = live_drags(drags, now_ms);
            kept.push(record.clone());
            Ok(Json::Obj(vec![("drags".to_string(), Json::Arr(kept))]))
        })?;
        Ok(token)
    }

    // Single use: the record is taken out of the file under the lock, so a replayed token finds
    // nothing. Rule 4: a token that is unknown, expired or already spent is refused outright.
    pub fn redeem(&self, token: &str, now_ms: u64) -> Result<Redeemed, String> {
        let mut found: Option<Json> = None;
        self.write_drags(|drags| {
            let mut kept = Vec::new();
            for drag in live_drags(drags, now_ms) {
                if drag.get("token").and_then(Json::as_str) == Some(token) && found.is_none() {
                    found = Some(drag);
                } else {
                    kept.push(drag);
                }
            }
            Ok(Json::Obj(vec![("drags".to_string(), Json::Arr(kept))]))
        })?;
        let record = found.ok_or_else(|| "that drag is not one this shelf started".to_string())?;
        let moving = record.get("intent").and_then(Json::as_str) == Some("move");
        let entries: Vec<Json> = record.get("entries").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
        let mut paths = Vec::new();
        for entry in &entries {
            paths.push(unchanged(entry)?);
        }
        Ok(Redeemed { moving, paths })
    }

    // Sample input, the whole of drags.json:
    // {"drags":[{"token":"9f2c","intent":"move","created":1789426925000,"entries":[{"path":"/a","dev":66305,"ino":12,"bytes":4}]}]}
    fn write_drags(&self, change: impl FnOnce(&Json) -> Result<Json, String>) -> Result<(), String> {
        let dir = self.drags.parent().ok_or("the shelf has no folder to write in")?;
        uistore::make_dir(dir)?;
        let lock = uistore::take_lock(&self.lock)?;
        let current = crate::persistdoc::read(&self.drags, crate::persistdoc::drags)?;
        let next = change(&current)?;
        let written = uistore::replace(&self.drags, &jsondoc::render(&next));
        uistore::unlock(&lock).map_err(|e| format!("{} could not be unlocked: {}", self.lock.display(), io_message(&e)))?;
        written
    }
}

// Every drag still inside its own lifetime, so an abandoned gesture cannot be redeemed later.
fn live_drags(drags: &Json, now_ms: u64) -> Vec<Json> {
    drags
        .get("drags")
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .filter(|drag| match drag.get("created").and_then(Json::as_f64) {
                    Some(created) => now_ms.saturating_sub(created as u64) < TOKEN_LIFE_MS,
                    None => false,
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

// The source identity the lift recorded, which is what makes a token name a file rather than a name.
fn entry_of(path: &str) -> Result<Json, String> {
    let full = absolute(path).map_err(|e| format!("{} could not be read: {}", path, io_message(&e)))?;
    let meta = fs::symlink_metadata(&full).map_err(|e| format!("{} could not be read: {}", path, io_message(&e)))?;
    Ok(Json::Obj(vec![
        ("path".to_string(), Json::Str(full.to_string_lossy().to_string())),
        ("dev".to_string(), Json::Num(meta.dev().to_string())),
        ("ino".to_string(), Json::Num(meta.ino().to_string())),
        ("bytes".to_string(), Json::Num(meta.len().to_string())),
    ]))
}

// The same file, or the drag is refused: a path that now names another inode is not what was lifted.
fn unchanged(entry: &Json) -> Result<String, String> {
    let path = entry.get("path").and_then(Json::as_str).ok_or("a drag entry with no path")?;
    let meta = fs::symlink_metadata(path).map_err(|_| format!("{} is no longer there", path))?;
    let same = number_of(entry, "dev") == Some(meta.dev().to_string())
        && number_of(entry, "ino") == Some(meta.ino().to_string());
    if !same {
        return Err(format!("{} is not the file the shelf was holding", path));
    }
    Ok(path.to_string())
}

// The record keeps the literal it was written with, so a comparison is a string one: an inode
// number above what an f64 holds exactly would otherwise compare equal to its neighbour.
fn number_of(entry: &Json, name: &str) -> Option<String> {
    match entry.get(name) {
        Some(Json::Num(text)) => Some(text.clone()),
        _ => None,
    }
}

fn mint() -> Result<String, String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    let mut source = fs::File::open("/dev/urandom").map_err(|e| format!("no randomness for a drag token: {}", io_message(&e)))?;
    source.read_exact(&mut bytes).map_err(|e| format!("short read of randomness: {}", io_message(&e)))?;
    Ok(bytes.iter().map(|b| format!("{:02x}", b)).collect())
}

#[cfg(test)]
#[path = "shelfdrag_tests.rs"]
mod tests;
