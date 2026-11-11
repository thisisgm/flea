//! Opening a place and which rail row lights for what a pane shows.
use super::places::record_error;
use super::rail::{FavouriteError, Kind, RailEntry, RecordError};
use super::{decode_path, inside, normalize, under_gvfs};

// What opening a place does, decided once for the rail and the query alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Open {
    Trash,
    // Could not open %1 · %2: the row's label and the reason, a catalog key or an opaque diagnostic.
    Refused {
        label: String,
        reason: FavouriteError,
    },
    // A share that mounts first through the network host.
    Network {
        uri: String,
        label: String,
    },
    Path(String),
}

pub fn open_entry(entry: &RailEntry) -> Open {
    if entry.kind == Kind::Trash {
        return Open::Trash;
    }
    if entry.kind == Kind::Favourite {
        if let Some(reason) = entry
            .original
            .as_ref()
            .map_or(Some(RecordError::InvalidRecord), record_error)
        {
            return Open::Refused {
                label: entry.label.clone(),
                reason: FavouriteError::Record(reason),
            };
        }
    }
    // A plugin row keeps a remote or file:// opens in uri and leaves path empty.
    let target = if entry.path.is_empty() && entry.kind == Kind::PluginRow {
        &entry.uri
    } else {
        &entry.path
    };
    if target.contains("://") && !target.starts_with("file://") {
        return Open::Network {
            uri: target.clone(),
            label: entry.label.clone(),
        };
    }
    Open::Path(
        target
            .strip_prefix("file://")
            .map_or_else(|| target.clone(), decode_path),
    )
}

// Where the rail cursor lands after a rebuild; None clears it, because a changed duplicate count names no occurrence.
pub fn rail_cursor_after(
    before: &[RailEntry],
    after: &[RailEntry],
    index: Option<usize>,
) -> Option<usize> {
    if before.is_empty() && index == Some(0) {
        return Some(0);
    }
    let index = index.filter(|i| *i < before.len())?;
    let key = before[index].identity();
    let old: Vec<usize> = (0..before.len())
        .filter(|i| before[*i].identity() == key)
        .collect();
    let occurrence = old.iter().filter(|i| **i < index).count();
    let matches: Vec<usize> = (0..after.len())
        .filter(|i| after[*i].identity() == key)
        .collect();
    (matches.len() == old.len()).then(|| matches[occurrence])
}

// The pane's view as the rail lights it: a view row, or the folder with the remembered gvfs share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Trash,
    Recent,
    Tag(String),
    Dir { share_key: String },
}

// The row holding the folder or view the pane shows; under gvfs only the mounted row opened under its share lights.
pub fn held_by(entries: &[RailEntry], dir: &str, view: &View) -> Option<usize> {
    let share_key = match view {
        View::Dir { share_key } => share_key,
        View::Trash => return entries.iter().position(|e| e.kind == Kind::Trash),
        View::Recent => return entries.iter().position(|e| e.kind == Kind::Recent),
        View::Tag(tag) => {
            return entries
                .iter()
                .position(|e| e.kind == Kind::Tag && e.tag == Some(tag.as_str()))
        }
    };
    let gvfs = under_gvfs(dir);
    let mut best = None;
    let mut depth = 0;
    for (i, e) in entries.iter().enumerate() {
        if matches!(e.kind, Kind::Recent | Kind::Trash | Kind::Tag)
            || e.error.is_some()
            || e.mounted == Some(false)
        {
            continue;
        }
        if gvfs {
            if !share_key.is_empty()
                && e.path.is_empty()
                && (normalize(&e.uri) == *share_key || normalize(&e.mount_uri) == *share_key)
            {
                return Some(i);
            }
            continue;
        }
        if e.path.is_empty() {
            continue;
        }
        let holds = if e.path == "/" {
            dir.starts_with('/')
        } else {
            inside(dir, &e.path)
        };
        if holds && (best.is_none() || e.path.len() > depth) {
            best = Some(i);
            depth = e.path.len();
        }
    }
    best
}

// share_alias is the pane's remembered gvfs share key, None when it stands nowhere remote.
pub fn lit_view(
    trash_opened: bool,
    recent: bool,
    searching: bool,
    answered_query: &str,
    share_alias: Option<&str>,
) -> View {
    if trash_opened {
        return View::Trash;
    }
    if recent {
        return View::Recent;
    }
    if let Some(tag) = answered_query.strip_prefix("tag:").filter(|_| searching) {
        return View::Tag(tag.to_owned());
    }
    View::Dir {
        share_key: share_alias.unwrap_or("").to_owned(),
    }
}
