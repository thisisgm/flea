//! The open folder's volume went, or the folder or one above it was deleted or moved: the pane and every tab inside it land on the nearest folder still there.
use super::{
    model::Model,
    wire::{text, Wire},
};
use crate::jsondoc::Json;
use std::{
    io,
    path::{Path, PathBuf},
};

// Sample input: {"t":"gone","path":"/w/p/open-me","parent":"/w"} moves a pane on /w/p/open-me, and a tab on /w/p/other, to /w.
pub fn receive(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    let said = match text(value, "t") {
        "gone" => "tabs.folder_gone",
        "unmounted" => "dialogs.file.disconnected",
        _ => return Ok(false),
    };
    let (path, parent) = (
        Path::new(text(value, "path")),
        PathBuf::from(text(value, "parent")),
    );
    if !path.is_absolute() || !parent.is_absolute() {
        return Ok(true);
    }
    let Some(root) = vanished_root(path, &parent) else {
        return Ok(true);
    };
    for (i, tab) in m.tabs.iter_mut().enumerate() {
        if i != m.tab && tab.path.starts_with(&root) {
            tab.path = parent.clone();
            tab.cursor = 0;
        }
    }
    // A list still out inside the folder that went, an ancestor of path included, is answered by this line, or by this line again when it crossed it.
    if m.pending
        .as_deref()
        .is_some_and(|pending| pending.starts_with(&root))
    {
        m.pending = None;
        m.restore_navigation();
    }
    if m.pending.is_none() && m.path.starts_with(path) {
        m.open(parent, w)?;
        m.say_key(said);
    }
    Ok(true)
}

// Sample: "/w/p/open-me" gone to "/w" answers "/w/p", the folder under the parent that took the rest with it.
fn vanished_root(path: &Path, parent: &Path) -> Option<PathBuf> {
    let first = path.strip_prefix(parent).ok()?.components().next()?;
    Some(parent.join(first))
}
