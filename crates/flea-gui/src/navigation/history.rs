//! Back and Forward stacks; an entry names the directory and the row its cursor stood on.

// Bounds a policy bug, not normal use, the same reason the folder maps carry a cap.
pub const MAX: usize = 500;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub index: usize,
}

impl Entry {
    pub fn new(path: impl Into<String>, name: impl Into<String>, index: usize) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            index,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack {
    Back,
    Forward,
}

// The landing a Back or Forward owes its listing, and the stack it pushed the drawn place onto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restore {
    pub entry: Entry,
    pub pushed_onto: Stack,
    pub locate: Option<String>,
    pub total: usize,
    pub fetched: bool,
}

#[derive(Default, Debug)]
pub struct History {
    pub back: Vec<Entry>,
    pub forward: Vec<Entry>,
}

impl History {
    pub fn stack(&mut self, which: Stack) -> &mut Vec<Entry> {
        match which {
            Stack::Back => &mut self.back,
            Stack::Forward => &mut self.forward,
        }
    }

    pub fn push(&mut self, which: Stack, entry: Entry) {
        let stack = self.stack(which);
        stack.push(entry);
        if stack.len() > MAX {
            stack.remove(0);
        }
    }

    pub fn paths(&self, which: Stack) -> Vec<&str> {
        match which {
            Stack::Back => &self.back,
            Stack::Forward => &self.forward,
        }
        .iter()
        .map(|entry| entry.path.as_str())
        .collect()
    }
}

// A bookmark can carry one trailing slash, which no row name ever has; "/" alone keeps its own.
pub fn trim_slash(path: &str) -> &str {
    if path.len() > 1 && path.ends_with('/') {
        &path[..path.len() - 1]
    } else {
        path
    }
}

pub fn parent_of(path: &str) -> &str {
    let text = trim_slash(path);
    // A plugin place's root is its own parent, as "/" is, and its first level climbs to that root.
    if let Some(rest) = text.strip_prefix("plugin://") {
        match rest.find('/') {
            None => return path,
            Some(slash) if !rest[slash + 1..].contains('/') => {
                return &path[.."plugin://".len() + slash + 1];
            }
            Some(_) => {}
        }
    }
    match text.rfind('/') {
        Some(0) | None => "/",
        Some(cut) => &text[..cut],
    }
}

pub fn leaf_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(cut) if cut + 1 < path.len() => &path[cut + 1..],
        _ => path,
    }
}

pub fn join(base: &str, name: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

// Which row of a held window a full target path names, by its exact listed name.
pub fn select_match<'a>(
    names: impl IntoIterator<Item = &'a str>,
    target: &str,
    folder: &str,
) -> Option<usize> {
    let leaf = if folder.len() <= 1 {
        target.strip_prefix('/')?
    } else {
        target.strip_prefix(folder)?.strip_prefix('/')?
    };
    names.into_iter().position(|name| name == leaf)
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
